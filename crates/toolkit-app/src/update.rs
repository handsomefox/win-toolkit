//! Updates to win-toolkit itself: the header chip that points out a new
//! version, and the window around the shared "App updates" dialog from
//! `app_update_ui`, drawn in this app's theme.

use app_update::{Settings, Updater};
use eframe::egui::{self, Color32, RichText, Visuals};

use crate::theme;

/// The app's theme, as the shared update dialog asks for it.
struct Look;

impl app_update_ui::Look for Look {
    fn muted(&self, _visuals: &Visuals) -> Color32 {
        theme::MUTED
    }

    fn success(&self, _visuals: &Visuals) -> Color32 {
        theme::SUCCESS
    }

    fn warning(&self, _visuals: &Visuals) -> Color32 {
        theme::WARNING
    }

    fn space_xs(&self) -> f32 {
        theme::SPACE_XS
    }

    fn space_sm(&self) -> f32 {
        theme::SPACE_SM
    }

    fn space_md(&self) -> f32 {
        theme::SPACE_MD
    }

    fn heading(&self, text: RichText) -> RichText {
        text.family(theme::bold()).size(theme::FONT_HEADING)
    }

    fn bold(&self, text: RichText) -> RichText {
        text.family(theme::bold())
    }

    fn fine_print(&self, text: RichText) -> RichText {
        text.size(theme::FONT_SMALL)
    }
}

/// A chip at the right of the header while a new version is out. Returns
/// whether it was clicked.
pub(crate) fn chip(ui: &mut egui::Ui, updater: &Updater, settings: &Settings) -> bool {
    let Some(text) = app_update_ui::chip_text(updater, settings) else {
        return false;
    };
    let chip = egui::Button::new(RichText::new(text).color(theme::ACCENT))
        .fill(theme::ACCENT.gamma_multiply(0.16))
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(theme::RADIUS_MD));
    ui.add(chip)
        .on_hover_text(app_update_ui::chip_hover_text(updater))
        .clicked()
}

/// The app updates window. `busy` is true while an operation runs; the
/// restart waits for it, because closing the app must never cut one short.
pub(crate) fn window(
    ctx: &egui::Context,
    open: &mut bool,
    updater: &mut Updater,
    settings: &mut Settings,
    busy: bool,
) {
    let mut chosen = None;
    egui::Window::new(app_update_ui::TITLE)
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_width(520.0);
            chosen = app_update_ui::body(ui, &Look, updater, settings);
            let busy = busy.then_some("Wait for the running operation to finish");
            chosen = app_update_ui::actions(ui, &Look, updater, busy).or(chosen);
        });
    if let Some(action) = chosen
        && app_update_ui::apply(action, ctx, updater, settings)
    {
        *open = false;
    }
}
