//! PID file + process liveness for the `digse start` daemon.
//!
//! The running server is tracked by a tiny TOML PID file written by the daemon
//! child right after it binds its listener (see [`crate::start::bind_and_record`]).
//! `digse ps` / `restart` / `stop` / `start` all consult it.
//!
//! Paths live next to the config file so `$DIGSE_CONFIG` is honored: if the
//! config points at `/x/y/config.toml`, the PID file is `/x/y/start.pid`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One recorded daemon instance. Serialized as TOML.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PidRecord {
    /// OS process id of the daemon child.
    pub pid: u32,
    /// Interface the listener is bound to.
    pub host: String,
    /// TCP port the listener is bound to.
    pub port: u16,
}

/// Directory holding both config and runtime state (`~/.digse`, or the parent of
/// `$DIGSE_CONFIG`).
pub fn state_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cfg = digse::DigseConfig::config_path()?;
    let parent = cfg
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
    Ok(parent.to_path_buf())
}

/// `<state_dir>/start.pid`.
pub fn pid_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(state_dir()?.join("start.pid"))
}

/// `<state_dir>/start.log` — appended to by the daemon child's stdout/stderr.
pub fn log_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(state_dir()?.join("start.log"))
}

/// Read the PID record, or `Ok(None)` if no PID file exists (or it is empty).
pub fn read() -> Result<Option<PidRecord>, Box<dyn std::error::Error>> {
    let path = pid_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    let record: PidRecord = toml::from_str(&text)?;
    Ok(Some(record))
}

/// Write the PID record, creating the state dir if needed.
pub fn write(record: &PidRecord) -> Result<(), Box<dyn std::error::Error>> {
    let path = pid_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(record)?;
    std::fs::write(&path, text)?;
    Ok(())
}

/// Delete the PID file if it exists (best-effort; missing file is not an error).
pub fn delete() -> Result<(), Box<dyn std::error::Error>> {
    let path = pid_file_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Outcome of probing the recorded daemon.
pub enum Liveness {
    /// Daemon is up; carries its record.
    Alive(PidRecord),
    /// No PID file, or the recorded PID is no longer our daemon (the stale file
    /// has been cleaned up).
    Stopped,
}

/// Read the PID file and decide whether the daemon is alive. A stale PID file
/// (process gone or PID reused) is deleted, and [`Liveness::Stopped`] is
/// returned — so every caller self-heals.
pub fn probe() -> Result<Liveness, Box<dyn std::error::Error>> {
    match read()? {
        Some(record) => {
            if is_our_process(record.pid) {
                Ok(Liveness::Alive(record))
            } else {
                let _ = delete();
                Ok(Liveness::Stopped)
            }
        }
        None => Ok(Liveness::Stopped),
    }
}

// --- process liveness (platform-specific) ---------------------------------
//
// `is_our_process` decides whether a recorded PID is still *our* daemon. The
// mechanism differs per OS but the contract is the same: return false for a
// missing/dead/reused PID so callers can self-heal stale PID files.

/// Linux: read `/proc/<pid>/cmdline` and require the hidden
/// `__start_foreground__` argv token, which no other process carries — that
/// alone defeats PID reuse. No signal syscalls are needed.
#[cfg(target_os = "linux")]
pub(crate) fn is_our_process(pid: u32) -> bool {
    if !std::path::Path::new(&format!("/proc/{}", pid)).exists() {
        return false;
    }
    let cmdline = match std::fs::read(format!("/proc/{}/cmdline", pid)) {
        Ok(b) => b,
        Err(_) => return false,
    };
    // `/proc/<pid>/cmdline` is NUL-separated argv; our marker is one whole arg.
    cmdline
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .any(|arg| arg == b"__start_foreground__")
}

/// macOS / other Unix: no `/proc`, so a `kill(pid, 0)` liveness probe and we
/// trust the PID file (same-uid only — `kill` returns EPERM for other users'
/// live processes, which we treat as "not ours"). Good enough for a
/// single-user local tool.
#[cfg(all(unix, not(target_os = "linux")))]
pub(crate) fn is_our_process(pid: u32) -> bool {
    // 0 == success, ESRCH == no such process, EPERM == exists but not ours.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Windows: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` then
/// `GetExitCodeProcess`; alive iff the exit code is `STILL_ACTIVE` (259).
#[cfg(windows)]
pub(crate) fn is_our_process(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;

    unsafe {
        let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if proc.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(proc, &mut code);
        CloseHandle(proc);
        ok != 0 && code == STILL_ACTIVE
    }
}
