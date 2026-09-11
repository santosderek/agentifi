//! Agentifi desktop client.

mod api;
mod app;
mod components;
mod events;
mod state;
mod theme;
mod views;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1080.0, 680.0])
            .with_title("Agentifi"),
        ..Default::default()
    };
    eframe::run_native(
        "Agentifi",
        options,
        Box::new(|_cc| Ok(Box::new(app::AgentifiApp::new()))),
    )
}
