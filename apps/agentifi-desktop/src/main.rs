use agentifi_domain::AgentSession;
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Agentifi",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(AgentifiApp::default()))),
    )
}

#[derive(Default)]
struct AgentifiApp {
    endpoint: String,
    status: String,
    sessions: Vec<AgentSession>,
}

impl AgentifiApp {
    fn connect(&mut self) {
        let endpoint = if self.endpoint.trim().is_empty() {
            "http://127.0.0.1:8787".to_owned()
        } else {
            self.endpoint.trim_end_matches('/').to_owned()
        };
        self.endpoint = endpoint.clone();
        match reqwest::blocking::get(format!("{endpoint}/api/v1/sessions")) {
            Ok(response) => match response.json::<Vec<AgentSession>>() {
                Ok(sessions) => {
                    self.status = format!("Connected; {} session(s)", sessions.len());
                    self.sessions = sessions;
                }
                Err(error) => self.status = format!("Invalid server response: {error}"),
            },
            Err(error) => self.status = format!("Connection failed: {error}"),
        }
    }
}

impl eframe::App for AgentifiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("Agentifi");
            ui.label("Control agent sessions through a local or remote Agentifi server.");
            ui.horizontal(|ui| {
                ui.label("Server:");
                ui.text_edit_singleline(&mut self.endpoint);
                if ui.button("Connect").clicked() {
                    self.connect();
                }
            });
            ui.separator();
            ui.label(&self.status);
            for session in &self.sessions {
                ui.group(|ui| {
                    ui.label(format!("{} — {}", session.title, session.project));
                    ui.label(format!("Status: {:?}", session.status));
                });
            }
        });
    }
}
