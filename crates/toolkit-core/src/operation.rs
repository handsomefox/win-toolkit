//! The operation model: a single, described maintenance or diagnostic action
//! the toolkit can run, plus the built-in catalogs for the command-driven
//! sections.
//!
//! Operations are plain data. The UI renders them (label, description,
//! consequences) and the platform layer executes them. Adding a tool is a matter
//! of adding operations to a catalog — there is no plugin system or dynamic
//! registry.

use crate::command::{CommandLine, POWERSHELL};
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
        label: "Check system drive (chkdsk /scan)",
        description: "Scans the drive Windows is installed on for file-system errors online, \
                      without locking it or requiring a reboot. Reports problems it finds.",
        duration_hint: "Usually a few minutes",
        risk: Risk::Medium,
        elevation: Elevation::Administrator,
        cancelable: false,
        // Elevated captures run through `cmd.exe`, which expands
        // `%SystemDrive%` to the drive Windows is installed on.
        execution: Execution::Capture(CommandLine::program("chkdsk", &["%SystemDrive%", "/scan"])),
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
        description: "Stops the Windows Update, BITS, and Cryptographic services, along with \
                      the running services that depend on them, such as Application Identity, \
                      which pauses AppLocker for the moment. It renames the \
                      SoftwareDistribution and catroot2 folders to .old (they are rebuilt \
                      automatically; nothing is deleted), then restarts every service it \
                      stopped. When an earlier reset left .old folders, they stay and this one \
                      uses .old1, .old2, and so on; delete them yourself once Windows Update \
                      works. Use this when Windows Update is stuck or failing. A reboot \
                      afterwards is recommended.",
        duration_hint: "Under a minute",
        risk: Risk::Medium,
        elevation: Elevation::Administrator,
        cancelable: false,
        execution: Execution::Capture(CommandLine::program(
            POWERSHELL,
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
//
// Application Identity and the AppLocker filter depend on the Cryptographic
// service, and Stop-Service refuses to stop a service whose dependents run
// unless -Force stops those too. DependentServices lists the indirect
// dependents as well. The ones that were running are started again afterwards.
const RESET_UPDATE_SCRIPT: &str = concat!(
    "$failed = $false; ",
    "$services = @('wuauserv', 'bits', 'cryptsvc'); ",
    "$dependents = @(Get-Service -Name $services | ForEach-Object { $_.DependentServices } | ",
    "Where-Object { $_.Status -eq 'Running' } | ForEach-Object { $_.Name } | Select-Object -Unique); ",
    "try { ",
    "foreach ($service in $services) { ",
    "Write-Output ('Stopping ' + $service); ",
    "Stop-Service -Name $service -Force -ErrorAction Stop }; ",
    // An earlier reset left .old folders behind. Nothing is deleted, so pick
    // the first suffix free for both folders: .old, then .old1, .old2, ...
    "$suffix = 'old'; $n = 0; ",
    "while ((Test-Path -LiteralPath ($env:SystemRoot + '\\SoftwareDistribution.' + $suffix)) -or ",
    "(Test-Path -LiteralPath ($env:SystemRoot + '\\System32\\catroot2.' + $suffix))) { ",
    "$n++; $suffix = 'old' + $n }; ",
    "Write-Output ('Renaming the folders to .' + $suffix); ",
    "Rename-Item -LiteralPath ($env:SystemRoot + '\\SoftwareDistribution') ",
    "-NewName ('SoftwareDistribution.' + $suffix) -ErrorAction Stop; ",
    "Rename-Item -LiteralPath ($env:SystemRoot + '\\System32\\catroot2') ",
    "-NewName ('catroot2.' + $suffix) -ErrorAction Stop; ",
    "} catch { $failed = $true; Write-Output ('Reset failed: ' + $_.Exception.Message) } ",
    "finally { foreach ($service in @('cryptsvc', 'bits', 'wuauserv') + $dependents) { ",
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
        execution: Execution::Capture(CommandLine::program(
            POWERSHELL,
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                RESTORE_POINT_SCRIPT,
            ],
        )),
    }
}

/// PowerShell that creates a restore point and reports the real outcome:
/// success, the "no restore point created" cases (24-hour frequency limit via
/// the warning stream, or one already present), or the error when System
/// Protection is off. This makes a silent no-op impossible. Every outcome but
/// success exits with code 1, so the run is reported as failed.
///
/// A point counts as new when its sequence number is higher than any before
/// the run and it carries this app's description. Counting points instead
/// would miss one, because Windows can delete the oldest point to make room.
const RESTORE_POINT_SCRIPT: &str = concat!(
    "try { ",
    "$b = (Get-ComputerRestorePoint | Measure-Object -Property SequenceNumber -Maximum).Maximum; ",
    "if ($null -eq $b) { $b = 0 }; ",
    "Checkpoint-Computer -Description 'Windows Toolkit' -RestorePointType MODIFY_SETTINGS ",
    "-WarningVariable w -WarningAction SilentlyContinue; ",
    "$a = @(Get-ComputerRestorePoint | Where-Object { $_.SequenceNumber -gt $b -and ",
    "$_.Description -eq 'Windows Toolkit' }); ",
    "if ($a.Count -gt 0) { 'Restore point created successfully.' } ",
    "elseif ($w) { 'No restore point created: ' + ($w -join '; '); exit 1 } ",
    "else { 'No restore point created. A recent one may already exist, ",
    "or System Protection is off for the system drive.'; exit 1 } ",
    "} catch { 'Could not create a restore point: ' + $_.Exception.Message; exit 1 }",
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
                POWERSHELL,
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
                POWERSHELL,
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
                POWERSHELL,
                &["-NoProfile", "-Command", "Disable-MMAgent -mc"],
            )),
        },
    ]
}

/// The Diagnostics catalog, which mixes a captured report generator with
/// launchers for built-in Windows tools and the app's own files.
/// `battery_report` is the file powercfg writes, when the app has a folder for
/// it; `system_root` is the Windows directory; `logs_dir` holds this app's
/// diagnostics logs, when it has one.
#[must_use]
pub fn diagnostics_operations(
    battery_report: Option<&str>,
    system_root: &str,
    logs_dir: Option<&str>,
) -> Vec<Operation> {
    let mut ops = Vec::new();

    if let Some(report_file) = battery_report {
        ops.push(Operation {
            id: "battery-report",
            label: "Generate battery report",
            description: "Runs powercfg to write a detailed battery health and usage report, then \
                          use 'Open battery report' to view it.",
            duration_hint: "",
            risk: Risk::ReadOnly,
            elevation: Elevation::None,
            cancelable: false,
            execution: Execution::Capture(CommandLine::program(
                "powercfg",
                &["/batteryreport", "/output", report_file],
            )),
        });
        ops.push(launcher(
            "open-battery-report",
            "Open battery report",
            "Opens the most recently generated battery report in your browser.",
            report_file.to_owned(),
            Vec::new(),
        ));
    }

    ops.push(launcher(
        "open-dxdiag",
        "Open DirectX Diagnostic Tool",
        "Opens dxdiag, which reports display, sound, and input device information.",
        "dxdiag".to_owned(),
        Vec::new(),
    ));
    ops.push(launcher(
        "open-eventvwr",
        "Open Event Viewer",
        "Opens the Windows Event Viewer to inspect system and application logs.",
        "eventvwr".to_owned(),
        Vec::new(),
    ));
    ops.push(launcher(
        "open-reliability",
        "Open Reliability Monitor",
        "Opens Reliability Monitor, a timeline of crashes and warnings.",
        "perfmon".to_owned(),
        vec!["/rel".to_owned()],
    ));

    ops.push(launcher(
        "open-cbs-log",
        "Open servicing log (CBS.log)",
        "Opens the component servicing log written by SFC and DISM.",
        format!(r"{system_root}\Logs\CBS\CBS.log"),
        Vec::new(),
    ));

    if let Some(dir) = logs_dir {
        ops.push(open_logs_folder(dir));
    }

    ops
}

/// Opens the folder that holds this app's diagnostics logs.
#[must_use]
pub fn open_logs_folder(dir: &str) -> Operation {
    launcher(
        "open-logs",
        "Open logs folder",
        "Opens the folder containing this app's diagnostics logs.",
        dir.to_owned(),
        Vec::new(),
    )
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
        // A running dependent of the Cryptographic service, as Application
        // Identity is on Windows 11, and one of BITS. Windows Update shares
        // Application Identity, which is started once.
        let script = format!(
            "function Get-Service {{ param($Name) $Name | ForEach-Object {{ \
             $running = @{{ cryptsvc = 'AppIDSvc'; bits = 'BitsDependent'; wuauserv = 'AppIDSvc' }}; \
             [pscustomobject]@{{ DependentServices = @( \
             [pscustomobject]@{{ Name = $running[$_]; Status = 'Running' }}, \
             [pscustomobject]@{{ Name = 'Stopped1'; Status = 'Stopped' }}) }} }} }}; \
             function Stop-Service {{ 'stopped' }}; function Rename-Item {{ {rename} }}; \
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
            assert_eq!(text.matches("Starting ").count(), 5);
            assert_eq!(text.matches("Starting AppIDSvc").count(), 1);
            assert!(text.contains("Starting BitsDependent"));
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
        // The three services, then the two dependents that were running.
        assert_eq!(text.matches("restarted").count(), 5);
        assert!(text.contains("reset completed"));
    }

    #[cfg(windows)]
    #[test]
    fn update_reset_keeps_the_folders_an_earlier_reset_left() {
        // `.old` is taken, `.old1` is free. Nothing is renamed for real.
        let script = format!(
            "function Get-Service {{ [pscustomobject]@{{ DependentServices = @() }} }}; \
             function Stop-Service {{ 'stopped' }}; \
             function Test-Path {{ param($LiteralPath) $LiteralPath -like '*.old' }}; \
             function Rename-Item {{ param($LiteralPath, $NewName) 'renamed to ' + $NewName }}; \
             function Start-Service {{ 'restarted' }}; {RESET_UPDATE_SCRIPT}"
        );
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("renamed to SoftwareDistribution.old1"),
            "{text}"
        );
        assert!(text.contains("renamed to catroot2.old1"), "{text}");
    }

    #[test]
    fn sfc_is_the_first_elevated_health_operation() {
        let health = health_groups();
        let sfc = &health[0].1[0];
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
    fn every_powershell_operation_names_it_for_resolution() {
        let operations = health_groups()
            .into_iter()
            .flat_map(|(_, ops)| ops)
            .chain(network_operations())
            .chain(performance_operations())
            .chain(diagnostics_operations(
                Some("report.html"),
                r"C:\Windows",
                Some("logs"),
            ));
        let mut found = 0;
        for op in operations {
            let Execution::Capture(command) = &op.execution else {
                continue;
            };
            let program = match command {
                CommandLine::Program(spec) => spec.program.as_str(),
                CommandLine::Script(script) => script.as_str(),
            };
            if program.to_ascii_lowercase().contains("powershell") {
                assert_eq!(program, POWERSHELL, "{}", op.id);
                found += 1;
            }
        }
        // Reset Windows Update, the restore point, and three memory compression ones.
        assert_eq!(found, 5);
    }

    #[test]
    fn restore_point_errors_exit_non_zero() {
        let op = create_restore_point();
        let Execution::Capture(CommandLine::Program(spec)) = &op.execution else {
            panic!("the restore point runs as a captured PowerShell command");
        };
        assert_eq!(spec.program, POWERSHELL);
        let script = spec.args.last().unwrap();
        assert!(script.ends_with(
            "} catch { 'Could not create a restore point: ' + $_.Exception.Message; exit 1 }"
        ));
        // Both "no restore point created" branches fail the run too; only the
        // success branch exits 0.
        assert_eq!(script.matches("exit 1").count(), 3);
        assert!(script.contains("{ 'Restore point created successfully.' }"));
        // Success is judged by a new sequence number, not by the count.
        assert!(script.contains("SequenceNumber -gt $b"));
        assert!(!script.contains(".Count; "));
        // The script sits inside `cmd.exe`'s double-quoted `/c` argument.
        assert!(!script.contains('"'));
    }

    #[test]
    fn chkdsk_checks_the_drive_windows_is_on() {
        let op = health_groups()
            .into_iter()
            .flat_map(|(_, ops)| ops)
            .find(|op| op.id == "chkdsk-scan")
            .unwrap();
        assert!(!op.label.contains("C:"));
        let Execution::Capture(command) = &op.execution else {
            panic!("chkdsk runs as a captured command");
        };
        assert_eq!(
            crate::command::elevated_cmd_parameters(command, r"C:\out.txt"),
            r#"/d /c "(chkdsk %SystemDrive% /scan) > "C:\out.txt" 2>&1""#
        );
    }

    #[test]
    fn battery_report_passes_its_output_path_as_one_argument() {
        let report = r"C:\Users\John Doe\AppData\Local\win-toolkit\reports\battery-report.html";
        let ops = diagnostics_operations(Some(report), r"C:\Windows", None);
        let generate = ops.iter().find(|op| op.id == "battery-report").unwrap();
        assert_eq!(generate.elevation, Elevation::None);
        assert_eq!(
            generate.execution,
            Execution::Capture(CommandLine::program(
                "powercfg",
                &["/batteryreport", "/output", report],
            ))
        );
        assert!(!ops.iter().any(|op| op.id == "open-logs"));
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
