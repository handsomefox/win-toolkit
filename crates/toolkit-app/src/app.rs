//! The eframe application shell. Stage 1 hosts the window chrome (themed header
//! and a central welcome panel); the tool sections are added in later stages.

use eframe::egui::{self, RichText};

use crate::theme;

pub(crate) struct ToolkitApp {}

impl ToolkitApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        Self {}
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
            });
    }

    fn draw_central(root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(12))
            .show(root, |ui| {
                ui.add_space(theme::SPACE_MD);
                ui.label("System inspection, diagnostics, and documented maintenance.");
                ui.add_space(theme::SPACE_MD);
                ui.label(
                    RichText::new(concat!("Version ", env!("CARGO_PKG_VERSION")))
                        .color(theme::MUTED),
                );
            });
    }
}

impl eframe::App for ToolkitApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Self::draw_header(root);
        Self::draw_central(root);
    }
}
