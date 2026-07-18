//! Gathers the read-only system overview from Windows APIs and the registry. On
//! non-Windows platforms it returns a minimal placeholder so the GUI still runs
//! for development.

use toolkit_core::SystemInfo;

/// Collects a snapshot of the operating system and hardware.
#[must_use]
pub fn system_info() -> SystemInfo {
    let logical_cpus = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    #[cfg(windows)]
    {
        imp::collect(logical_cpus)
    }
    #[cfg(not(windows))]
    {
        SystemInfo {
            os_name: "Not Windows (development build)".to_owned(),
            os_build: String::new(),
            computer_name: std::env::var("HOSTNAME").unwrap_or_default(),
            uptime: toolkit_core::format_uptime(0),
            cpu: "Unavailable off Windows".to_owned(),
            logical_cpus,
            total_memory: 0,
            available_memory: 0,
            drives: Vec::new(),
        }
    }
}

#[cfg(windows)]
mod imp {
    use toolkit_core::{DriveInfo, SystemInfo, format_uptime};
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW};
    use windows::Win32::System::SystemInformation::{
        GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };
    use windows::core::{HSTRING, PCWSTR};
    use windows_registry::LOCAL_MACHINE;

    /// `GetDriveTypeW` return value for a fixed (non-removable) drive.
    const DRIVE_FIXED: u32 = 3;

    pub(super) fn collect(logical_cpus: usize) -> SystemInfo {
        let (total_memory, available_memory) = memory();
        SystemInfo {
            os_name: os_name(),
            os_build: os_build(),
            computer_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
            uptime: uptime(),
            cpu: cpu_name(),
            logical_cpus,
            total_memory,
            available_memory,
            drives: fixed_drives(),
        }
    }

    fn os_name() -> String {
        let Ok(key) = LOCAL_MACHINE.open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") else {
            return "Windows".to_owned();
        };
        let name = key
            .get_string("ProductName")
            .unwrap_or_else(|_| "Windows".to_owned());
        // Microsoft never updated `ProductName` for Windows 11 — it still reads
        // "Windows 10 ...". Correct it using the build number (11 starts at
        // build 22000) so the overview does not misreport the OS.
        let build: u32 = key
            .get_string("CurrentBuild")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        if build >= 22000 && name.contains("Windows 10") {
            name.replace("Windows 10", "Windows 11")
        } else {
            name
        }
    }

    fn os_build() -> String {
        let Ok(key) = LOCAL_MACHINE.open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") else {
            return String::new();
        };
        let build = key.get_string("CurrentBuild").unwrap_or_default();
        match key.get_u32("UBR") {
            Ok(ubr) => format!("{build}.{ubr}"),
            Err(_) => build,
        }
    }

    fn cpu_name() -> String {
        LOCAL_MACHINE
            .open(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0")
            .and_then(|key| key.get_string("ProcessorNameString"))
            .map_or_else(|_| "Unknown CPU".to_owned(), |name| name.trim().to_owned())
    }

    fn uptime() -> String {
        // SAFETY: `GetTickCount64` takes no arguments and cannot fail.
        let milliseconds = unsafe { GetTickCount64() };
        format_uptime(milliseconds / 1000)
    }

    fn memory() -> (u64, u64) {
        let mut status = MEMORYSTATUSEX {
            dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or_default(),
            ..Default::default()
        };
        // SAFETY: `status.dwLength` is set to the struct size as required, and
        // `status` is a valid out-pointer.
        if unsafe { GlobalMemoryStatusEx(&raw mut status) }.is_ok() {
            (status.ullTotalPhys, status.ullAvailPhys)
        } else {
            (0, 0)
        }
    }

    fn fixed_drives() -> Vec<DriveInfo> {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let root = format!(r"{}:\", letter as char);
            let wide = HSTRING::from(root.as_str());
            // SAFETY: `wide` is a valid NUL-terminated string that outlives the
            // call.
            let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
            if drive_type != DRIVE_FIXED {
                continue;
            }
            let mut total: u64 = 0;
            let mut free: u64 = 0;
            // SAFETY: `wide` is valid; `total`/`free` are valid out-pointers and
            // the unused available-to-caller argument is null.
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    PCWSTR(wide.as_ptr()),
                    None,
                    Some(&raw mut total),
                    Some(&raw mut free),
                )
            };
            if ok.is_ok() {
                drives.push(DriveInfo {
                    name: root,
                    total,
                    free,
                });
            }
        }
        drives
    }
}
