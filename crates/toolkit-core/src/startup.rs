//! Portable types and parsing for the Startup manager. Reading and writing the
//! registry and the Startup folders lives in `toolkit-platform`.
//!
//! Windows starts programs at sign-in from the `Run` registry keys and from the
//! Startup folders. Whether each entry is enabled is stored apart from the
//! entry, under `...\Explorer\StartupApproved`, in the subkey
//! [`StartupSource::approved_key`] names. That is where Task Manager's
//! **Startup apps** page keeps it.

use std::time::{Duration, SystemTime};

/// Where a startup entry is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupScope {
    /// `HKEY_CURRENT_USER` and this user's Startup folder. Togglable without
    /// elevation.
    CurrentUser,
    /// `HKEY_LOCAL_MACHINE` and the all-users Startup folder. Changing these
    /// requires Administrator.
    LocalMachine,
}

impl StartupScope {
    /// A short human label for the scope.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            StartupScope::CurrentUser => "Current user",
            StartupScope::LocalMachine => "All users",
        }
    }

    /// Whether this app can toggle entries in the scope without elevation
    /// (current-user entries only).
    #[must_use]
    pub fn can_toggle(self) -> bool {
        self == StartupScope::CurrentUser
    }
}

/// Which list Windows reads a startup entry from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupSource {
    /// A value under `Software\Microsoft\Windows\CurrentVersion\Run`.
    Run,
    /// A value under `Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run`,
    /// where 32-bit installers register machine-wide entries.
    Run32,
    /// A file in a Startup folder.
    StartupFolder,
}

impl StartupSource {
    /// A short human label for the source.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            StartupSource::Run => "Registry",
            StartupSource::Run32 => "Registry (32-bit)",
            StartupSource::StartupFolder => "Startup folder",
        }
    }

    /// The subkey of [`STARTUP_APPROVED`] that records whether entries from
    /// this source are enabled.
    #[must_use]
    pub fn approved_key(self) -> &'static str {
        match self {
            StartupSource::Run => "Run",
            StartupSource::Run32 => "Run32",
            StartupSource::StartupFolder => "StartupFolder",
        }
    }

    /// The full path of [`Self::approved_key`], under the entry's own root
    /// key: `HKEY_CURRENT_USER` or `HKEY_LOCAL_MACHINE`.
    #[must_use]
    pub fn approved_path(self) -> String {
        format!(r"{STARTUP_APPROVED}\{}", self.approved_key())
    }
}

/// The key, under `HKEY_CURRENT_USER` or `HKEY_LOCAL_MACHINE`, whose subkeys
/// record which startup entries are enabled.
pub const STARTUP_APPROVED: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// A program registered to run at sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupEntry {
    /// The registry value name, or the file name in a Startup folder. The
    /// `StartupApproved` value for the entry has the same name.
    pub name: String,
    /// The command line the entry runs, or the path of the Startup folder file.
    pub command: String,
    /// Whose sign-in starts the entry.
    pub scope: StartupScope,
    /// Which list the entry is in.
    pub source: StartupSource,
    /// Whether the entry is currently enabled.
    pub enabled: bool,
}

/// Whether a `StartupApproved` value marks its entry enabled. An entry with no
/// value, or an empty one, is enabled. Otherwise an even first byte means
/// enabled and an odd one disabled.
#[must_use]
pub fn approved_value_enabled(value: Option<&[u8]>) -> bool {
    value
        .and_then(<[u8]>::first)
        .is_none_or(|byte| byte % 2 == 0)
}

/// The 12-byte `StartupApproved` value that marks an entry enabled or disabled.
///
/// The first byte is 2 for enabled and 3 for disabled. A disabled value also
/// carries the time it was disabled, as a little-endian `FILETIME` in bytes 4
/// to 11. An enabled value has zeros there.
#[must_use]
pub fn approved_value(enabled: bool, now: SystemTime) -> [u8; 12] {
    let mut value = [0_u8; 12];
    if enabled {
        value[0] = 0x02;
    } else {
        value[0] = 0x03;
        value[4..].copy_from_slice(&filetime(now).to_le_bytes());
    }
    value
}

/// `time` as a Windows `FILETIME`: 100-nanosecond intervals since 1601-01-01
/// UTC. A time before 1970 counts as 1970.
fn filetime(time: SystemTime) -> u64 {
    /// 100-nanosecond intervals from 1601-01-01 to 1970-01-01.
    const UNIX_EPOCH_AS_FILETIME: u64 = 116_444_736_000_000_000;
    let since_unix = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let intervals = u64::try_from(since_unix.as_nanos() / 100).unwrap_or(u64::MAX);
    UNIX_EPOCH_AS_FILETIME.saturating_add(intervals)
}

/// Whether a file in a Startup folder is a startup entry. Every file is, except
/// `desktop.ini`, which holds the folder's own display settings.
#[must_use]
pub fn is_startup_folder_entry(file_name: &str) -> bool {
    !file_name.is_empty() && !file_name.eq_ignore_ascii_case("desktop.ini")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_source_has_its_own_approved_key() {
        assert_eq!(
            StartupSource::Run.approved_path(),
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
        );
        assert_eq!(StartupSource::Run32.approved_key(), "Run32");
        assert_eq!(StartupSource::StartupFolder.approved_key(), "StartupFolder");
    }

    #[test]
    fn missing_or_even_values_are_enabled_and_odd_ones_disabled() {
        assert!(approved_value_enabled(None));
        assert!(approved_value_enabled(Some(&[])));
        assert!(approved_value_enabled(Some(&[0x02, 0, 0, 0])));
        assert!(approved_value_enabled(Some(&[0x06])));
        assert!(!approved_value_enabled(Some(&[0x03, 0, 0, 0])));
        assert!(!approved_value_enabled(Some(&[0x07])));
    }

    #[test]
    fn written_values_read_back_as_written() {
        let now = SystemTime::now();
        assert!(approved_value_enabled(Some(&approved_value(true, now))));
        assert!(!approved_value_enabled(Some(&approved_value(false, now))));
    }

    #[test]
    fn a_disabled_value_records_when_it_was_disabled() {
        // 2024-01-01T00:00:00Z.
        let time = SystemTime::UNIX_EPOCH + Duration::from_hours(473_352);
        let expected: u64 = 133_485_408_000_000_000;
        let disabled = approved_value(false, time);
        assert_eq!(disabled[..4], [0x03, 0, 0, 0]);
        assert_eq!(disabled[4..], expected.to_le_bytes());
        assert_eq!(approved_value(true, time), {
            let mut enabled = [0; 12];
            enabled[0] = 0x02;
            enabled
        });
    }

    #[test]
    fn a_time_before_1970_does_not_wrap() {
        let early = SystemTime::UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(filetime(early), 116_444_736_000_000_000);
    }

    #[test]
    fn desktop_ini_is_not_a_startup_entry() {
        assert!(is_startup_folder_entry("Example.lnk"));
        assert!(is_startup_folder_entry("script.bat"));
        assert!(!is_startup_folder_entry("desktop.ini"));
        assert!(!is_startup_folder_entry("Desktop.INI"));
        assert!(!is_startup_folder_entry(""));
    }
}
