use agentifi_domain::AgentSession;
use eframe::egui;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

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
    local_server: Option<Child>,
}

impl AgentifiApp {
    fn new() -> Self {
        let mut app = Self {
            endpoint: LOCAL_ENDPOINT.to_owned(),
            status: "Checking local Agentifi server…".to_owned(),
            sessions: Vec::new(),
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
}

impl Drop for AgentifiApp {
    fn drop(&mut self) {
        if let Some(mut server) = self.local_server.take() {
            let _ = server.kill();
            let _ = server.wait();
        }
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
