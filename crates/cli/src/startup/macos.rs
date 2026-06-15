//! macOS autostart (launchd) — not yet implemented.
//!
//! All three commands are harmless: they report that launchd-based autostart is
//! not supported in this version and do nothing destructive. The daemon itself
//! runs fine on macOS via `digse start`; only the *boot* entry is unimplemented.

pub fn status() -> Result<(), Box<dyn std::error::Error>> {
    println!("startup: autostart on macOS via launchd is not yet supported");
    Ok(())
}

pub fn add() -> Result<(), Box<dyn std::error::Error>> {
    println!("startup: autostart on macOS via launchd is not yet supported");
    println!("         Start the daemon manually with: digse start");
    Ok(())
}

pub fn remove() -> Result<(), Box<dyn std::error::Error>> {
    println!("startup: autostart on macOS via launchd is not yet supported (nothing to remove)");
    Ok(())
}
