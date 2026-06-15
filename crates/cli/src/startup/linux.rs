//! Boot-time autostart via a systemd *user* service (Linux).
//!
//! `digse startup add` installs `~/.config/systemd/user/digse.service` whose
//! `ExecStart` runs `digse __start_foreground__` in the foreground — so systemd
//! supervises it directly, and `digse ps` still recognizes the instance (the
//! hidden argv marker is present either way). `add` also enables
//! `loginctl enable-linger`, which makes the user manager — and thus the unit —
//! start at BOOT, before login.
//!
//! `add` enables for boot but does not start now (to avoid a port clash with a
//! running `digse start` instance).

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// `digse startup status` — report whether autostart is configured and active.
pub fn status() -> Result<(), Box<dyn std::error::Error>> {
    let unit = unit_path()?;
    let present = unit.exists();

    println!("startup: autostart via systemd user service");
    println!(
        "unit file: {} ({})",
        unit.display(),
        if present { "present" } else { "absent" }
    );

    let (_, enabled, _) = run("systemctl", &["--user", "is-enabled", "digse"])?;
    let (_, active, _) = run("systemctl", &["--user", "is-active", "digse"])?;
    println!(
        "enabled (boot): {}",
        if enabled.is_empty() { "unknown" } else { &enabled }
    );
    println!(
        "active (now):   {}",
        if active.is_empty() { "unknown" } else { &active }
    );

    let linger = current_linger()?;
    println!(
        "linger:         {}",
        match linger.as_str() {
            "yes" => "yes (user manager starts at boot)",
            "" => "unknown",
            other => other,
        }
    );
    Ok(())
}

/// `digse startup add` — write the unit, enable it for boot, enable linger.
/// Does NOT start it now; prints how to start it (the user chooses, to avoid a
/// port clash with an already-running `digse start`).
pub fn add() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let cfg = digse::DigseConfig::load().unwrap_or_default();
    let host = cfg.serve.host.clone();
    let port = cfg.serve.port;

    let dir = unit_dir()?;
    std::fs::create_dir_all(&dir)?;
    let unit = unit_path()?;
    std::fs::write(&unit, unit_contents(&exe.display().to_string(), &host, port))?;

    println!("startup: wrote {}", unit.display());
    println!(
        "         ExecStart={} __start_foreground__ --host {} --port {}",
        exe.display(),
        host,
        port
    );

    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "digse"])?;
    println!("startup: enabled for boot");

    let user = current_user()?;
    run("loginctl", &["enable-linger", &user])
        .map_err(|e| format!("loginctl enable-linger failed: {}", e))?;
    println!("startup: linger enabled (user manager starts at boot)");

    println!();
    println!("Autostart is configured and will start at next boot.");
    println!("To start it now:  digse start");
    println!("  (or: systemctl --user start digse)");
    Ok(())
}

/// `digse startup remove` — stop + disable the unit, delete it, reload.
/// Linger is left untouched (it is shared with any other user units).
pub fn remove() -> Result<(), Box<dyn std::error::Error>> {
    let (_, active, _) = run("systemctl", &["--user", "is-active", "digse"])?;
    if active == "active" {
        let _ = systemctl(&["stop", "digse"]);
    }
    // Disable is best-effort: it errors if the unit never existed.
    let _ = systemctl(&["disable", "digse"]);

    let unit = unit_path()?;
    if unit.exists() {
        std::fs::remove_file(&unit)?;
        println!("startup: removed {}", unit.display());
    } else {
        println!("startup: no unit file present (nothing to remove)");
    }

    systemctl(&["daemon-reload"])?;
    println!("startup: autostart disabled");
    println!("         (linger left enabled; it may serve other user units)");
    Ok(())
}

/// Render the systemd unit. No literal `{`/`}` in a unit file, so no brace
/// doubling for `format!`.
fn unit_contents(exe: &str, host: &str, port: u16) -> String {
    format!(
        "[Unit]
Description=digse metasearch daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
ExecStart={exe} __start_foreground__ --host {host} --port {port}
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
",
        exe = exe,
        host = host,
        port = port,
    )
}

// --- paths ----------------------------------------------------------------

/// `~/.config` for systemd unit placement.
fn config_home() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = home::home_dir()
        .ok_or_else(|| "home directory not found".to_string())?;
    Ok(home.join(".config"))
}

/// `<config_home>/systemd/user` — where user units live.
fn unit_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(config_home()?.join("systemd").join("user"))
}

/// `<unit_dir>/digse.service`.
fn unit_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(unit_dir()?.join("digse.service"))
}

// --- external commands ----------------------------------------------------

/// Run a command, capturing trimmed stdout/stderr. `(success, stdout, stderr)`.
/// `systemctl is-enabled`/`is-active` return non-zero for "disabled"/"inactive"
/// states, so callers must read stdout rather than rely on `success`.
fn run(prog: &str, args: &[&str]) -> Result<(bool, String, String), Box<dyn std::error::Error>> {
    let out = Command::new(prog)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run '{}': {} (is it installed?)", prog, e))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Ok((out.status.success(), stdout, stderr))
}

/// `systemctl --user <args>`, erroring (with stderr) on failure.
fn systemctl(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let mut full = vec!["--user"];
    full.extend_from_slice(args);
    let (ok, _, err) = run("systemctl", &full)?;
    if !ok {
        return Err(format!("systemctl --user {} failed: {}", args.join(" "), err).into());
    }
    Ok(())
}

/// `$USER` (falling back to `$LOGNAME`).
fn current_user() -> Result<String, Box<dyn std::error::Error>> {
    for var in ["USER", "LOGNAME"] {
        if let Ok(u) = env::var(var) {
            if !u.is_empty() {
                return Ok(u);
            }
        }
    }
    Err("could not determine current user name ($USER/$LOGNAME unset)".into())
}

/// `loginctl show-user <user> --property=Linger --value` ("yes"/"no").
fn current_linger() -> Result<String, Box<dyn std::error::Error>> {
    let user = current_user()?;
    let (_, val, _) = run(
        "loginctl",
        &["show-user", &user, "--property=Linger", "--value"],
    )?;
    Ok(val)
}
