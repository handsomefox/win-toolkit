//! Portable types and formatting for the read-only system overview. The values
//! are gathered by `toolkit-platform`; this module only defines the shape and
//! the display helpers so they can be unit-tested anywhere.

/// A fixed disk volume and its capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveInfo {
    /// The drive root, e.g. `C:\`.
    pub name: String,
    /// Total capacity in bytes.
    pub total: u64,
    /// Free space in bytes.
    pub free: u64,
}

/// A read-only snapshot of the machine's operating system and hardware.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemInfo {
    /// OS product name, e.g. "Windows 11 Pro".
    pub os_name: String,
    /// OS build string, e.g. "26100.1234".
    pub os_build: String,
    /// The machine (computer) name.
    pub computer_name: String,
    /// System manufacturer and model, e.g. "ASUS ROG STRIX B550-F".
    pub system_model: String,
    /// Human-readable uptime, e.g. "3d 4h 12m".
    pub uptime: String,
    /// CPU model name.
    pub cpu: String,
    /// Number of logical processors.
    pub logical_cpus: usize,
    /// Installed display adapters (GPUs).
    pub gpus: Vec<String>,
    /// Total physical memory in bytes.
    pub total_memory: u64,
    /// Currently available physical memory in bytes.
    pub available_memory: u64,
    /// Fixed drives and their capacities.
    pub drives: Vec<DriveInfo>,
}

impl SystemInfo {
    /// Renders the overview as plain text for copying into a support request.
    #[must_use]
    pub fn to_report(&self) -> String {
        use std::fmt::Write as _;

        let mut report = String::new();
        report.push_str("Windows Toolkit - system report\n\n");
        // Writing to a `String` is infallible, so the results are discarded.
        let _ = writeln!(
            report,
            "OS:        {} (build {})",
            self.os_name, self.os_build
        );
        let _ = writeln!(report, "Machine:   {}", self.computer_name);
        if !self.system_model.is_empty() {
            let _ = writeln!(report, "Model:     {}", self.system_model);
        }
        let _ = writeln!(report, "Uptime:    {}", self.uptime);
        let _ = writeln!(
            report,
            "CPU:       {} ({} logical processors)",
            self.cpu, self.logical_cpus
        );
        for gpu in &self.gpus {
            let _ = writeln!(report, "GPU:       {gpu}");
        }
        let _ = writeln!(
            report,
            "Memory:    {} free of {}",
            format_bytes(self.available_memory),
            format_bytes(self.total_memory)
        );
        report.push_str("Drives:\n");
        for drive in &self.drives {
            let _ = writeln!(
                report,
                "  {} {} free of {}",
                drive.name,
                format_bytes(drive.free),
                format_bytes(drive.total)
            );
        }
        report
    }
}

/// Formats a byte count using binary units (KiB, MiB, GiB, TiB).
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    #[expect(
        clippy::cast_precision_loss,
        reason = "display-only formatting of a byte count"
    )]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Formats a duration in seconds as `Nd Nh Nm`.
#[must_use]
pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16.0 GiB");
    }

    #[test]
    fn uptime_drops_leading_zero_units() {
        assert_eq!(format_uptime(90), "1m");
        assert_eq!(format_uptime(3 * 3600 + 5 * 60), "3h 5m");
        assert_eq!(format_uptime(2 * 86_400 + 3600), "2d 1h 0m");
    }

    #[test]
    fn report_lists_drives() {
        let info = SystemInfo {
            os_name: "Windows 11 Pro".to_owned(),
            os_build: "26100.1".to_owned(),
            computer_name: "PC".to_owned(),
            system_model: "ACME Box".to_owned(),
            uptime: "1h 0m".to_owned(),
            cpu: "Test CPU".to_owned(),
            logical_cpus: 8,
            gpus: vec!["Test GPU".to_owned()],
            total_memory: 16 * 1024 * 1024 * 1024,
            available_memory: 8 * 1024 * 1024 * 1024,
            drives: vec![DriveInfo {
                name: r"C:\".to_owned(),
                total: 1024,
                free: 512,
            }],
        };
        let report = info.to_report();
        assert!(report.contains("Windows 11 Pro (build 26100.1)"));
        assert!(report.contains("GPU:       Test GPU"));
        assert!(report.contains(r"C:\ 512 B free of 1.0 KiB"));
    }
}
