//! Resolution of the application data directory and its subdirectories.
//!
//! On Windows the Local `AppData` root comes from `SHGetKnownFolderPath` (falling
//! back to the `LOCALAPPDATA` environment variable). On other platforms the app
//! data directory falls back to `~/.win-toolkit`, which is useful when
//! developing the GUI on Linux.

use std::path::PathBuf;

use toolkit_core::APP_SLUG;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn local_app_data() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::known_folders::local_app_data().or_else(|| env_path("LOCALAPPDATA"))
    }
    #[cfg(not(windows))]
    {
        env_path("LOCALAPPDATA")
    }
}

fn home_dir() -> Option<PathBuf> {
    env_path("USERPROFILE").or_else(|| env_path("HOME"))
}

/// The application's data directory: `%LOCALAPPDATA%\win-toolkit` on Windows,
/// `~/.win-toolkit` elsewhere.
#[must_use]
pub fn app_data_dir() -> Option<PathBuf> {
    if let Some(local) = local_app_data() {
        return Some(local.join(APP_SLUG));
    }
    home_dir().map(|home| home.join(format!(".{APP_SLUG}")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdirs_nest_under_the_app_data_dir() {
        if let Some(app_dir) = app_data_dir() {
            assert_eq!(logs_dir(), Some(app_dir.join("logs")));
            assert_eq!(runs_dir(), Some(app_dir.join("runs")));
        }
    }
}
