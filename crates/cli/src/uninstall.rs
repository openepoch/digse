//! Uninstall digse completely (binary, config, autostart).

use std::path::PathBuf;

/// Files/directories that will be removed.
pub struct RemovalPlan {
    pub binary: PathBuf,
    pub state_dir: PathBuf,
    pub autostart_files: Vec<PathBuf>,
}

/// Generate the removal plan by detecting all installed files.
pub fn plan_removal() -> Result<RemovalPlan, Box<dyn std::error::Error>> {
    let binary = std::env::current_exe()?;
    let state_dir = crate::pidfile::state_dir()?;

    let mut autostart_files = Vec::new();

    // Linux systemd unit
    #[cfg(target_os = "linux")]
    {
        if let Some(config_dir) = dirs::config_dir() {
            let unit = config_dir.join("systemd/user/digse.service");
            if unit.exists() {
                autostart_files.push(unit);
            }
        }
    }

    // Windows VBS shim is handled by startup::remove()
    // We just track the VBS shim for display
    #[cfg(windows)]
    {
        let shim = state_dir.join("digse-autostart.vbs");
        if shim.exists() {
            autostart_files.push(shim);
        }
    }

    Ok(RemovalPlan {
        binary,
        state_dir,
        autostart_files,
    })
}

/// Display what will be removed and ask for confirmation.
pub fn confirm(plan: &RemovalPlan) -> Result<bool, Box<dyn std::error::Error>> {
    println!("The following files will be removed:");
    println!();
    println!("  Binary: {}", plan.binary.display());
    println!("  State directory: {}", plan.state_dir.display());

    // List contents of state directory
    if let Ok(entries) = std::fs::read_dir(&plan.state_dir) {
        for entry in entries.flatten() {
            println!("    {}", entry.file_name().to_string_lossy());
        }
    }

    for file in &plan.autostart_files {
        println!("  Autostart: {}", file.display());
    }

    println!();
    print!("Remove digse completely? [y/N]: ");
    use std::io::Write;
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().to_ascii_lowercase();

    Ok(input == "y" || input == "yes")
}

/// Stop daemon, remove autostart, delete state files.
pub fn remove_daemon_and_state() -> Result<(), Box<dyn std::error::Error>> {
    // Stop the daemon if running
    let _ = crate::daemon::stop_server(); // Ignore if not running

    // Remove autostart configuration
    let _ = crate::startup::remove(); // Ignore if not configured

    // Delete state directory
    let state_dir = crate::pidfile::state_dir()?;
    if state_dir.exists() {
        std::fs::remove_dir_all(&state_dir)?;
        println!("Removed state directory: {}", state_dir.display());
    }

    // Remove systemd unit on Linux
    #[cfg(target_os = "linux")]
    {
        if let Some(config_dir) = dirs::config_dir() {
            let unit = config_dir.join("systemd/user/digse.service");

            // Stop and disable the service
            let _ = std::process::Command::new("systemctl")
                .args(&["--user", "stop", "digse"])
                .output();
            let _ = std::process::Command::new("systemctl")
                .args(&["--user", "disable", "digse"])
                .output();

            if unit.exists() {
                std::fs::remove_file(&unit)?;
                println!("Removed systemd unit: {}", unit.display());
            }

            // Reload systemd
            let _ = std::process::Command::new("systemctl")
                .args(&["--user", "daemon-reload"])
                .output();
        }
    }

    Ok(())
}

/// Delete the binary itself using self_replace.
pub fn remove_binary() -> Result<(), Box<dyn std::error::Error>> {
    let bin_path = std::env::current_exe()?;

    #[cfg(windows)]
    {
        // On Windows, self_replace can do in-place replacement
        self_replace::self_delete(&bin_path)?;
        println!("Removed binary: {}", bin_path.display());
    }

    #[cfg(unix)]
    {
        // On Unix, we need to spawn a shell script to delete after exit
        let script = format!(
            "#!/bin/sh\nsleep 0.5\nrm - '{}'\n",
            bin_path.display()
        );

        let temp_script = std::env::temp_dir()
            .join(format!("digse-uninstall-{}.sh", std::process::id()));

        std::fs::write(&temp_script, script)?;

        // Make executable
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_script)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_script, perms)?;

        // Spawn detached script to delete after we exit
        let _ = std::process::Command::new(&temp_script)
            .spawn();

        println!("Binary will be removed shortly: {}", bin_path.display());
    }

    Ok(())
}

/// Main uninstall entry point.
pub fn run_uninstall() -> Result<(), Box<dyn std::error::Error>> {
    let plan = plan_removal()?;

    if !confirm(&plan)? {
        println!("Uninstall cancelled.");
        return Ok(());
    }

    println!();

    // Stop daemon and remove state
    remove_daemon_and_state()?;

    // Remove binary (last - this will exit)
    remove_binary()?;

    println!();
    println!("digse has been uninstalled.");

    #[cfg(unix)]
    {
        println!("The binary will be removed momentarily.");
        println!("You can close this terminal.");
    }

    Ok(())
}
