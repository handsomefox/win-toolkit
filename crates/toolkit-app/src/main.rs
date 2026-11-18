//! win-toolkit: a Windows desktop app for system inspection, diagnostics, and
//! documented maintenance. This binary hosts the egui interface; the Windows
//! integration lives in `toolkit-platform` and the portable domain types in
//! `toolkit-core`.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod theme;
mod update;
mod worker;

use eframe::egui;

fn main() -> eframe::Result {
    let logs_dir = app_diagnostics::init(
        toolkit_platform::logs_dir().as_deref(),
        toolkit_core::APP_SLUG,
        "info",
    );
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "win-toolkit starting");
    if let Some(dir) = &logs_dir {
        tracing::info!("logging to {}", dir.display());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(toolkit_core::APP_TITLE)
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    // The guard lives only for the window, so an updated copy started below
    // finds the lock free.
    let result = {
        // A second launch brings this window forward instead of opening another.
        let instance = match app_instance::acquire(APP_INFO.bin) {
            Ok(app_instance::Instance::Primary(guard)) => Some(guard),
            Ok(app_instance::Instance::Secondary) => {
                tracing::info!("another copy is running; asked it to come forward");
                return Ok(());
            }
            // Run without the guard rather than refuse to start.
            Err(error) => {
                tracing::warn!(%error, "running without the single-instance lock");
                None
            }
        };

        eframe::run_native(
            toolkit_core::APP_TITLE,
            options,
            Box::new(|cc| {
                if let Some(guard) = &instance {
                    bring_forward_on_activate(guard, &cc.egui_ctx);
                }
                Ok(Box::new(app::ToolkitApp::new(cc, logs_dir)))
            }),
        )
    };
    // eframe saves the app's state before `run_native` returns, so the
    // updated copy starts from everything this one knew.
    if let Err(error) = app_update::restart_if_requested() {
        tracing::error!(%error, "could not start the updated app");
    }
    result
}

/// Where this app's releases are published.
const APP_INFO: app_update::AppInfo = app_update::AppInfo {
    name: toolkit_core::APP_TITLE,
    owner: "handsomefox",
    repo: "win-toolkit",
    bin: "win-toolkit",
    version: env!("CARGO_PKG_VERSION"),
};

/// Restores and focuses the window whenever another launch signals.
fn bring_forward_on_activate(guard: &app_instance::Guard, ctx: &egui::Context) {
    let ctx = ctx.clone();
    let watching = guard.on_activate(move || {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    });
    if let Err(error) = watching {
        tracing::warn!(%error, "a second launch will not bring this window forward");
    }
}

fn app_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon.png");
    match image::load_from_memory(bytes) {
        Ok(decoded) => {
            let rgba = decoded.into_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(err) => {
            tracing::warn!("failed to decode window icon: {err}");
            egui::IconData::default()
        }
    }
}
