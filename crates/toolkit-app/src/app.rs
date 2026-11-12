//! The eframe application shell: the section sidebar, per-section views, and the
//! confirm-then-run flow that drives operations through the worker.

use std::path::{Path, PathBuf};

use crossbeam_channel::TryRecvError;
use eframe::egui::{self, RichText};
use toolkit_core::{
    Elevation, Operation, Risk, SandboxConfig, StartupEntry, StartupScope, SystemInfo,
    diagnostics_operations, format_bytes, health_groups, launcher, network_operations,
    open_logs_folder, performance_operations,
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
    Finished(Outcome),
}

impl RunView {
    fn running(operation: &Operation) -> Self {
        Self {
            label: operation.label,
            is_capture: operation.is_capture(),
            status: RunStatus::Running,
            lines: Vec::new(),
        }
    }

    fn failed(label: &'static str, message: String) -> Self {
        Self {
            label,
            is_capture: false,
            status: RunStatus::Finished(Outcome::Error(message)),
            lines: Vec::new(),
        }
    }
}

/// What the output panel shows: nothing, the operation the worker is running,
/// or how the last run ended. A running operation keeps its place until it
/// finishes, so the worker's events always reach the run they belong to, and
/// nothing can start while the worker is busy.
#[derive(Default)]
struct RunPanel {
    view: Option<RunView>,
}

impl RunPanel {
    fn is_running(&self) -> bool {
        matches!(
            self.view.as_ref().map(|run| &run.status),
            Some(RunStatus::Running)
        )
    }

    fn view(&self) -> Option<&RunView> {
        self.view.as_ref()
    }

    /// Shows `run` in place of a finished one. Returns false, leaving the
    /// panel as it is, while an operation runs.
    fn show(&mut self, run: RunView) -> bool {
        if self.is_running() {
            return false;
        }
        self.view = Some(run);
        true
    }

    /// Hides a finished run. A running one stays.
    fn dismiss(&mut self) {
        if !self.is_running() {
            self.view = None;
        }
    }

    /// Applies every waiting worker event. If the worker thread has stopped
    /// (a panic, which the log records), the run it left behind fails.
    fn drain(&mut self, worker: &Worker) {
        loop {
            match worker.try_recv() {
                Ok(event) => self.apply(event),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.worker_stopped();
                    return;
                }
            }
        }
    }

    fn worker_stopped(&mut self) {
        if self.is_running() {
            self.apply(Event::Finished(Outcome::Error(
                "the background worker stopped; the diagnostics log has the details".to_owned(),
            )));
        }
    }

    /// Applies a worker event to the running operation.
    fn apply(&mut self, event: Event) {
        let Some(run) = self
            .view
            .as_mut()
            .filter(|run| matches!(run.status, RunStatus::Running))
        else {
            return;
        };
        match event {
            Event::Output { lines } => run.lines = lines,
            Event::Finished(outcome) => run.status = RunStatus::Finished(outcome),
        }
    }
}

pub(crate) struct ToolkitApp {
    worker: Worker,
    section: Section,
    run: RunPanel,
    confirm: Option<Operation>,
    /// The folder that holds the diagnostics log, when file logging works.
    logs_dir: Option<PathBuf>,
    overview: Option<SystemInfo>,
    sandbox: SandboxConfig,
    /// Checked once: turning the feature on takes a reboot.
    sandbox_available: bool,
    startup: Option<Vec<StartupEntry>>,
    /// Why the last startup toggle failed. Shown in the Startup section, not
    /// the run panel, which a running operation keeps for itself.
    startup_error: Option<String>,
    health: Vec<(&'static str, Vec<Operation>)>,
    network: Vec<Operation>,
    performance: Vec<Operation>,
    diagnostics: Vec<Operation>,
    updater: app_update::Updater,
    update_settings: app_update::Settings,
    update_open: bool,
}

/// The storage key for [`ToolkitApp::update_settings`].
const UPDATE_SETTINGS_KEY: &str = "app_updates";

impl ToolkitApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, logs_dir: Option<PathBuf>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let update_settings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, UPDATE_SETTINGS_KEY))
            .unwrap_or_default();
        Self {
            updater: app_update::Updater::new(crate::APP_INFO, {
                let ctx = cc.egui_ctx.clone();
                move || ctx.request_repaint()
            }),
            update_settings,
            update_open: false,
            worker: worker::spawn(cc.egui_ctx.clone()),
            section: Section::Health,
            run: RunPanel::default(),
            confirm: None,
            overview: None,
            sandbox: SandboxConfig::default(),
            sandbox_available: sandbox_available(),
            startup: None,
            startup_error: None,
            diagnostics: build_diagnostics(logs_dir.as_deref()),
            logs_dir,
            health: health_groups(),
            network: network_operations(),
            performance: performance_operations(),
        }
    }

    fn is_running(&self) -> bool {
        self.run.is_running()
    }

    fn on_operation_clicked(&mut self, operation: Operation) {
        if needs_confirmation(&operation) {
            self.confirm = Some(operation);
        } else {
            self.start(operation);
        }
    }

    fn start(&mut self, operation: Operation) {
        if self.run.show(RunView::running(&operation)) {
            if !self.worker.send(Command::Run(operation)) {
                self.run.worker_stopped();
            }
        } else {
            tracing::warn!(
                operation = operation.id,
                "not started while another operation runs"
            );
        }
    }

    /// Reports a failure on the UI thread in the output panel.
    fn show_error(&mut self, label: &'static str, message: String) {
        tracing::warn!("{label}: {message}");
        self.run.show(RunView::failed(label, message));
    }

    fn draw_header(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(toolkit_core::APP_TITLE)
                                .family(theme::bold())
                                .size(theme::FONT_HEADING),
                        );
                        ui.label(
                            RichText::new(
                                "System inspection, diagnostics, and documented maintenance",
                            )
                            .color(theme::MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::update::chip(ui, &self.updater, &self.update_settings) {
                            self.update_open = true;
                        }
                    });
                });
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
        let running = self.is_running();
        let mut clicked = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(16))
            .show(root, |ui| match self.section {
                Section::Overview => self.draw_overview(ui),
                Section::Health => clicked = health_section(ui, &self.health, running),
                Section::Network => {
                    clicked = operation_section(ui, "Network", &self.network, running);
                }
                Section::Performance => {
                    clicked = operation_section(ui, "Performance", &self.performance, running);
                }
                Section::Diagnostics => {
                    clicked = operation_section(ui, "Diagnostics", &self.diagnostics, running);
                }
                Section::Sandbox => self.draw_sandbox(ui),
                Section::Startup => self.draw_startup(ui),
                Section::About => self.draw_about(ui),
            });
        if let Some(operation) = clicked {
            self.on_operation_clicked(operation);
        }
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui) {
        let info = &*self
            .overview
            .get_or_insert_with(toolkit_platform::system_info);
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
                card(ui, |ui| {
                    info_row(
                        ui,
                        "OS",
                        &format!("{} (build {})", info.os_name, info.os_build),
                    );
                    info_row(ui, "Machine", &info.computer_name);
                    if !info.system_model.is_empty() {
                        info_row(ui, "Model", &info.system_model);
                    }
                    info_row(ui, "Uptime", &info.uptime);
                    info_row(
                        ui,
                        "CPU",
                        &format!("{} ({} logical)", info.cpu, info.logical_cpus),
                    );
                    for (index, gpu) in info.gpus.iter().enumerate() {
                        let label = if index == 0 { "GPU" } else { "" };
                        info_row(ui, label, gpu);
                    }
                    info_row(
                        ui,
                        "Memory",
                        &format!(
                            "{} free of {}",
                            format_bytes(info.available_memory),
                            format_bytes(info.total_memory)
                        ),
                    );
                });
                ui.add_space(theme::SPACE_MD);
                ui.label(RichText::new("Drives").family(theme::bold()));
                ui.add_space(theme::SPACE_XS);
                card(ui, |ui| {
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
            });
        if copy {
            ui.ctx().copy_text(info.to_report());
        }
        if refresh {
            self.overview = Some(toolkit_platform::system_info());
        }
    }

    fn draw_sandbox(&mut self, ui: &mut egui::Ui) {
        ui.heading("Windows Sandbox");
        ui.add_space(theme::SPACE_MD);
        let available = self.sandbox_available;
        let mut open_features = false;
        if !available {
            ui.label(
                RichText::new(
                    "Windows Sandbox is not installed. Enable the 'Windows Sandbox' optional \
                     feature (Windows Pro or Enterprise) to use this.",
                )
                .color(theme::WARNING),
            );
            ui.add_space(theme::SPACE_XS);
            if ui
                .add_enabled(
                    !self.is_running(),
                    egui::Button::new("Open Windows Features"),
                )
                .clicked()
            {
                open_features = true;
            }
            ui.add_space(theme::SPACE_SM);
        }
        if open_features {
            self.start(launcher(
                "open-optionalfeatures",
                "Windows Features",
                "Opens the Windows optional features dialog.",
                "optionalfeatures".to_owned(),
                Vec::new(),
            ));
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
            Err(message) => self.show_error("Windows Sandbox", message),
        }
    }

    fn draw_startup(&mut self, ui: &mut egui::Ui) {
        let entries = &*self.startup.get_or_insert_with(load_startup);
        let error = self.startup_error.as_deref();
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
                if let Some(error) = error {
                    ui.label(
                        RichText::new(format!("Could not change the entry: {error}"))
                            .color(theme::DANGER),
                    );
                }
                ui.add_space(theme::SPACE_MD);
                if entries.is_empty() {
                    ui.label(RichText::new("No startup entries found.").color(theme::MUTED));
                }
                for entry in entries {
                    if let Some(change) = startup_card(ui, entry) {
                        toggle = Some((entry.scope, entry.name.clone(), change));
                    }
                    ui.add_space(theme::SPACE_SM);
                }
            });
        if refresh {
            self.startup = Some(load_startup());
            self.startup_error = None;
        }
        if let Some((scope, name, enabled)) = toggle {
            match toolkit_platform::set_startup_enabled(scope, &name, enabled) {
                Ok(()) => {
                    self.startup = Some(load_startup());
                    self.startup_error = None;
                }
                Err(message) => {
                    tracing::warn!("Startup: {message}");
                    self.startup_error = Some(message);
                }
            }
        }
    }

    /// The bottom panel showing the current or most recent operation's status and
    /// captured output. Shared across sections so output never depends on which
    /// section is visible.
    fn draw_run_panel(&mut self, root: &mut egui::Ui) {
        if self.run.view().is_none() {
            return;
        }
        let mut dismiss = false;
        egui::Panel::bottom("run-output")
            .resizable(true)
            .default_size(260.0)
            .min_size(96.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(root, |ui| {
                let Some(run) = self.run.view() else {
                    return;
                };
                let running = matches!(run.status, RunStatus::Running);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(run.label).family(theme::bold()));
                    let (text, color) = status_text(&run.status);
                    ui.label(RichText::new(text).color(color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!running, egui::Button::new("Dismiss"))
                            .clicked()
                        {
                            dismiss = true;
                        }
                    });
                });
                if run.is_capture {
                    ui.add_space(theme::SPACE_SM);
                    egui::ScrollArea::vertical()
                        .id_salt("run-output-scroll")
                        .auto_shrink([false, false])
                        .stick_to_bottom(running)
                        .show(ui, |ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            for line in &run.lines {
                                ui.monospace(line);
                            }
                        });
                }
            });
        if dismiss {
            self.run.dismiss();
        }
    }

    fn draw_about(&mut self, ui: &mut egui::Ui) {
        ui.heading(toolkit_core::APP_TITLE);
        ui.add_space(theme::SPACE_SM);
        ui.label(RichText::new(concat!("Version ", env!("CARGO_PKG_VERSION"))).color(theme::MUTED));
        ui.add_space(theme::SPACE_SM);
        ui.hyperlink("https://github.com/handsomefox/win-toolkit");
        ui.add_space(theme::SPACE_SM);
        if ui
            .button("Check for updates")
            .on_hover_text("Check GitHub for a new version now, and choose how often to check")
            .clicked()
        {
            // The label promises a check, so start one as the window opens.
            // The updater leaves a running check or download alone.
            self.updater.check_now();
            self.update_open = true;
        }
        ui.add_space(theme::SPACE_MD);
        let running = self.is_running();
        let mut open_logs = None;
        if let Some(dir) = &self.logs_dir {
            ui.label(RichText::new("Diagnostics log").family(theme::bold()));
            ui.label(RichText::new(dir.display().to_string()).color(theme::MUTED));
            ui.add_space(theme::SPACE_XS);
            if ui
                .add_enabled(!running, egui::Button::new("Open logs folder"))
                .clicked()
            {
                open_logs = Some(open_logs_folder(&dir.to_string_lossy()));
            }
        }
        if let Some(operation) = open_logs {
            self.start(operation);
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
        let running = self.is_running();
        let Some(operation) = &self.confirm else {
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
                    if ui.add_enabled(!running, egui::Button::new("Run")).clicked() {
                        run = true;
                    }
                });
            });
        if run {
            if let Some(operation) = self.confirm.take() {
                self.start(operation);
            }
        } else if cancel {
            self.confirm = None;
        }
    }
}

/// Draws one startup entry as a card. Returns the requested new enabled state if
/// the user toggled it.
fn startup_card(ui: &mut egui::Ui, entry: &StartupEntry) -> Option<bool> {
    let mut toggled = None;
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&entry.name).family(theme::bold()));
                ui.label(RichText::new(&entry.command).color(theme::MUTED));
                ui.label(RichText::new(entry.scope.label()).color(theme::MUTED));
                if !entry.scope.can_toggle() {
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
                        entry.scope.can_toggle(),
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

/// Lists startup entries with this user's first, each group by name.
fn load_startup() -> Vec<StartupEntry> {
    let mut entries = toolkit_platform::list_startup();
    entries.sort_by(|a, b| {
        let scope_rank = |scope| u8::from(scope != StartupScope::CurrentUser);
        scope_rank(a.scope)
            .cmp(&scope_rank(b.scope))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// Whether the Windows Sandbox optional feature appears to be installed.
fn sandbox_available() -> bool {
    let system_root = toolkit_platform::system_root();
    Path::new(&format!(r"{system_root}\System32\WindowsSandbox.exe")).exists()
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

/// Wraps content in a full-width surface card.
fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(theme::RADIUS_MD)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
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

/// Draws a titled section of operation cards. Returns the operation whose
/// action was clicked.
fn operation_section(
    ui: &mut egui::Ui,
    title: &str,
    ops: &[Operation],
    running: bool,
) -> Option<Operation> {
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading(title);
            ui.add_space(theme::SPACE_MD);
            clicked = operation_cards(ui, ops, running);
        });
    clicked
}

/// Draws the Health & Repair section, one labelled group of cards at a time.
/// Returns the operation whose action was clicked.
fn health_section(
    ui: &mut egui::Ui,
    groups: &[(&'static str, Vec<Operation>)],
    running: bool,
) -> Option<Operation> {
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading("Health & Repair");
            ui.add_space(theme::SPACE_MD);
            for (group, ops) in groups {
                ui.label(
                    RichText::new(*group)
                        .family(theme::bold())
                        .color(theme::MUTED),
                );
                ui.add_space(theme::SPACE_XS);
                if let Some(operation) = operation_cards(ui, ops, running) {
                    clicked = Some(operation);
                }
                ui.add_space(theme::SPACE_SM);
            }
        });
    clicked
}

/// Draws a card per operation. Returns the operation whose action was clicked.
fn operation_cards(ui: &mut egui::Ui, ops: &[Operation], running: bool) -> Option<Operation> {
    let mut clicked = None;
    for operation in ops {
        if operation_card(ui, operation, running) {
            clicked = Some(operation.clone());
        }
        ui.add_space(theme::SPACE_SM);
    }
    clicked
}

/// Draws one operation as a card and returns whether its action was clicked.
fn operation_card(ui: &mut egui::Ui, operation: &Operation, running: bool) -> bool {
    let mut clicked = false;
    card(ui, |ui| {
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

fn status_text(status: &RunStatus) -> (String, egui::Color32) {
    match status {
        RunStatus::Running => ("Running…".to_owned(), theme::WARNING),
        RunStatus::Finished(Outcome::Success) => {
            ("Completed successfully.".to_owned(), theme::SUCCESS)
        }
        RunStatus::Finished(Outcome::Failed(code)) => {
            (format!("Exited with code {code}."), theme::DANGER)
        }
        RunStatus::Finished(Outcome::Declined) => (
            "Elevation was declined; nothing was run.".to_owned(),
            theme::MUTED,
        ),
        RunStatus::Finished(Outcome::Error(message)) => {
            (format!("Could not run: {message}"), theme::DANGER)
        }
    }
}

/// Builds the Diagnostics catalog, creating the folder the battery report is
/// written to.
fn build_diagnostics(logs_dir: Option<&Path>) -> Vec<Operation> {
    let report_file = toolkit_platform::app_data_dir().map(|dir| {
        let reports = dir.join("reports");
        if let Err(error) = std::fs::create_dir_all(&reports) {
            tracing::warn!(%error, "could not create {}", reports.display());
        }
        reports
            .join("battery-report.html")
            .to_string_lossy()
            .into_owned()
    });
    let logs_dir = logs_dir.map(|dir| dir.to_string_lossy().into_owned());
    diagnostics_operations(
        report_file.as_deref(),
        &toolkit_platform::system_root(),
        logs_dir.as_deref(),
    )
}

impl eframe::App for ToolkitApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.run.drain(&self.worker);
        self.updater.poll(&mut self.update_settings);
        self.draw_header(root);
        self.draw_sidebar(root);
        self.draw_run_panel(root);
        self.draw_central(root);
        self.draw_confirm(&ctx);
        if self.update_open {
            let busy = self.is_running();
            crate::update::window(
                &ctx,
                &mut self.update_open,
                &mut self.updater,
                &mut self.update_settings,
                busy,
            );
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, UPDATE_SETTINGS_KEY, &self.update_settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(label: &'static str) -> Operation {
        launcher(
            "test",
            label,
            "Opens a test target.",
            "x".to_owned(),
            Vec::new(),
        )
    }

    #[test]
    fn a_running_operation_keeps_the_panel_until_it_finishes() {
        let mut panel = RunPanel::default();
        assert!(panel.show(RunView::running(&operation("first"))));
        assert!(panel.is_running());

        assert!(!panel.show(RunView::running(&operation("second"))));
        assert!(!panel.show(RunView::failed("Startup", "denied".to_owned())));
        panel.dismiss();
        assert_eq!(panel.view().map(|run| run.label), Some("first"));

        panel.apply(Event::Output {
            lines: vec!["line".to_owned()],
        });
        panel.apply(Event::Finished(Outcome::Success));
        assert!(!panel.is_running());
        assert_eq!(panel.view().unwrap().lines, ["line"]);

        assert!(panel.show(RunView::running(&operation("second"))));
        assert_eq!(panel.view().map(|run| run.label), Some("second"));
    }

    #[test]
    fn a_stopped_worker_fails_the_run_it_left_behind() {
        let worker = Worker::stopped();
        let mut panel = RunPanel::default();
        assert!(panel.show(RunView::running(&operation("first"))));
        panel.drain(&worker);
        assert!(!panel.is_running());
        assert!(matches!(
            panel.view().unwrap().status,
            RunStatus::Finished(Outcome::Error(_))
        ));
        assert!(!worker.send(Command::Run(operation("second"))));
    }

    #[test]
    fn a_finished_run_can_be_dismissed_and_takes_no_more_events() {
        let mut panel = RunPanel::default();
        assert!(panel.show(RunView::failed("Startup", "denied".to_owned())));
        assert!(!panel.is_running());
        panel.apply(Event::Finished(Outcome::Success));
        assert!(matches!(
            panel.view().unwrap().status,
            RunStatus::Finished(Outcome::Error(_))
        ));
        panel.dismiss();
        assert!(panel.view().is_none());
    }
}
