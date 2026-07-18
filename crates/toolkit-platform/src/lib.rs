//! Windows platform integration for win-toolkit.
//!
//! This crate is the only place allowed to talk to Windows APIs. On other
//! platforms the Windows-specific pieces compile to stubs (or fall back to
//! environment variables) so the workspace builds and tests run on Linux.

pub mod exec;
#[cfg(windows)]
mod known_folders;
pub mod paths;

pub use exec::{CaptureOutput, ElevatedChild, ExecError, launch, run_capture, run_elevated};
pub use paths::{app_data_dir, logs_dir, runs_dir};
