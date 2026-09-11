use agentifi_domain::{AgentSession, SessionStatus};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke};
use eframe::epaint::Shadow;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
use uuid::Uuid;

const LOCAL_ENDPOINT: &str = "http://127.0.0.1:8787";
const GLASS: Color32 = Color32::from_rgba_premultiplied(27, 34, 55, 225);
const ACCENT: Color32 = Color32::from_rgb(103, 211, 255);

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
    events: Option<Receiver<()>>,
    live: bool,
}
impl AgentifiApp {
    fn new() -> Self {
        let mut app = Self {
            endpoint: LOCAL_ENDPOINT.into(),
            status: "Connecting…".into(),
            sessions: Vec::new(),
            selected_session: None,
            local_server: None,
            events: None,
            live: false,
        };
        app.connect_local_or_start_server();
        app
    }
    fn connect_local_or_start_server(&mut self) {
        if self.fetch_sessions(LOCAL_ENDPOINT).is_err() {
            if let Ok(child) = spawn_local_server() {
                self.local_server = Some(child);
                for _ in 0..30 {
                    if self.fetch_sessions(LOCAL_ENDPOINT).is_ok() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        self.start_events();
    }
    fn start_events(&mut self) {
        let endpoint = self.endpoint.clone();
        let (tx, rx) = mpsc::channel();
        self.events = Some(rx);
        self.live = true;
        thread::spawn(move || {
            let Ok(response) = reqwest::blocking::Client::new()
                .get(format!("{endpoint}/api/v1/events"))
                .header("Accept", "text/event-stream")
                .send()
            else {
                return;
            };
            for line in BufReader::new(response).lines().map_while(Result::ok) {
                if line.starts_with("event:") || line.starts_with("data:") {
                    let _ = tx.send(());
                }
            }
        });
    }
    fn fetch_sessions(&mut self, endpoint: &str) -> Result<(), String> {
        let endpoint = endpoint.trim_end_matches('/').to_owned();
        let response = reqwest::blocking::get(format!("{endpoint}/api/v1/sessions"))
            .map_err(|e| e.to_string())?;
        let sessions = response
            .json::<Vec<AgentSession>>()
            .map_err(|e| e.to_string())?;
        self.endpoint = endpoint;
        self.status = format!("{} sessions · live SSE", sessions.len());
        self.sessions = sessions;
        self.selected_session = self
            .selected_session
            .filter(|id| self.sessions.iter().any(|s| s.id == *id));
        Ok(())
    }
    fn refresh(&mut self) {
        let endpoint = self.endpoint.clone();
        let _ = self.fetch_sessions(&endpoint);
    }
    fn attach(&mut self, id: Uuid) {
        let body = serde_json::json!({"jsonrpc":"2.0","id":"desktop-attach","method":"sessions.attach","params":{"session_id":id}});
        match reqwest::blocking::Client::new()
            .post(format!("{}/api/v1/rpc", self.endpoint))
            .json(&body)
            .send()
        {
            Ok(_) => self.status = "Session attached · Pi control ready".into(),
            Err(e) => self.status = format!("Attach failed: {e}"),
        }
    }
    fn selected(&self) -> Option<&AgentSession> {
        self.selected_session
            .and_then(|id| self.sessions.iter().find(|s| s.id == id))
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals {
            dark_mode: true,
            panel_fill: Color32::from_rgb(10, 13, 25),
            window_fill: GLASS,
            faint_bg_color: Color32::from_rgb(16, 21, 38),
            extreme_bg_color: Color32::from_rgb(7, 9, 18),
            selection: egui::style::Selection {
                bg_fill: Color32::from_rgba_premultiplied(39, 133, 186, 150),
                stroke: Stroke::new(1.0_f32, ACCENT),
            },
            ..egui::Visuals::dark()
        });
        if let Some(rx) = &self.events {
            if rx.try_iter().next().is_some() {
                self.refresh();
            }
        }
        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgb(9, 12, 24))
                    .inner_margin(Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(
                        egui::RichText::new("AGENTIFI")
                            .color(Color32::WHITE)
                            .size(24.0),
                    );
                    ui.label(egui::RichText::new("CONTROL PLANE").color(ACCENT).small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text = if self.live { "● LIVE" } else { "○ OFFLINE" };
                        ui.colored_label(
                            if self.live {
                                Color32::from_rgb(104, 234, 181)
                            } else {
                                Color32::RED
                            },
                            text,
                        );
                        if ui.button("↻ Refresh").clicked() {
                            self.refresh();
                        }
                    });
                });
                ui.add_space(16.0);
                ui.columns(3, |columns| {
                    glass_panel(&mut columns[0], "MACHINES", |ui| {
                        ui.colored_label(Color32::from_rgb(104, 234, 181), "●  THIS MACHINE");
                        ui.small("Agentifi local server");
                        ui.add_space(14.0);
                        ui.label("Endpoint");
                        ui.text_edit_singleline(&mut self.endpoint);
                        if ui.button("Connect").clicked() {
                            self.refresh();
                            self.start_events();
                        }
                    });
                    glass_panel(
                        &mut columns[1],
                        &format!("SESSIONS  ·  {}", self.sessions.len()),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Pi workspace");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.small("LIVE SSE");
                                    },
                                );
                            });
                            ui.add_space(8.0);
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for session in self.sessions.clone() {
                                    let selected = self.selected_session == Some(session.id);
                                    let frame = Frame::NONE
                                        .fill(if selected {
                                            Color32::from_rgba_premultiplied(28, 105, 145, 150)
                                        } else {
                                            Color32::from_rgba_premultiplied(255, 255, 255, 8)
                                        })
                                        .stroke(Stroke::new(
                                            1.0_f32,
                                            if selected {
                                                ACCENT
                                            } else {
                                                Color32::from_rgba_premultiplied(255, 255, 255, 18)
                                            },
                                        ))
                                        .corner_radius(CornerRadius::same(12))
                                        .inner_margin(Margin::same(12));
                                    frame.show(ui, |ui| {
                                        if ui
                                            .selectable_label(
                                                selected,
                                                egui::RichText::new(&session.title).strong(),
                                            )
                                            .clicked()
                                        {
                                            self.selected_session = Some(session.id);
                                            self.attach(session.id);
                                        }
                                        ui.small(format!(
                                            "{}  ·  {}",
                                            session.project,
                                            status_label(&session.status)
                                        ));
                                    });
                                    ui.add_space(7.0);
                                }
                            });
                        },
                    );
                    glass_panel(&mut columns[2], "SESSION DETAIL", |ui| {
                        if let Some(session) = self.selected() {
                            ui.heading(&session.title);
                            ui.colored_label(ACCENT, &session.project);
                            ui.separator();
                            ui.label(format!("Status   {}", status_label(&session.status)));
                            ui.label(format!("ID       {}", session.id));
                            if let Some(path) = &session.source_path {
                                ui.small(path);
                            }
                            ui.add_space(16.0);
                            ui.label(egui::RichText::new("PI ACTIVITY").color(ACCENT).strong());
                            ui.label("Attach a session to stream Pi events here.");
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                let _ = ui.button("Prompt");
                                let _ = ui.button("Steer");
                                let _ = ui.button("Abort");
                            });
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label("Select a Pi session");
                            });
                        }
                    });
                });
                ui.add_space(10.0);
                ui.label(egui::RichText::new(&self.status).color(Color32::from_rgb(157, 170, 198)));
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
fn glass_panel(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    Frame::NONE
        .fill(GLASS)
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_premultiplied(170, 210, 255, 35),
        ))
        .corner_radius(CornerRadius::same(18))
        .shadow(Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(90),
        })
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .color(Color32::from_rgb(157, 170, 198))
                    .strong(),
            );
            ui.add_space(10.0);
            content(ui);
        });
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
                .and_then(|p| p.parent().map(|p| p.join("agentifi-server")))
        })
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.join("target/debug/agentifi-server"))
        })
        .ok_or("server executable not found")?;
    Command::new(&command)
        .env("AGENTIFI_BIND", "127.0.0.1:8787")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())
}
