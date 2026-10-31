//! Updates to win-toolkit itself: the header chip that points out a new
//! version, and the window with its release notes, the install, and how often
//! to check.

use app_update::{CheckPeriod, NoteBlock, Settings, Update, Updater};
use eframe::egui::{self, RichText};

use crate::{APP_INFO, theme};

enum Action {
    Download,
    Restart,
    Skip,
    Later,
}

/// A chip at the right of the header while a new version is out. Returns
/// whether it was clicked.
pub(crate) fn chip(ui: &mut egui::Ui, updater: &Updater, settings: &Settings) -> bool {
    let Some(release) = updater.notification(settings) else {
        return false;
    };
    let text = if updater.staged().is_some() {
        format!("Restart for {}", release.version)
    } else {
        format!("Update {} available", release.version)
    };
    let chip = egui::Button::new(RichText::new(text).color(theme::ACCENT))
        .fill(theme::ACCENT.gamma_multiply(0.16))
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(theme::RADIUS_MD));
    ui.add(chip)
        .on_hover_text("A new version of win-toolkit is out. Click to see what changed.")
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
    let mut period = settings.period;
    let mut check = false;
    let mut action = None;
    egui::Window::new("App updates")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_width(520.0);
            body(ui, updater, settings, &mut period, &mut check);
            action = actions(ui, updater, busy);
        });
    settings.period = period;
    if check {
        updater.check_now();
    }
    match action {
        Some(Action::Download) => updater.download(),
        Some(Action::Restart) => {
            if updater.install_and_restart().is_ok() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        Some(Action::Skip) => {
            updater.skip(settings);
            *open = false;
        }
        Some(Action::Later) => {
            updater.dismiss();
            *open = false;
        }
        None => {}
    }
}

fn body(
    ui: &mut egui::Ui,
    updater: &Updater,
    settings: &Settings,
    period: &mut CheckPeriod,
    check: &mut bool,
) {
    let current = updater.current().map_or_else(
        || APP_INFO.version.to_owned(),
        |version| version.to_string(),
    );
    if let Some(update) = updater.update() {
        available_update(ui, updater, settings, update, &current);
    } else if updater.is_checking() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Checking GitHub for a new version…");
        });
    } else if updater.is_up_to_date() {
        ui.label(
            RichText::new(format!("Version {current} is the latest version."))
                .color(theme::SUCCESS),
        );
    } else {
        ui.label(RichText::new(format!("You have version {current}.")).color(theme::MUTED));
    }
    if let Some(error) = updater.error() {
        ui.add_space(theme::SPACE_SM);
        ui.label(RichText::new(error).color(theme::WARNING));
    }

    ui.add_space(theme::SPACE_MD);
    ui.separator();
    ui.label(RichText::new("Checking").family(theme::bold()));
    ui.add_space(theme::SPACE_XS);
    ui.horizontal(|ui| {
        ui.label("Check for new versions");
        egui::ComboBox::from_id_salt("app-update-period")
            .selected_text(period.label())
            .show_ui(ui, |ui| {
                for option in CheckPeriod::ALL {
                    ui.selectable_value(period, option, option.label());
                }
            });
        *check = ui
            .add_enabled(!updater.is_busy(), egui::Button::new("Check now"))
            .clicked();
    });
    let last = settings.last_check.map_or_else(
        || "Never checked".to_owned(),
        |then| {
            format!(
                "Last checked {}",
                app_update::describe_age(then, app_update::now())
            )
        },
    );
    ui.label(
        RichText::new(format!(
            "{last}. A check asks GitHub for this app's releases and sends nothing else."
        ))
        .color(theme::MUTED)
        .size(theme::FONT_SMALL),
    );
}

/// The newest version, where the install stands, and the notes of every
/// release since the running one.
fn available_update(
    ui: &mut egui::Ui,
    updater: &Updater,
    settings: &Settings,
    update: &Update,
    current: &str,
) {
    let latest = update.latest();
    ui.label(
        RichText::new(format!("Version {} is available", latest.version))
            .family(theme::bold())
            .size(theme::FONT_HEADING),
    );
    let released = if latest.date.is_empty() {
        String::new()
    } else {
        format!(" It was released on {}.", latest.date)
    };
    ui.label(RichText::new(format!("You have version {current}.{released}")).color(theme::MUTED));
    if updater.staged() == Some(latest.version) {
        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new("Downloaded and verified. Restart win-toolkit to finish.")
                .color(theme::SUCCESS),
        );
    } else if settings.skipped.as_deref() == Some(latest.version.to_string().as_str()) {
        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new("You skipped this version, so the header does not point it out.")
                .color(theme::MUTED),
        );
    }
    if !updater.can_install() {
        ui.add_space(theme::SPACE_XS);
        ui.label(
            RichText::new(
                "This release has no build that win-toolkit can install by itself. \
                 Download it from the release page instead.",
            )
            .color(theme::MUTED),
        );
        ui.hyperlink_to("Release page", &latest.page_url);
    }
    ui.add_space(theme::SPACE_MD);
    ui.label(RichText::new("What's new").family(theme::bold()));
    ui.add_space(theme::SPACE_XS);
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for release in update.releases() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(release.version.to_string()).family(theme::bold()));
                    if !release.date.is_empty() {
                        ui.label(RichText::new(&release.date).color(theme::MUTED));
                    }
                });
                release_notes(ui, &release.notes);
                ui.add_space(theme::SPACE_SM);
            }
            ui.hyperlink_to("All releases", APP_INFO.releases_page());
        });
}

fn release_notes(ui: &mut egui::Ui, markdown: &str) {
    let blocks = app_update::parse_notes(markdown);
    if blocks.is_empty() {
        ui.label(RichText::new("No notes for this release.").color(theme::MUTED));
    }
    for block in blocks {
        match block {
            NoteBlock::Heading(text) => {
                ui.label(RichText::new(text).family(theme::bold()));
            }
            NoteBlock::Bullet(text) => {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new("•").color(theme::MUTED));
                    ui.add(egui::Label::new(text).wrap());
                });
            }
            NoteBlock::Paragraph(text) => {
                ui.add(egui::Label::new(text).wrap());
            }
        }
    }
}

fn actions(ui: &mut egui::Ui, updater: &Updater, busy: bool) -> Option<Action> {
    let update = updater.update()?;
    ui.add_space(theme::SPACE_MD);
    if let Some((received, total)) = updater.progress() {
        let percent = total
            .filter(|total| *total > 0)
            .map_or(0, |total| received.saturating_mul(100) / total)
            .min(100);
        let fraction = f32::from(u8::try_from(percent).unwrap_or(100)) / 100.0;
        ui.add(egui::ProgressBar::new(fraction).text(format!(
            "Downloading {}… {percent}%",
            update.latest().version
        )));
        return None;
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if updater.staged().is_some() {
            if ui
                .add_enabled(!busy, egui::Button::new("Restart to update"))
                .on_hover_text("Close win-toolkit and open the new version")
                .on_disabled_hover_text("Wait for the running operation to finish")
                .clicked()
            {
                action = Some(Action::Restart);
            }
        } else if updater.can_install()
            && ui
                .add_enabled(!updater.is_busy(), egui::Button::new("Download update"))
                .on_hover_text(
                    "Download the new version and check it against its published SHA-256",
                )
                .clicked()
        {
            action = Some(Action::Download);
        }
        if ui
            .button("Skip this version")
            .on_hover_text("Stop pointing out this version. A newer one shows up again.")
            .clicked()
        {
            action = Some(Action::Skip);
        }
        if ui
            .button("Not now")
            .on_hover_text("Hide the reminder until win-toolkit starts again")
            .clicked()
        {
            action = Some(Action::Later);
        }
    });
    action
}
