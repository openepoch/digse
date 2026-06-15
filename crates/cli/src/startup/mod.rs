//! Boot-time autostart (`digse startup status|add|remove`).
//!
//! The mechanism is platform-specific; this module re-exports a uniform
//! `status` / `add` / `remove` API for whichever platform we were built for:
//!
//! - **Linux** (`linux.rs`): a systemd *user* service + `loginctl enable-linger`,
//!   so the daemon starts at BOOT, before login.
//! - **Windows** (`windows.rs`): a per-user registry **Run** key
//!   (`HKCU\…\Run\digse`) launched via a hidden `.vbs` shim. Needs NO admin.
//!
//! This deliberately stays separate from live-process management
//! (`start`/`stop`/`restart`): `startup` only owns the *boot entry*.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{add, remove, status};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{add, remove, status};
