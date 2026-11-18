//! Reads and toggles startup entries: the `Run` registry keys and the Startup
//! folders, for the current user and for all users.
//!
//! Each entry's enabled state lives under `...\Explorer\StartupApproved`, in
//! the subkey for its source, under the same root key as the entry (see
//! [`toolkit_core::StartupSource`]). Current-user entries can be toggled
//! without elevation. Machine-wide entries are read-only here, because
//! changing them needs Administrator.

use toolkit_core::StartupEntry;

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

/// Enables or disables a current-user startup entry, in the `StartupApproved`
/// subkey for its source.
///
/// # Errors
///
/// Returns an error message if the entry is machine-wide (needs elevation), on
/// non-Windows platforms, or if the registry write fails.
pub fn set_startup_enabled(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
    if !entry.scope.can_toggle() {
        return Err("machine-wide startup entries require Administrator".to_owned());
    }
    #[cfg(windows)]
    {
        imp::set_enabled(entry, enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("startup management is only available on Windows".to_owned())
    }
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use toolkit_core::{
        StartupEntry, StartupScope, StartupSource, approved_value, approved_value_enabled,
        is_startup_folder_entry,
    };
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        FOLDERID_CommonStartup, FOLDERID_Startup, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };
    use windows::core::GUID;
    use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type};

    const RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// The 32-bit view of the machine-wide `Run` key, read by full path from
    /// this 64-bit process.
    const RUN32_PATH: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";

    pub(super) fn list() -> Vec<StartupEntry> {
        let mut entries = read_run(StartupScope::CurrentUser, StartupSource::Run, RUN_PATH);
        entries.extend(read_run(
            StartupScope::LocalMachine,
            StartupSource::Run,
            RUN_PATH,
        ));
        entries.extend(read_run(
            StartupScope::LocalMachine,
            StartupSource::Run32,
            RUN32_PATH,
        ));
        entries.extend(read_folder(StartupScope::CurrentUser, &FOLDERID_Startup));
        entries.extend(read_folder(
            StartupScope::LocalMachine,
            &FOLDERID_CommonStartup,
        ));
        entries
    }

    fn root(scope: StartupScope) -> &'static Key {
        match scope {
            StartupScope::CurrentUser => CURRENT_USER,
            StartupScope::LocalMachine => LOCAL_MACHINE,
        }
    }

    fn read_run(scope: StartupScope, source: StartupSource, path: &str) -> Vec<StartupEntry> {
        let Ok(run) = root(scope).open(path) else {
            return Vec::new();
        };
        let Ok(values) = run.values() else {
            return Vec::new();
        };
        let approved = root(scope).open(source.approved_path()).ok();
        values
            .map(|(name, value)| StartupEntry {
                command: String::try_from(value).unwrap_or_default(),
                enabled: is_enabled(approved.as_ref(), &name),
                name,
                scope,
                source,
            })
            .collect()
    }

    fn read_folder(scope: StartupScope, folder: &GUID) -> Vec<StartupEntry> {
        let Some(dir) = known_folder(folder) else {
            return Vec::new();
        };
        let Ok(files) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let source = StartupSource::StartupFolder;
        let approved = root(scope).open(source.approved_path()).ok();
        files
            .flatten()
            .filter(|file| file.file_type().is_ok_and(|kind| !kind.is_dir()))
            .filter_map(|file| {
                let name = file.file_name().into_string().ok()?;
                is_startup_folder_entry(&name).then(|| StartupEntry {
                    command: file.path().display().to_string(),
                    enabled: is_enabled(approved.as_ref(), &name),
                    name,
                    scope,
                    source,
                })
            })
            .collect()
    }

    fn is_enabled(approved: Option<&Key>, name: &str) -> bool {
        let value = approved.and_then(|key| key.get_value(name).ok());
        approved_value_enabled(value.as_deref())
    }

    /// The path of a known folder, or `None` when Windows cannot give one.
    fn known_folder(id: &GUID) -> Option<PathBuf> {
        // SAFETY: `id` points at a known-folder GUID for the whole call, and no
        // token is passed, so the folder is the current user's.
        let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
        // SAFETY: on success `raw` is a NUL-terminated UTF-16 string that
        // stays valid until `CoTaskMemFree` below.
        let path = unsafe { raw.to_string() }.ok();
        // SAFETY: `raw` was allocated by `SHGetKnownFolderPath`, which hands
        // ownership to the caller, and it is freed once here.
        unsafe { CoTaskMemFree(Some(raw.as_ptr().cast_const().cast())) };
        path.map(PathBuf::from).filter(|path| path.is_absolute())
    }

    pub(super) fn set_enabled(entry: &StartupEntry, enabled: bool) -> Result<(), String> {
        let approved = root(entry.scope)
            .create(entry.source.approved_path())
            .map_err(|err| err.message())?;
        approved
            .set_bytes(
                &entry.name,
                Type::Bytes,
                &approved_value(enabled, SystemTime::now()),
            )
            .map_err(|err| err.message())
    }
}

#[cfg(test)]
mod tests {
    use toolkit_core::{StartupScope, StartupSource};

    use super::*;

    #[test]
    fn machine_wide_entries_are_refused_without_a_registry_write() {
        for source in [
            StartupSource::Run,
            StartupSource::Run32,
            StartupSource::StartupFolder,
        ] {
            let entry = StartupEntry {
                name: "Example".to_owned(),
                command: "example.exe".to_owned(),
                scope: StartupScope::LocalMachine,
                source,
                enabled: true,
            };
            assert_eq!(
                set_startup_enabled(&entry, false),
                Err("machine-wide startup entries require Administrator".to_owned())
            );
        }
    }
}
