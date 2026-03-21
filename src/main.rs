#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod common;
mod config;
mod github;
mod helium;
mod logging;
mod paths;
mod powershell;
mod scheduler;
mod service;
mod state;
mod versioning;

use eframe::egui;
use service::AppService;

fn main() {
    let service = match AppService::new() {
        Ok(service) => service,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };

    if std::env::args().any(|arg| arg == "--background") {
        if let Err(error) = service.run_background_update() {
            service.record_error(&format!("Background update failed: {error}"));
        }
        return;
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([820.0, 620.0])
        .with_min_inner_size([540.0, 420.0]);

    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "Helium Browser Updater",
        native_options,
        Box::new(move |creation_context| {
            Ok(Box::new(app::HeliumUpdaterApp::new(
                creation_context,
                service.clone(),
            )))
        }),
    );
}

fn load_window_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../images.jpg");
    let image = image::load_from_memory(icon_bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();

    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}
