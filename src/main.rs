mod api;
mod app;
mod config;
mod credentials;

use app::CabinetApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cabinet")
            .with_inner_size([1220.0, 790.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Cabinet",
        options,
        Box::new(|cc| Ok(Box::new(CabinetApp::new(cc)))),
    )
}
