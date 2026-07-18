//! `SHGetKnownFolderPath` wrapper for the user's Local `AppData` root. Preferred
//! over raw environment variables per Windows conventions; callers fall back to
//! env vars when the lookup fails.

use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath};

pub(crate) fn local_app_data() -> Option<PathBuf> {
    known_folder(&FOLDERID_LocalAppData)
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    // SAFETY: the GUID is a valid known-folder identifier and the API owns the
    // returned allocation until it is released below.
    let value = unsafe { SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG::default(), None) }.ok()?;
    // SAFETY: a successful `SHGetKnownFolderPath` returns a valid NUL-terminated
    // UTF-16 string that remains allocated until the `CoTaskMemFree` below.
    let path = unsafe { value.to_string() }.ok().map(PathBuf::from);
    // SAFETY: `value` is the COM allocation returned above, is no longer read,
    // and is released exactly once.
    unsafe { CoTaskMemFree(Some(value.as_ptr().cast())) };
    path
}
