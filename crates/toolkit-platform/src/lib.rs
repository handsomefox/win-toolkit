//! Windows platform integration for win-toolkit.
//!
//! This crate is the only place allowed to talk to Windows APIs. On other
//! platforms the Windows-specific pieces compile to stubs (or fall back to
//! environment variables) so the workspace builds and tests run on Linux.

pub mod exec;
pub mod paths;
pub mod startup;
pub mod sysinfo;

pub use exec::{CaptureOutput, ElevatedChild, ExecError, launch, run_capture, run_elevated};
pub use paths::{
    app_data_dir, logs_dir, runs_dir, sandbox_dir, system_directory, system_root,
    windows_powershell,
};
pub use startup::{list_startup, set_startup_enabled};
pub use sysinfo::system_info;
