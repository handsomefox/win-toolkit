//! The eframe application shell: the section sidebar, per-section views, and the
//! confirm-then-run flow that drives operations through the worker.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use toolkit_core::{
    Elevation, Operation, Risk, health_operations, launcher, network_operations,
    performance_operations,
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
                Section::About => self.draw_about(ui),
                other => Self::draw_placeholder(ui, other),
            });
    }

    fn draw_placeholder(ui: &mut egui::Ui, section: Section) {
        ui.heading(section.label());
        ui.add_space(theme::SPACE_MD);
        ui.label(RichText::new("Coming in a later version.").color(theme::MUTED));
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
