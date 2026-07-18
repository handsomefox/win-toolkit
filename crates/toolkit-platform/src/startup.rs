//! Reads and toggles per-user and machine-wide startup entries via the registry.
//!
//! Entries come from the `...\CurrentVersion\Run` keys; their enabled state is
//! stored separately under `...\Explorer\StartupApproved\Run` as a 12-byte blob
//! whose first byte is even when enabled and odd when disabled. Current-user
//! entries can be toggled without elevation; machine-wide entries are read-only
//! here (changing them needs Administrator).

use toolkit_core::{StartupEntry, StartupScope};

/// Lists startup entries for the current user and all users.
#[must_use]
pub fn list_startup() -> Vec<StartupEntry> {
    #[cfg(windows)]
    {
        imp::list()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Enables or disables a current-user startup entry.
///
/// # Errors
///
/// Returns an error message if the entry is machine-wide (needs elevation), on
/// non-Windows platforms, or if the registry write fails.
pub fn set_startup_enabled(scope: StartupScope, name: &str, enabled: bool) -> Result<(), String> {
    if scope != StartupScope::CurrentUser {
        return Err("machine-wide startup entries require Administrator".to_owned());
    }
    #[cfg(windows)]
    {
        imp::set_enabled(name, enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = (name, enabled);
        Err("startup management is only available on Windows".to_owned())
    }
}

#[cfg(windows)]
mod imp {
    use toolkit_core::{StartupEntry, StartupScope};
    use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type};

    const RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED_PATH: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    /// 12-byte `StartupApproved` blob marking an entry enabled.
    const ENABLED_BLOB: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// 12-byte `StartupApproved` blob marking an entry disabled.
    const DISABLED_BLOB: [u8; 12] = [0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    pub(super) fn list() -> Vec<StartupEntry> {
        let mut entries = read(CURRENT_USER, StartupScope::CurrentUser);
        entries.extend(read(LOCAL_MACHINE, StartupScope::LocalMachine));
        entries
    }

    fn read(root: &Key, scope: StartupScope) -> Vec<StartupEntry> {
        let Ok(run) = root.open(RUN_PATH) else {
            return Vec::new();
        };
        let approved = root.open(APPROVED_PATH).ok();
        let Ok(values) = run.values() else {
            return Vec::new();
        };
        values
            .map(|(name, value)| {
                let command = String::try_from(value).unwrap_or_default();
                let enabled = is_enabled(approved.as_ref(), &name);
                StartupEntry {
                    name,
                    command,
                    scope,
                    enabled,
                    can_toggle: scope == StartupScope::CurrentUser,
                }
            })
            .collect()
    }

    /// An entry is enabled unless `StartupApproved` records it with an odd
    /// leading byte.
    fn is_enabled(approved: Option<&Key>, name: &str) -> bool {
        let Some(approved) = approved else {
            return true;
        };
        match approved.get_value(name) {
            Ok(value) => value.as_ref().first().is_none_or(|byte| byte % 2 == 0),
            Err(_) => true,
        }
    }

    pub(super) fn set_enabled(name: &str, enabled: bool) -> Result<(), String> {
        let approved = CURRENT_USER
            .create(APPROVED_PATH)
            .map_err(|err| err.message())?;
        let blob = if enabled { ENABLED_BLOB } else { DISABLED_BLOB };
        approved
            .set_bytes(name, Type::Bytes, &blob)
            .map_err(|err| err.message())
    }
}
