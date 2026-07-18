//! The eframe application shell: the section sidebar, per-section views, and the
//! confirm-then-run flow that drives operations through the worker.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use toolkit_core::{
    Elevation, Operation, Risk, SandboxConfig, StartupEntry, StartupScope, SystemInfo,
    format_bytes, health_operations, launcher, network_operations, performance_operations,
};

use crate::theme;
use crate::worker::{self, Command, Event, Outcome, Worker};

/// The sidebar sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Overview,
    Health,
    Network,
    Startup,
    Performance,
    Sandbox,
    Diagnostics,
    About,
}

impl Section {
    const ALL: [Section; 8] = [
        Section::Overview,
        Section::Health,
        Section::Network,
        Section::Startup,
        Section::Performance,
        Section::Sandbox,
        Section::Diagnostics,
        Section::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::Overview => "Overview",
            Section::Health => "Health & Repair",
            Section::Network => "Network",
            Section::Startup => "Startup",
            Section::Performance => "Performance",
            Section::Sandbox => "Sandbox",
            Section::Diagnostics => "Diagnostics",
            Section::About => "About",
        }
    }
}

/// The state of the most recent (or current) operation run.
struct RunView {
    label: &'static str,
    is_capture: bool,
    status: RunStatus,
    lines: Vec<String>,
}

enum RunStatus {
    Running,
    Success,
    Failed(i32),
    Declined,
    Error(String),
}

pub(crate) struct ToolkitApp {
    worker: Worker,
    section: Section,
    run: Option<RunView>,
    confirm: Option<Operation>,
    log_path: Option<PathBuf>,
    overview: Option<SystemInfo>,
    sandbox: SandboxConfig,
    startup: Option<Vec<StartupEntry>>,
    health: Vec<Operation>,
    network: Vec<Operation>,
    performance: Vec<Operation>,
    diagnostics: Vec<Operation>,
}

impl ToolkitApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, log_path: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        Self {
            worker: worker::spawn(cc.egui_ctx.clone()),
            section: Section::Health,
            run: None,
            confirm: None,
            overview: None,
            sandbox: SandboxConfig::default(),
            startup: None,
            diagnostics: build_diagnostics(log_path.as_deref()),
            log_path,
            health: health_operations(),
            network: network_operations(),
            performance: performance_operations(),
        }
    }

    fn is_running(&self) -> bool {
        matches!(
            self.run.as_ref().map(|run| &run.status),
            Some(RunStatus::Running)
        )
    }

    fn drain_events(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            match event {
                Event::Started => {}
                Event::Output { lines } => {
                    if let Some(run) = &mut self.run {
                        run.lines = lines;
                    }
                }
                Event::Finished(outcome) => {
                    if let Some(run) = &mut self.run {
                        run.status = match outcome {
                            Outcome::Success => RunStatus::Success,
                            Outcome::Failed(code) => RunStatus::Failed(code),
                            Outcome::Declined => RunStatus::Declined,
                            Outcome::Error(message) => RunStatus::Error(message),
                        };
                    }
                }
            }
        }
    }

    fn on_operation_clicked(&mut self, operation: Operation) {
        if needs_confirmation(&operation) {
            self.confirm = Some(operation);
        } else {
            self.start(operation);
        }
    }

    fn start(&mut self, operation: Operation) {
        self.run = Some(RunView {
            label: operation.label,
            is_capture: operation.is_capture(),
            status: RunStatus::Running,
            lines: Vec::new(),
        });
        self.worker.send(Command::Run(operation));
    }

    fn draw_header(root: &mut egui::Ui) {
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(root, |ui| {
                ui.label(
                    RichText::new(toolkit_core::APP_TITLE)
                        .family(theme::bold())
                        .size(theme::FONT_HEADING),
                );
                ui.label(
                    RichText::new("System inspection, diagnostics, and documented maintenance")
                        .color(theme::MUTED),
                );
            });
    }

    fn draw_sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(theme::SIDEBAR_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(10, 12)),
            )
            .show(root, |ui| {
                for section in Section::ALL {
                    ui.selectable_value(&mut self.section, section, section.label());
                    ui.add_space(theme::SPACE_XS);
                }
            });
    }

    fn draw_central(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16))
            .show(root, |ui| match self.section {
                Section::Overview => self.draw_overview(ui),
                Section::Health => {
                    let ops = self.health.clone();
                    self.draw_operation_section(ui, "Health & Repair", &ops);
                }
                Section::Network => {
                    let ops = self.network.clone();
                    self.draw_operation_section(ui, "Network", &ops);
                }
                Section::Performance => {
                    let ops = self.performance.clone();
                    self.draw_operation_section(ui, "Performance", &ops);
                }
                Section::Diagnostics => {
                    let ops = self.diagnostics.clone();
                    self.draw_operation_section(ui, "Diagnostics", &ops);
                }
                Section::Sandbox => self.draw_sandbox(ui),
                Section::Startup => self.draw_startup(ui),
                Section::About => self.draw_about(ui),
            });
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui) {
        if self.overview.is_none() {
            self.overview = Some(toolkit_platform::system_info());
        }
        let Some(info) = self.overview.clone() else {
            return;
        };
        let mut refresh = false;
        let mut copy = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Overview");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Copy report").clicked() {
                            copy = true;
                        }
                        if ui.button("Refresh").clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(theme::SPACE_MD);
                info_row(
                    ui,
                    "OS",
                    &format!("{} (build {})", info.os_name, info.os_build),
                );
                info_row(ui, "Machine", &info.computer_name);
                info_row(ui, "Uptime", &info.uptime);
                info_row(
                    ui,
                    "CPU",
                    &format!("{} ({} logical)", info.cpu, info.logical_cpus),
                );
                info_row(
                    ui,
                    "Memory",
                    &format!(
                        "{} free of {}",
                        format_bytes(info.available_memory),
                        format_bytes(info.total_memory)
                    ),
                );
                ui.add_space(theme::SPACE_SM);
                ui.label(RichText::new("Drives").family(theme::bold()));
                for drive in &info.drives {
                    info_row(
                        ui,
                        &drive.name,
                        &format!(
                            "{} free of {}",
                            format_bytes(drive.free),
                            format_bytes(drive.total)
                        ),
                    );
                }
            });
        if refresh {
            self.overview = Some(toolkit_platform::system_info());
        }
        if copy {
            ui.ctx().copy_text(info.to_report());
        }
    }

    fn draw_sandbox(&mut self, ui: &mut egui::Ui) {
        ui.heading("Windows Sandbox");
        ui.add_space(theme::SPACE_MD);
        let available = sandbox_available();
        if !available {
            ui.label(
                RichText::new(
                    "Windows Sandbox is not installed. Enable the 'Windows Sandbox' optional \
                     feature (Windows Pro or Enterprise) to use this.",
                )
                .color(theme::WARNING),
            );
            ui.add_space(theme::SPACE_SM);
        }
        ui.label(
            RichText::new("Build a disposable, isolated Windows environment.").color(theme::MUTED),
        );
        ui.add_space(theme::SPACE_MD);

        let mut memory_gb = (self.sandbox.memory_mb / 1024).max(1);
        ui.horizontal(|ui| {
            ui.add_sized(
                [110.0, theme::FONT_BODY],
                egui::Label::new(RichText::new("Memory").color(theme::MUTED)),
            );
            ui.add(egui::Slider::new(&mut memory_gb, 1..=16).suffix(" GB"));
        });
        self.sandbox.memory_mb = memory_gb * 1024;
        ui.checkbox(&mut self.sandbox.vgpu, "Virtual GPU (vGPU)");
        ui.checkbox(&mut self.sandbox.networking, "Networking");
        ui.add_space(theme::SPACE_MD);

        let launch = ui
            .add_enabled(
                available && !self.is_running(),
                egui::Button::new("Launch sandbox"),
            )
            .clicked();
        if launch {
            self.launch_sandbox();
        }

        if let Some(run) = &self.run {
            ui.add_space(theme::SPACE_MD);
            ui.separator();
            ui.add_space(theme::SPACE_SM);
            draw_run_output(ui, run);
        }
    }

    fn launch_sandbox(&mut self) {
        match write_sandbox_config(self.sandbox) {
            Ok(path) => self.start(launcher(
                "launch-sandbox",
                "Windows Sandbox",
                "Launches Windows Sandbox with the selected configuration.",
                path,
                Vec::new(),
            )),
            Err(message) => {
                self.run = Some(RunView {
                    label: "Windows Sandbox",
                    is_capture: false,
                    status: RunStatus::Error(message),
                    lines: Vec::new(),
                });
            }
        }
    }

    fn draw_startup(&mut self, ui: &mut egui::Ui) {
        if self.startup.is_none() {
            self.startup = Some(toolkit_platform::list_startup());
        }
        let entries = self.startup.clone().unwrap_or_default();
        let mut refresh = false;
        let mut toggle: Option<(StartupScope, String, bool)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Startup");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Refresh").clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.add_space(theme::SPACE_MD);
                if entries.is_empty() {
                    ui.label(RichText::new("No startup entries found.").color(theme::MUTED));
                }
                for entry in &entries {
                    if let Some(change) = startup_card(ui, entry) {
                        toggle = Some((entry.scope, entry.name.clone(), change));
                    }
                    ui.add_space(theme::SPACE_SM);
                }
            });
        if refresh {
            self.startup = Some(toolkit_platform::list_startup());
        }
        if let Some((scope, name, enabled)) = toggle {
            match toolkit_platform::set_startup_enabled(scope, &name, enabled) {
                Ok(()) => self.startup = Some(toolkit_platform::list_startup()),
                Err(message) => {
                    self.run = Some(RunView {
                        label: "Startup",
                        is_capture: false,
                        status: RunStatus::Error(message),
                        lines: Vec::new(),
                    });
                }
            }
        }
    }

    fn draw_operation_section(&mut self, ui: &mut egui::Ui, title: &str, ops: &[Operation]) {
        let running = self.is_running();
        let mut clicked: Option<Operation> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading(title);
                ui.add_space(theme::SPACE_MD);
                for operation in ops {
                    if operation_card(ui, operation, running) {
                        clicked = Some(operation.clone());
                    }
                    ui.add_space(theme::SPACE_SM);
                }
                if let Some(run) = &self.run {
                    ui.add_space(theme::SPACE_SM);
                    ui.separator();
                    ui.add_space(theme::SPACE_SM);
                    draw_run_output(ui, run);
                }
            });
        if let Some(operation) = clicked {
            self.on_operation_clicked(operation);
        }
    }

    fn draw_about(&self, ui: &mut egui::Ui) {
        ui.heading(toolkit_core::APP_TITLE);
        ui.add_space(theme::SPACE_SM);
        ui.label(RichText::new(concat!("Version ", env!("CARGO_PKG_VERSION"))).color(theme::MUTED));
        ui.add_space(theme::SPACE_SM);
        ui.hyperlink("https://github.com/handsomefox/win-toolkit");
        ui.add_space(theme::SPACE_MD);
        if let Some(path) = &self.log_path {
            ui.label(RichText::new("Diagnostics log").family(theme::bold()));
            ui.label(RichText::new(path.display().to_string()).color(theme::MUTED));
        }
        ui.add_space(theme::SPACE_MD);
        ui.label(
            RichText::new(
                "Bundled fonts keep their own licenses: Inter (SIL OFL 1.1) and Phosphor (MIT).",
            )
            .color(theme::MUTED),
        );
    }

    fn draw_confirm(&mut self, ctx: &egui::Context) {
        let Some(operation) = self.confirm.clone() else {
            return;
        };
        let mut run = false;
        let mut cancel = false;
        egui::Window::new("Confirm operation")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(RichText::new(operation.label).family(theme::bold()));
                ui.add_space(theme::SPACE_SM);
                ui.label(operation.description);
                ui.add_space(theme::SPACE_SM);
                if !operation.duration_hint.is_empty() {
                    ui.label(RichText::new(operation.duration_hint).color(theme::MUTED));
                }
                if operation.elevation == Elevation::Administrator {
                    ui.label(
                        RichText::new("This will request Administrator rights (a UAC prompt).")
                            .color(theme::WARNING),
                    );
                }
                if operation.is_capture() && !operation.cancelable {
                    ui.label(
                        RichText::new("This cannot be cancelled once started.")
                            .color(theme::WARNING),
                    );
                }
                ui.add_space(theme::SPACE_MD);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui.button("Run").clicked() {
                        run = true;
                    }
                });
            });
        if run {
            self.confirm = None;
            self.start(operation);
        } else if cancel {
            self.confirm = None;
        }
    }
}

/// Draws one startup entry as a card. Returns the requested new enabled state if
/// the user toggled it.
fn startup_card(ui: &mut egui::Ui, entry: &StartupEntry) -> Option<bool> {
    let mut toggled = None;
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(theme::RADIUS_MD)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&entry.name).family(theme::bold()));
                    ui.label(RichText::new(&entry.command).color(theme::MUTED));
                    ui.label(RichText::new(entry.scope.label()).color(theme::MUTED));
                    if !entry.can_toggle {
                        ui.label(
                            RichText::new("System-wide; requires Administrator to change")
                                .color(theme::MUTED),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = entry.enabled;
                    if ui
                        .add_enabled(
                            entry.can_toggle,
                            egui::Checkbox::new(&mut enabled, "Enabled"),
                        )
                        .changed()
                    {
                        toggled = Some(enabled);
                    }
                });
            });
        });
    toggled
}

/// Whether the Windows Sandbox optional feature appears to be installed.
fn sandbox_available() -> bool {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    std::path::Path::new(&format!(r"{system_root}\System32\WindowsSandbox.exe")).exists()
}

/// Writes the sandbox configuration to a `.wsb` file and returns its path.
fn write_sandbox_config(config: SandboxConfig) -> Result<String, String> {
    let dir = toolkit_platform::sandbox_dir()
        .ok_or_else(|| "could not resolve the sandbox directory".to_owned())?;
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join("session.wsb");
    std::fs::write(&path, config.to_wsb()).map_err(|err| err.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Draws a labelled read-only field: a fixed-width muted label and its value.
fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, theme::FONT_BODY],
            egui::Label::new(RichText::new(label).color(theme::MUTED)),
        );
        ui.label(value);
    });
}

/// Whether an operation should show a confirmation dialog before running:
/// anything elevated or with more-than-low consequences.
fn needs_confirmation(operation: &Operation) -> bool {
    operation.elevation == Elevation::Administrator
        || matches!(operation.risk, Risk::Medium | Risk::High)
}

/// Draws one operation as a card and returns whether its action was clicked.
fn operation_card(ui: &mut egui::Ui, operation: &Operation, running: bool) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(theme::RADIUS_MD)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.label(
                RichText::new(operation.label)
                    .family(theme::bold())
                    .size(theme::FONT_BODY),
            );
            ui.add_space(theme::SPACE_XS);
            ui.label(RichText::new(operation.description).color(theme::MUTED));
            if !operation.duration_hint.is_empty() {
                ui.add_space(theme::SPACE_XS);
                ui.label(RichText::new(operation.duration_hint).color(theme::MUTED));
            }
            if operation.elevation == Elevation::Administrator {
                ui.label(RichText::new("Requires Administrator").color(theme::WARNING));
            }
            ui.add_space(theme::SPACE_SM);
            let action = if operation.is_capture() {
                "Run"
            } else {
                "Open"
            };
            if ui
                .add_enabled(!running, egui::Button::new(action))
                .clicked()
            {
                clicked = true;
            }
        });
    clicked
}

fn draw_run_output(ui: &mut egui::Ui, run: &RunView) {
    ui.label(RichText::new(run.label).family(theme::bold()));
    ui.add_space(theme::SPACE_XS);
    let (text, color) = status_text(&run.status);
    ui.label(RichText::new(text).color(color));
    if run.is_capture {
        ui.add_space(theme::SPACE_SM);
        let running = matches!(run.status, RunStatus::Running);
        egui::ScrollArea::vertical()
            .id_salt("run-output")
            .max_height(320.0)
            .auto_shrink([false, false])
            .stick_to_bottom(running)
            .show(ui, |ui| {
                ui.style_mut().interaction.selectable_labels = true;
                for line in &run.lines {
                    ui.monospace(line);
                }
            });
    }
}

fn status_text(status: &RunStatus) -> (String, egui::Color32) {
    match status {
        RunStatus::Running => ("Running…".to_owned(), theme::WARNING),
        RunStatus::Success => ("Completed successfully.".to_owned(), theme::SUCCESS),
        RunStatus::Failed(code) => (format!("Exited with code {code}."), theme::DANGER),
        RunStatus::Declined => (
            "Elevation was declined; nothing was run.".to_owned(),
            theme::MUTED,
        ),
        RunStatus::Error(message) => (format!("Could not run: {message}"), theme::DANGER),
    }
}

/// Builds the Diagnostics catalog, which mixes a captured report generator with
/// launchers for built-in Windows tools and the app's own files.
fn build_diagnostics(log_path: Option<&std::path::Path>) -> Vec<Operation> {
    let mut ops = Vec::new();

    if let Some(reports) = toolkit_platform::app_data_dir().map(|dir| dir.join("reports")) {
        let _ = std::fs::create_dir_all(&reports);
        let report_file = reports.join("battery-report.html");
        ops.push(Operation {
            id: "battery-report",
            label: "Generate battery report",
            description: "Runs powercfg to write a detailed battery health and usage report, then \
                          use 'Open battery report' to view it.",
            duration_hint: "",
            risk: Risk::ReadOnly,
            elevation: Elevation::None,
            cancelable: false,
            execution: toolkit_core::Execution::Capture(toolkit_core::CommandLine::program(
                "powercfg",
                &["/batteryreport", "/output", &report_file.to_string_lossy()],
            )),
        });
        ops.push(launcher(
            "open-battery-report",
            "Open battery report",
            "Opens the most recently generated battery report in your browser.",
            report_file.to_string_lossy().into_owned(),
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

    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    ops.push(launcher(
        "open-cbs-log",
        "Open servicing log (CBS.log)",
        "Opens the component servicing log written by SFC and DISM.",
        format!(r"{system_root}\Logs\CBS\CBS.log"),
        Vec::new(),
    ));

    if let Some(dir) = log_path.and_then(std::path::Path::parent) {
        ops.push(launcher(
            "open-logs",
            "Open logs folder",
            "Opens the folder containing this app's diagnostics logs.",
            dir.to_string_lossy().into_owned(),
            Vec::new(),
        ));
    }

    ops
}

impl eframe::App for ToolkitApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.drain_events();
        Self::draw_header(root);
        self.draw_sidebar(root);
        self.draw_central(root);
        self.draw_confirm(&ctx);
    }
}
