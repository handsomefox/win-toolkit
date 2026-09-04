//! The operation model: a single, described maintenance or diagnostic action
//! the toolkit can run, plus the built-in catalogs for the command-driven
//! sections.
//!
//! Operations are plain data. The UI renders them (label, description,
//! consequences) and the platform layer executes them. Adding a tool is a matter
//! of adding operations to a catalog — there is no plugin system or dynamic
//! registry.

use crate::command::CommandLine;
use crate::{Elevation, Risk};

/// How an operation is carried out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Execution {
    /// Run a command line and capture its output for display.
    Capture(CommandLine),
    /// Launch a program, file, or folder without capturing output (e.g. opening
    /// a built-in Windows tool or a report). Always runs unelevated.
    Launch {
        /// The program, file, or folder to open.
        target: String,
        /// Optional arguments passed to the target.
        args: Vec<String>,
    },
}

/// A single maintenance or diagnostic action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// Stable machine identifier, used for logging and UI state.
    pub id: &'static str,
    /// Short human-facing name.
    pub label: &'static str,
    /// What the operation does and its consequences, shown before it runs.
    pub description: &'static str,
    /// A rough duration expectation (e.g. "Usually 5-15 minutes"), or empty.
    pub duration_hint: &'static str,
    /// How consequential the operation is.
    pub risk: Risk,
    /// Whether the operation must run elevated.
    pub elevation: Elevation,
    /// Whether the operation can be safely cancelled once started. Elevated
    /// children cannot be signalled from the unelevated GUI and long servicing
    /// operations are unsafe to interrupt, so those are not cancelable.
    pub cancelable: bool,
    /// How the operation is executed.
    pub execution: Execution,
}

impl Operation {
    /// Whether this operation captures and displays output (as opposed to a
    /// fire-and-forget launch).
    #[must_use]
    pub fn is_capture(&self) -> bool {
        matches!(self.execution, Execution::Capture(_))
    }
}

/// Builds a launcher operation (opens a program/file/folder, no output).
#[must_use]
pub fn launcher(
    id: &'static str,
    label: &'static str,
    description: &'static str,
    target: String,
    args: Vec<String>,
) -> Operation {
    Operation {
        id,
        label,
        description,
        duration_hint: "",
        risk: Risk::ReadOnly,
        elevation: Elevation::None,
        cancelable: false,
        execution: Execution::Launch { target, args },
    }
}

/// The Health &amp; Repair catalog, grouped for the UI: repair, component-store
/// cleanup, and recovery.
#[must_use]
pub fn health_groups() -> Vec<(&'static str, Vec<Operation>)> {
    vec![
        (
            "Repair",
            vec![sfc_scannow(), dism_restore_health(), chkdsk_scan()],
        ),
        (
            "Component store (WinSxS)",
            vec![dism_analyze_store(), dism_cleanup_resetbase()],
        ),
        (
            "Recovery",
            vec![reset_windows_update(), create_restore_point()],
        ),
    ]
}

/// The Health &amp; Repair operations as a flat list.
#[must_use]
pub fn health_operations() -> Vec<Operation> {
    health_groups()
        .into_iter()
        .flat_map(|(_, ops)| ops)
        .collect()
}

fn sfc_scannow() -> Operation {
    Operation {
        id: "sfc-scannow",
        label: "System File Checker (sfc /scannow)",
        description: "Scans all protected Windows system files and repairs corrupted ones from the \
                      local component store. Makes no other changes.",
        duration_hint: "Usually 5-15 minutes",
        risk: Risk::High,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program("sfc", &["/scannow"])),
    }
}

fn dism_restore_health() -> Operation {
    Operation {
        id: "dism-restorehealth",
        label: "DISM Restore Health",
        description: "Repairs the Windows component store using Windows Update as the source of \
                      known-good files. Run this if System File Checker cannot repair some files.",
        duration_hint: "Usually 5-20 minutes; needs internet",
        risk: Risk::High,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program(
            "dism",
            &["/Online", "/Cleanup-Image", "/RestoreHealth"],
        )),
    }
}

fn chkdsk_scan() -> Operation {
    Operation {
        id: "chkdsk-scan",
        label: "Check system drive (chkdsk C: /scan)",
        description: "Scans the system drive for file-system errors online, without locking it or \
                      requiring a reboot. Reports problems it finds.",
        duration_hint: "Usually a few minutes",
        risk: Risk::Medium,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program("chkdsk", &["C:", "/scan"])),
    }
}

fn dism_analyze_store() -> Operation {
    Operation {
        id: "dism-analyze-store",
        label: "Analyze component store (WinSxS)",
        description: "Reports the size of the WinSxS component store and whether a cleanup is \
                      recommended. Makes no changes.",
        duration_hint: "Usually under a minute",
        risk: Risk::ReadOnly,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program(
            "dism",
            &["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"],
        )),
    }
}

fn dism_cleanup_resetbase() -> Operation {
    Operation {
        id: "dism-startcomponentcleanup-resetbase",
        label: "Clean up component store (/ResetBase)",
        description: "Removes superseded versions of updated components from WinSxS to reclaim disk \
                      space. /ResetBase means installed updates can no longer be uninstalled \
                      afterwards.",
        duration_hint: "Usually 5-20 minutes",
        risk: Risk::High,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program(
            "dism",
            &[
                "/Online",
                "/Cleanup-Image",
                "/StartComponentCleanup",
                "/ResetBase",
            ],
        )),
    }
}

fn reset_windows_update() -> Operation {
    Operation {
        id: "reset-windows-update",
        label: "Reset Windows Update",
        description: "Stops the Windows Update, BITS, and Cryptographic services, renames the \
                      SoftwareDistribution and catroot2 folders to .old (they are rebuilt \
                      automatically; nothing is deleted), then restarts the services. Use this \
                      when Windows Update is stuck or failing. A reboot afterwards is recommended.",
        duration_hint: "Under a minute",
        risk: Risk::Medium,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                RESET_UPDATE_SCRIPT,
            ],
        )),
    }
}

// Stop on reset failures, but always attempt to restart every service. Report
// any restart failure as well, and preserve failure in the process exit code.
const RESET_UPDATE_SCRIPT: &str = concat!(
    "$failed = $false; ",
    "$services = @('wuauserv', 'bits', 'cryptsvc'); ",
    "try { ",
    "foreach ($service in $services) { ",
    "Write-Output ('Stopping ' + $service); ",
    "Stop-Service -Name $service -ErrorAction Stop }; ",
    "Rename-Item -LiteralPath ($env:SystemRoot + '\\SoftwareDistribution') ",
    "-NewName 'SoftwareDistribution.old' -ErrorAction Stop; ",
    "Rename-Item -LiteralPath ($env:SystemRoot + '\\System32\\catroot2') ",
    "-NewName 'catroot2.old' -ErrorAction Stop; ",
    "} catch { $failed = $true; Write-Output ('Reset failed: ' + $_.Exception.Message) } ",
    "finally { foreach ($service in @('cryptsvc', 'bits', 'wuauserv')) { ",
    "try { Write-Output ('Starting ' + $service); ",
    "Start-Service -Name $service -ErrorAction Stop } ",
    "catch { $failed = $true; Write-Output ('Restart failed: ' + $_.Exception.Message) } ",
    "} }; ",
    "if ($failed) { exit 1 }; Write-Output 'Windows Update reset completed.'; exit 0",
);

fn create_restore_point() -> Operation {
    Operation {
        id: "create-restore-point",
        label: "Create a System Restore point",
        description: "Creates a restore point you can roll back to before making other changes. If \
                      System Protection is turned off for the system drive, or a restore point was \
                      already created in the last 24 hours, Windows will not create one — the \
                      result is reported here.",
        duration_hint: "Usually under a minute",
        risk: Risk::Low,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::Script(RESTORE_POINT_SCRIPT.to_owned())),
    }
}

/// PowerShell that creates a restore point and reports the real outcome:
/// success, the "no restore point created" cases (24-hour frequency limit via
/// the warning stream, or one already present), or the error when System
/// Protection is off. This makes a silent no-op impossible.
const RESTORE_POINT_SCRIPT: &str = concat!(
    "powershell -NoProfile -ExecutionPolicy Bypass -Command ",
    "\"try { ",
    "$b = @(Get-ComputerRestorePoint).Count; ",
    "Checkpoint-Computer -Description 'Windows Toolkit' -RestorePointType MODIFY_SETTINGS ",
    "-WarningVariable w -WarningAction SilentlyContinue; ",
    "$a = @(Get-ComputerRestorePoint).Count; ",
    "if ($a -gt $b) { 'Restore point created successfully.' } ",
    "elseif ($w) { 'No restore point created: ' + ($w -join '; ') } ",
    "else { 'No restore point created. A recent one may already exist, ",
    "or System Protection is off for the system drive.' } ",
    "} catch { 'Could not create a restore point: ' + $_.Exception.Message }\"",
);

/// The Network catalog: DNS/stack resets and configuration display.
#[must_use]
pub fn network_operations() -> Vec<Operation> {
    vec![
        Operation {
            id: "ipconfig-all",
            label: "Show network configuration (ipconfig /all)",
            description: "Displays the full network configuration for every adapter. Read-only.",
            duration_hint: "",
            risk: Risk::ReadOnly,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("ipconfig", &["/all"])),
        },
        Operation {
            id: "flush-dns",
            label: "Flush DNS resolver cache",
            description: "Clears cached DNS lookups. Safe and instant; fixes stale or wrong \
                          name-resolution results.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("ipconfig", &["/flushdns"])),
        },
        Operation {
            id: "release-ip",
            label: "Release IP address",
            description: "Releases the current DHCP lease on all adapters. Your connection will \
                          drop until you renew.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("ipconfig", &["/release"])),
        },
        Operation {
            id: "renew-ip",
            label: "Renew IP address",
            description: "Requests a fresh DHCP lease on all adapters.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("ipconfig", &["/renew"])),
        },
        Operation {
            id: "reset-winsock",
            label: "Reset Winsock catalog",
            description: "Resets the Windows Sockets catalog to a clean state, which can fix \
                          connectivity broken by misbehaving network software. Requires a reboot \
                          to take effect and may reset settings added by VPN or firewall software.",
            duration_hint: "",
            risk: Risk::Medium,
            elevation: Elevation::Administrator,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("netsh", &["winsock", "reset"])),
        },
        Operation {
            id: "reset-tcpip",
            label: "Reset TCP/IP stack",
            description: "Rewrites the TCP/IP stack registry keys to their defaults. Requires a \
                          reboot to take effect and will clear custom IP settings such as static \
                          addresses.",
            duration_hint: "",
            risk: Risk::Medium,
            elevation: Elevation::Administrator,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("netsh", &["int", "ip", "reset"])),
        },
    ]
}

/// The Performance catalog: power-plan selection and the documented memory
/// compression toggle. These change documented Windows settings and make no
/// performance claims.
#[must_use]
pub fn performance_operations() -> Vec<Operation> {
    vec![
        Operation {
            id: "powercfg-list",
            label: "Show power plans",
            description: "Lists the available power plans and marks the active one. Read-only.",
            duration_hint: "",
            risk: Risk::ReadOnly,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program("powercfg", &["/list"])),
        },
        Operation {
            id: "powercfg-balanced",
            label: "Set Balanced power plan",
            description: "Switches the active power plan to Balanced, the Windows default.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powercfg",
                &["/setactive", "scheme_balanced"],
            )),
        },
        Operation {
            id: "powercfg-high",
            label: "Set High performance power plan",
            description: "Switches the active power plan to High performance. May increase power \
                          use; has no effect on desktops without the plan available.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powercfg",
                &["/setactive", "scheme_min"],
            )),
        },
        Operation {
            id: "powercfg-ultimate",
            label: "Add Ultimate Performance power plan",
            description: "Adds Microsoft's hidden Ultimate Performance plan so it can be selected. \
                          Adds a documented plan; it does not change your active plan.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powercfg",
                &["-duplicatescheme", "e9a42b02-d5df-448d-aa00-03f14749eb61"],
            )),
        },
        Operation {
            id: "mmagent-status",
            label: "Show memory compression state",
            description: "Reports whether Windows memory compression is currently enabled. \
                          Read-only, but Get-MMAgent requires Administrator rights.",
            duration_hint: "",
            risk: Risk::ReadOnly,
            elevation: Elevation::Administrator,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powershell",
                &["-NoProfile", "-Command", "(Get-MMAgent).MemoryCompression"],
            )),
        },
        Operation {
            id: "mmagent-enable",
            label: "Enable memory compression",
            description: "Turns on the documented Windows memory-compression setting (the default \
                          on most systems). This toggles a Windows setting; it is not a performance \
                          boost.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::Administrator,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powershell",
                &["-NoProfile", "-Command", "Enable-MMAgent -mc"],
            )),
        },
        Operation {
            id: "mmagent-disable",
            label: "Disable memory compression",
            description: "Turns off the documented Windows memory-compression setting. This toggles \
                          a Windows setting; it is not a performance boost and may increase memory \
                          pressure.",
            duration_hint: "",
            risk: Risk::Low,
            elevation: Elevation::Administrator,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powershell",
                &["-NoProfile", "-Command", "Disable-MMAgent -mc"],
            )),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn simulate_update_reset(rename_fails: bool, restart_fails: bool) -> std::process::Output {
        // Override only the three mutating cmdlets. No services or files are changed.
        let rename = if rename_fails {
            "throw 'rename blocked'"
        } else {
            "'renamed'"
        };
        let restart = if restart_fails {
            "throw 'restart blocked'"
        } else {
            "'restarted'"
        };
        let script = format!(
            "function Stop-Service {{ 'stopped' }}; function Rename-Item {{ {rename} }}; \
             function Start-Service {{ {restart} }}; {RESET_UPDATE_SCRIPT}"
        );
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn update_reset_reports_failure_and_attempts_every_restart() {
        for (rename_fails, restart_fails) in [(true, false), (false, true)] {
            let output = simulate_update_reset(rename_fails, restart_fails);
            assert_eq!(output.status.code(), Some(1));
            let text = String::from_utf8_lossy(&output.stdout);
            assert_eq!(text.matches("Starting ").count(), 3);
            assert!(!text.contains("reset completed"));
            assert!(text.contains(if rename_fails {
                "rename blocked"
            } else {
                "restart blocked"
            }));
        }
    }

    #[cfg(windows)]
    #[test]
    fn update_reset_reports_success_after_both_renames() {
        let output = simulate_update_reset(false, false);
        assert!(output.status.success());
        let text = String::from_utf8_lossy(&output.stdout);
        assert_eq!(text.matches("renamed").count(), 2);
        assert_eq!(text.matches("restarted").count(), 3);
        assert!(text.contains("reset completed"));
    }

    #[test]
    fn sfc_is_the_first_elevated_health_operation() {
        let health = health_operations();
        let sfc = &health[0];
        assert_eq!(sfc.id, "sfc-scannow");
        assert_eq!(sfc.elevation, Elevation::Administrator);
        assert!(!sfc.cancelable);
        assert!(sfc.is_capture());
    }

    #[test]
    fn read_only_network_and_performance_queries_are_unelevated() {
        let ipconfig = network_operations()
            .into_iter()
            .find(|op| op.id == "ipconfig-all")
            .unwrap();
        assert_eq!(ipconfig.elevation, Elevation::None);
        let plans = performance_operations()
            .into_iter()
            .find(|op| op.id == "powercfg-list")
            .unwrap();
        assert_eq!(plans.elevation, Elevation::None);
    }

    #[test]
    fn resets_that_need_a_reboot_are_elevated_and_medium_risk() {
        for id in ["reset-winsock", "reset-tcpip"] {
            let op = network_operations()
                .into_iter()
                .find(|op| op.id == id)
                .unwrap();
            assert_eq!(op.elevation, Elevation::Administrator);
            assert_eq!(op.risk, Risk::Medium);
        }
    }

    #[test]
    fn launcher_operations_are_unelevated_launches() {
        let op = launcher(
            "open-x",
            "Open X",
            "Opens X.",
            "dxdiag".to_owned(),
            Vec::new(),
        );
        assert_eq!(op.elevation, Elevation::None);
        assert!(!op.is_capture());
    }
}
