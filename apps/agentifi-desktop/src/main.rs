mod app;
mod components;
mod state;
mod theme;
mod views;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Agentifi",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app::AgentifiApp::new()))),
    )
}
