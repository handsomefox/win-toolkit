//! The eframe application shell: the section sidebar, per-section views, and the
//! confirm-then-run flow that drives elevated operations through the worker.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use toolkit_core::{Operation, sfc_scannow};

use crate::theme;
use crate::worker::{self, Command, Event, Outcome, Worker};

/// The sidebar sections. Only some are implemented in the current version; the
/// rest render a placeholder so the navigation frame is in place.
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
            log_path,
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

    fn start(&mut self, operation: Operation) {
        self.run = Some(RunView {
            label: operation.label,
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
                Section::Health => self.draw_health(ui),
                Section::About => self.draw_about(ui),
                other => Self::draw_placeholder(ui, other),
            });
    }

    fn draw_placeholder(ui: &mut egui::Ui, section: Section) {
        ui.heading(section.label());
        ui.add_space(theme::SPACE_MD);
        ui.label(RichText::new("Coming in a later version.").color(theme::MUTED));
    }

    fn draw_health(&mut self, ui: &mut egui::Ui) {
        ui.heading("Health & Repair");
        ui.add_space(theme::SPACE_MD);
        self.draw_operation_card(ui, &sfc_scannow());
        if self.run.is_some() {
            ui.add_space(theme::SPACE_MD);
            ui.separator();
            ui.add_space(theme::SPACE_SM);
            self.draw_run_output(ui);
        }
    }

    fn draw_operation_card(&mut self, ui: &mut egui::Ui, operation: &Operation) {
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
                ui.add_space(theme::SPACE_XS);
                ui.label(RichText::new(operation.duration_hint).color(theme::MUTED));
                if !operation.cancelable {
                    ui.label(
                        RichText::new("Cannot be cancelled once started.").color(theme::WARNING),
                    );
                }
                ui.add_space(theme::SPACE_SM);
                let running = self.is_running();
                if ui.add_enabled(!running, egui::Button::new("Run")).clicked() {
                    self.confirm = Some(operation.clone());
                }
            });
    }

    fn draw_run_output(&mut self, ui: &mut egui::Ui) {
        let Some(run) = &self.run else {
            return;
        };
        ui.label(RichText::new(run.label).family(theme::bold()));
        ui.add_space(theme::SPACE_XS);
        let (text, color) = status_text(&run.status);
        ui.label(RichText::new(text).color(color));
        ui.add_space(theme::SPACE_SM);
        let running = matches!(run.status, RunStatus::Running);
        egui::ScrollArea::vertical()
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

    fn draw_about(&mut self, ui: &mut egui::Ui) {
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
                ui.set_max_width(420.0);
                ui.label(RichText::new(operation.label).family(theme::bold()));
                ui.add_space(theme::SPACE_SM);
                ui.label(operation.description);
                ui.add_space(theme::SPACE_SM);
                ui.label(RichText::new(operation.duration_hint).color(theme::MUTED));
                if operation.elevation == toolkit_core::Elevation::Administrator {
                    ui.label(
                        RichText::new("This will request Administrator rights (a UAC prompt).")
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
