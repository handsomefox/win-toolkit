//! win-toolkit: a Windows desktop app for system inspection, diagnostics, and
//! documented maintenance. This binary hosts the egui interface; the Windows
//! integration lives in `toolkit-platform` and the portable domain types in
//! `toolkit-core`.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod diagnostics;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let log_path = diagnostics::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "win-toolkit starting");
    if let Some(path) = &log_path {
        tracing::info!("logging to {}", path.display());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(toolkit_core::APP_TITLE)
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        toolkit_core::APP_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(app::ToolkitApp::new(cc)))),
    )
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
