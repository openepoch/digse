//! Daemon start/stop helpers shared by `digse start` / `restart` / `stop`.
//!
//! `start_server_background` re-execs this binary with the hidden
//! `__start_foreground__` subcommand as a fully detached child, and waits for
//! the child to bind its listener and write the PID file. `stop_server` kills
//! the recorded daemon and clears the PID file.
//!
//! The detach/kill mechanics differ per OS and live in [`spawn_detached`] /
//! [`stop_pid`]; the PID-file wait loop and bookkeeping are shared.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::pidfile;

/// How long the parent waits for the child to bind + write the PID file before
/// reporting failure (typically a port-in-use bind error).
const START_TIMEOUT: Duration = Duration::from_millis(6000);
/// Grace period after SIGTERM before escalating to SIGKILL (Unix only).
#[cfg(unix)]
const STOP_GRACE: Duration = Duration::from_millis(5000);

/// Spawn the server detached, wait for it to bind + write the PID file, and
/// return its pid. Errors (pointing at the log file) if it does not come up
/// within [`START_TIMEOUT`] — usually because the port was already in use.
///
/// Concurrency: concurrent `start`/`restart` calls can race past the live-check
/// before either writes a PID file; the loser's child then fails to bind and
/// exits, leaving the winner on the port. We do not lock the PID file — this is
/// best-effort and fine for a local tool.
pub fn start_server_background(host: &str, port: u16) -> Result<u32, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let log_path = pidfile::log_file_path()?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let log_err = log.try_clone()?;

    let mut cmd = Command::new(exe);
    cmd.arg("__start_foreground__")
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    let child_pid = spawn_detached(&mut cmd)?;

    // Wait for the child to bind and record its PID (portable — same on every
    // OS, since the foreground child writes the PID file via std fs).
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Ok(Some(rec)) = pidfile::read() {
            if rec.pid == child_pid {
                return Ok(child_pid);
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "digse start: server did not come up within {}ms (is the port in use? see {})",
        START_TIMEOUT.as_millis(),
        log_path.display()
    )
    .into())
}

/// Stop the running daemon. Returns `Ok(true)` if a daemon was running and is
/// now stopped, `Ok(false)` if nothing was running (PID file absent or stale —
/// stale files are cleaned up).
pub fn stop_server() -> Result<bool, Box<dyn std::error::Error>> {
    let rec = match pidfile::read()? {
        Some(r) => r,
        None => return Ok(false),
    };
    if !pidfile::is_our_process(rec.pid) {
        let _ = pidfile::delete();
        return Ok(false);
    }

    stop_pid(rec.pid);

    let _ = pidfile::delete();
    Ok(true)
}

// --- detach/kill mechanics (per OS) ---------------------------------------

/// Spawn `cmd` fully detached from this process — a new session on Unix (so the
/// daemon survives the parent and its controlling terminal), no console window
/// on Windows — and return the child PID without waiting on it.
#[cfg(unix)]
fn spawn_detached(cmd: &mut Command) -> Result<u32, Box<dyn std::error::Error>> {
    use std::os::unix::process::CommandExt;
    // Detach into a new session (setsid also makes the child its own process
    // group leader), so the daemon survives the parent and the controlling
    // terminal. `setsid` is the one syscall std does not expose; we do NOT also
    // set process_group(0), which would pre-empt setsid with EPERM.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    let pid = child.id();
    drop(child); // do not wait — let it run detached
    Ok(pid)
}

#[cfg(windows)]
fn spawn_detached(cmd: &mut Command) -> Result<u32, Box<dyn std::error::Error>> {
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS: this process has no console (does not inherit one and is
    // not given a new one tied to a terminal). CREATE_NO_WINDOW: even if a
    // console would otherwise be allocated, none is shown. Together they keep
    // the daemon — and the `digse start` that launches it — silent.
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    let child = cmd.spawn()?;
    let pid = child.id();
    drop(child); // do not wait — let it run detached
    Ok(pid)
}

/// Kill the recorded daemon PID. Unix escalates SIGTERM→SIGKILL; Windows uses
/// `TerminateProcess` (digse has no signal handler to drain gracefully there).
#[cfg(unix)]
fn stop_pid(pid: u32) {
    // Graceful: SIGTERM.
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline {
        if !pidfile::is_our_process(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Force: SIGKILL.
    if pidfile::is_our_process(pid) {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(windows)]
fn stop_pid(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    unsafe {
        let proc = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if proc.is_null() {
            return;
        }
        // TerminateProcess is a hard kill (exit code 1). No graceful phase in
        // v1: there is no Windows equivalent of SIGTERM the daemon listens for.
        TerminateProcess(proc, 1);
        // Let it actually exit so a subsequent is_our_process() reflects reality.
        std::thread::sleep(Duration::from_millis(200));
        CloseHandle(proc);
    }
}
