//! Portable domain types for win-toolkit.
//!
//! This crate holds the platform-neutral vocabulary shared by the UI
//! (`toolkit-app`) and the Windows integration layer (`toolkit-platform`): the
//! application's identity and the classification of the operations the toolkit
//! can run. It builds and tests on any platform so the bulk of the logic can be
//! developed and checked on Linux.

pub mod command;
pub mod operation;
pub mod sysinfo;

pub use command::{
    CommandLine, CommandSpec, compact_lines, console_lines, decode_console_output,
    elevated_cmd_parameters,
};
pub use operation::{
    Execution, Operation, health_operations, launcher, network_operations, performance_operations,
};
pub use sysinfo::{DriveInfo, SystemInfo, format_bytes, format_uptime};

/// The short, filesystem-safe application slug. Used for the app-data directory
/// (`%LOCALAPPDATA%\win-toolkit`) and log file names.
pub const APP_SLUG: &str = "win-toolkit";

/// The human-facing product name, shown in the window title and About screen.
pub const APP_TITLE: &str = "Windows Toolkit";

/// Whether an operation needs Administrator rights to run.
///
/// The application itself runs unelevated; operations that require elevation are
/// launched as an elevated child process (triggering a UAC prompt) only when the
/// user chooses to run them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    /// Runs in the current (unelevated) process.
    None,
    /// Must be launched with Administrator rights.
    Administrator,
}

/// How consequential an operation is, used to choose how prominently the UI
/// warns the user before running it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// Read-only inspection with no system changes.
    ReadOnly,
    /// Changes state but is trivially and fully reversible.
    Low,
    /// Changes state in a way that is reversible but disruptive (e.g. requires a
    /// reboot, or resets configuration that must be re-applied).
    Medium,
    /// Long-running or hard-to-interrupt system servicing operations.
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_stable() {
        assert_eq!(APP_SLUG, "win-toolkit");
        assert_eq!(APP_TITLE, "Windows Toolkit");
    }

    #[test]
    fn classification_enums_compare_by_value() {
        assert_eq!(Elevation::Administrator, Elevation::Administrator);
        assert_ne!(Risk::ReadOnly, Risk::High);
    }
}
