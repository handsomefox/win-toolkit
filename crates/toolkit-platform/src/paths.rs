//! Resolution of the application data directory and its subdirectories.
//!
//! The app's own folder comes from `app_folders`: `%LOCALAPPDATA%\win-toolkit`
//! on Windows, and the XDG data folder elsewhere, which is useful when
//! developing the GUI on Linux.

use std::path::PathBuf;

use toolkit_core::APP_SLUG;

/// The Windows directory, from `SystemRoot`, or `C:\Windows` when that is not
/// set.
#[must_use]
pub fn system_root() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned())
}

/// The application's data directory: `%LOCALAPPDATA%\win-toolkit` on Windows,
/// `$XDG_DATA_HOME/win-toolkit` or `~/.local/share/win-toolkit` elsewhere.
#[must_use]
pub fn app_data_dir() -> Option<PathBuf> {
    app_folders::app_data_dir(APP_SLUG)
}

/// Directory the diagnostics log is written to.
#[must_use]
pub fn logs_dir() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("logs"))
}

/// Directory the captured output of elevated child processes is written to.
///
/// This lives under the app-data directory rather than the system temp dir so a
/// high-integrity elevated child can write it while the medium-integrity GUI
/// reads it back.
#[must_use]
pub fn runs_dir() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("runs"))
}

/// Directory generated Windows Sandbox (`.wsb`) configurations are written to.
#[must_use]
pub fn sandbox_dir() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("sandbox"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdirs_nest_under_the_app_data_dir() {
        if let Some(app_dir) = app_data_dir() {
            assert!(app_dir.ends_with(APP_SLUG));
            assert_eq!(logs_dir(), Some(app_dir.join("logs")));
            assert_eq!(runs_dir(), Some(app_dir.join("runs")));
        }
    }
}
