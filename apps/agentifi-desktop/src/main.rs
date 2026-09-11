use agentifi_domain::{AgentSession, SessionStatus};
use eframe::egui;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

const LOCAL_ENDPOINT: &str = "http://127.0.0.1:8787";

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Agentifi",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(AgentifiApp::new()))),
    )
}

struct AgentifiApp {
    endpoint: String,
    status: String,
    sessions: Vec<AgentSession>,
    selected_session: Option<Uuid>,
    local_server: Option<Child>,
}

impl AgentifiApp {
    fn new() -> Self {
        let mut app = Self {
            endpoint: LOCAL_ENDPOINT.to_owned(),
            status: "Checking local Agentifi server…".to_owned(),
            sessions: Vec::new(),
            selected_session: None,
            local_server: None,
        };
        app.connect_local_or_start_server();
        app
    }

    fn connect_local_or_start_server(&mut self) {
        if self.fetch_sessions(LOCAL_ENDPOINT).is_ok() {
            return;
        }

        match spawn_local_server() {
            Ok(child) => {
                self.local_server = Some(child);
                for _ in 0..30 {
                    if self.fetch_sessions(LOCAL_ENDPOINT).is_ok() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                self.status = "Started local server, but it did not become ready".to_owned();
            }
            Err(error) => {
                self.status = format!("Local server unavailable: {error}");
            }
        }
    }

    fn fetch_sessions(&mut self, endpoint: &str) -> Result<(), String> {
        let endpoint = endpoint.trim_end_matches('/').to_owned();
        match reqwest::blocking::get(format!("{endpoint}/api/v1/sessions")) {
            Ok(response) => match response.json::<Vec<AgentSession>>() {
                Ok(sessions) => {
                    self.endpoint = endpoint;
                    self.status = format!("Connected; {} session(s)", sessions.len());
                    self.sessions = sessions;
                    self.selected_session = self
                        .selected_session
                        .filter(|id| self.sessions.iter().any(|session| session.id == *id));
                    Ok(())
                }
                Err(error) => {
                    self.status = format!("Invalid server response: {error}");
                    Err(error.to_string())
                }
            },
            Err(error) => {
                self.status = format!("Connection failed: {error}");
                Err(error.to_string())
            }
        }
    }

    fn connect(&mut self) {
        let endpoint = if self.endpoint.trim().is_empty() {
            LOCAL_ENDPOINT.to_owned()
        } else {
            self.endpoint.trim_end_matches('/').to_owned()
        };
        if endpoint == LOCAL_ENDPOINT && self.fetch_sessions(&endpoint).is_err() {
            self.connect_local_or_start_server();
        } else {
            let _ = self.fetch_sessions(&endpoint);
        }
    }

    fn selected(&self) -> Option<&AgentSession> {
        self.selected_session
            .and_then(|id| self.sessions.iter().find(|session| session.id == id))
    }
}

impl Drop for AgentifiApp {
    fn drop(&mut self) {
        if let Some(mut server) = self.local_server.take() {
            let _ = server.kill();
            let _ = server.wait();
        }
    }
}

impl eframe::App for AgentifiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("topbar").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Agentifi");
                ui.separator();
                ui.label("Session control plane");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.connect();
                    }
                });
            });
        });

        egui::SidePanel::left("machines")
            .default_width(220.0)
            .show(context, |ui| {
                ui.heading("Machines");
                ui.separator();
                ui.selectable_value(
                    &mut self.endpoint,
                    LOCAL_ENDPOINT.to_owned(),
                    "This machine",
                );
                ui.small("Local Agentifi server");
                ui.add_space(12.0);
                ui.label("Server endpoint");
                ui.text_edit_singleline(&mut self.endpoint);
                if ui.button("Connect").clicked() {
                    self.connect();
                }
                ui.add_space(12.0);
                ui.separator();
                ui.label("Future: trusted remote machines, pairing, and capability scopes.");
            });

        egui::SidePanel::right("details")
            .default_width(270.0)
            .show(context, |ui| {
                ui.heading("Session details");
                ui.separator();
                if let Some(session) = self.selected() {
                    ui.label(egui::RichText::new(&session.title).strong());
                    ui.label(format!("Project: {}", session.project));
                    ui.label(format!("Status: {}", status_label(&session.status)));
                    ui.label(format!("ID: {}", session.id));
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(false, egui::Button::new("Resume")).clicked() {
                            self.status =
                                "Resume is not available in the current API slice".to_owned();
                        }
                        if ui.add_enabled(false, egui::Button::new("Stop")).clicked() {
                            self.status =
                                "Stop is not available in the current API slice".to_owned();
                        }
                    });
                    ui.small("Lifecycle actions will be wired through application use cases.");
                } else {
                    ui.label("Select a session to inspect it.");
                }
            });

        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("Sessions");
            ui.horizontal(|ui| {
                ui.label(format!("{} session(s)", self.sessions.len()));
                ui.separator();
                ui.label("Source: Agentifi server");
            });
            ui.separator();

            if self.sessions.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No sessions discovered yet.");
                });
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for session in self.sessions.clone() {
                        let selected = self.selected_session == Some(session.id);
                        let response = ui.selectable_label(
                            selected,
                            format!(
                                "{}  ·  {}  ·  {}",
                                session.title,
                                session.project,
                                status_label(&session.status)
                            ),
                        );
                        if response.clicked() {
                            self.selected_session = Some(session.id);
                        }
                    }
                });
            }
        });

        egui::TopBottomPanel::bottom("status").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small(&self.endpoint);
                });
            });
        });
    }
}

fn status_label(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Paused => "paused",
        SessionStatus::Completed => "completed",
    }
}

fn spawn_local_server() -> Result<Child, String> {
    let command = std::env::var_os("AGENTIFI_SERVER_COMMAND")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|p| p.join("agentifi-server")))
        })
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.join("target/debug/agentifi-server"))
        })
        .ok_or_else(|| "could not determine the local server executable".to_owned())?;

    if !command.is_file() {
        return Err(format!(
            "server executable not found at {}",
            command.display()
        ));
    }

    Command::new(&command)
        .env("AGENTIFI_BIND", "127.0.0.1:8787")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", command.display()))
}
