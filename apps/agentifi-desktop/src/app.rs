use crate::{
    components::{metric, nav_button, session_row, status, Icon},
    state::{AppView, UiState},
    theme, views,
};
use agentifi_domain::{AgentSession, SessionStatus};
use eframe::egui::{self, Align, Layout, RichText, ScrollArea};
use reqwest::blocking::Client;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
use uuid::Uuid;

const ENDPOINT: &str = "http://127.0.0.1:8787";

pub struct AgentifiApp {
    endpoint: String,
    status: String,
    sessions: Vec<AgentSession>,
    ui: UiState,
    prompt: String,
    local_server: Option<Child>,
    events: Option<Receiver<()>>,
    live: bool,
}
impl AgentifiApp {
    pub fn new() -> Self {
        let mut app = Self {
            endpoint: ENDPOINT.into(),
            status: "Connecting…".into(),
            sessions: vec![],
            ui: UiState::default(),
            prompt: String::new(),
            local_server: None,
            events: None,
            live: false,
        };
        app.connect_or_start();
        app
    }
    fn connect_or_start(&mut self) {
        if self.refresh().is_err() {
            if let Ok(child) = spawn_server() {
                self.local_server = Some(child);
                for _ in 0..30 {
                    if self.refresh().is_ok() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        self.start_events();
    }
    fn refresh(&mut self) -> Result<(), String> {
        let response = reqwest::blocking::get(format!("{}/api/v1/sessions", self.endpoint))
            .map_err(|e| e.to_string())?;
        self.sessions = response.json().map_err(|e| e.to_string())?;
        self.status = format!("{} sessions", self.sessions.len());
        Ok(())
    }
    fn start_events(&mut self) {
        let endpoint = self.endpoint.clone();
        let (tx, rx) = mpsc::channel();
        self.events = Some(rx);
        self.live = true;
        thread::spawn(move || {
            let Ok(response) = Client::new()
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
    fn command(&mut self, method: &str, id: Uuid, message: Option<String>) {
        let mut params = serde_json::json!({"session_id": id});
        if let Some(message) = message {
            params["message"] = message.into();
        }
        let body =
            serde_json::json!({"jsonrpc":"2.0","id":"desktop","method":method,"params":params});
        match Client::new()
            .post(format!("{}/api/v1/rpc", self.endpoint))
            .json(&body)
            .send()
        {
            Ok(r) if r.status().is_success() => self.status = format!("{method} accepted"),
            Ok(r) => self.status = format!("{method} failed: {}", r.status()),
            Err(e) => self.status = format!("{method} failed: {e}"),
        }
    }
    fn select(&mut self, id: Uuid) {
        self.ui.selected = Some(id);
        self.ui.view = AppView::Workspace;
        self.command("sessions.attach", id, None);
    }
    fn selected(&self) -> Option<AgentSession> {
        self.ui
            .selected
            .and_then(|id| self.sessions.iter().find(|s| s.id == id).cloned())
    }
    fn filtered(&self) -> Vec<AgentSession> {
        let q = self.ui.search.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| {
                let text = format!("{} {} {}", s.title, s.project, status(s)).to_lowercase();
                (q.is_empty() || text.contains(&q))
                    && self
                        .ui
                        .status_filter
                        .as_ref()
                        .is_none_or(|f| status(s) == f)
            })
            .cloned()
            .collect()
    }
}
impl Drop for AgentifiApp {
    fn drop(&mut self) {
        if let Some(mut child) = self.local_server.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl eframe::App for AgentifiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply(ctx);
        if let Some(rx) = &self.events {
            if rx.try_iter().next().is_some() {
                let _ = self.refresh();
            }
        }
        egui::SidePanel::left("navigation")
            .exact_width(218.0)
            .frame(egui::Frame::NONE.fill(theme::SIDEBAR_BG).inner_margin(16))
            .show(ctx, |ui| self.navigation(ui));
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::NONE
                    .fill(theme::APP_BG)
                    .inner_margin(egui::Margin::symmetric(20, 14)),
            )
            .show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR_BG)
                    .inner_margin(egui::Margin::symmetric(20, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.small(&self.status);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.small(if self.live {
                            "● Connected / SSE"
                        } else {
                            "○ Offline"
                        });
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::APP_BG).inner_margin(20))
            .show(ctx, |ui| match self.ui.view {
                AppView::Overview => self.overview(ui),
                AppView::Explorer => self.explorer(ui),
                AppView::Board => self.board(ui),
                AppView::Workspace => self.workspace(ui),
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
impl AgentifiApp {
    fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.heading(
            RichText::new("Agentifi")
                .size(21.0)
                .color(theme::TEXT_PRIMARY),
        );
        ui.small(RichText::new("PI SESSION CONTROL").color(theme::TEXT_MUTED));
        ui.add_space(24.0);
        ui.label(RichText::new("WORKSPACE").small().color(theme::TEXT_MUTED));
        for (view, label, icon) in [
            (AppView::Overview, views::OVERVIEW_LABEL, Icon::Overview),
            (AppView::Explorer, views::EXPLORER_LABEL, Icon::Explorer),
            (AppView::Board, views::BOARD_LABEL, Icon::Board),
        ] {
            if nav_button(ui, self.ui.view == view, icon, label) {
                self.ui.view = view;
            }
        }
        ui.add_space(22.0);
        ui.label(RichText::new("PROJECTS").small().color(theme::TEXT_MUTED));
        let mut projects: Vec<_> = self.sessions.iter().map(|s| s.project.as_str()).collect();
        projects.sort();
        projects.dedup();
        for project in projects.into_iter().take(8) {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Folder").color(theme::BLUE));
                ui.small(project);
            });
        }
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.separator();
            ui.horizontal(|ui| {
                ui.colored_label(if self.live { theme::GREEN } else { theme::RED }, "●");
                ui.small(if self.live { "Connected" } else { "Offline" });
            });
        });
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new(match self.ui.view {
                    AppView::Overview => views::OVERVIEW_LABEL,
                    AppView::Explorer => views::EXPLORER_LABEL,
                    AppView::Board => views::BOARD_LABEL,
                    AppView::Workspace => views::WORKSPACE_LABEL,
                })
                .color(theme::TEXT_PRIMARY),
            );
            ui.separator();
            ui.add_sized(
                [260.0, 28.0],
                egui::TextEdit::singleline(&mut self.ui.search).hint_text("Search sessions…"),
            );
            if ui.button("Filters").clicked() {
                self.ui.status_filter = if self.ui.status_filter.is_some() {
                    None
                } else {
                    Some("active".into())
                };
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Refresh").clicked() {
                    let _ = self.refresh();
                }
            });
        });
    }
    fn overview(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Session insights and activity metrics").color(theme::TEXT_SECONDARY),
        );
        ui.add_space(16.0);
        ui.columns(4, |cols| {
            metric(
                &mut cols[0],
                self.sessions.len().to_string(),
                "TOTAL SESSIONS",
                theme::BLUE,
            );
            metric(
                &mut cols[1],
                self.sessions
                    .iter()
                    .filter(|s| matches!(s.status, SessionStatus::Active))
                    .count()
                    .to_string(),
                "ACTIVE",
                theme::GREEN,
            );
            metric(
                &mut cols[2],
                self.sessions
                    .iter()
                    .filter_map(|s| s.source_path.as_ref())
                    .count()
                    .to_string(),
                "DISCOVERED",
                theme::PURPLE,
            );
            metric(
                &mut cols[3],
                self.sessions
                    .iter()
                    .filter(|s| matches!(s.status, SessionStatus::Completed))
                    .count()
                    .to_string(),
                "COMPLETED",
                theme::ORANGE,
            );
        });
        ui.add_space(16.0);
        ui.columns(2, |cols| {
            theme::surface().show(&mut cols[0], |ui| {
                ui.heading("Recent sessions");
                ui.add_space(8.0);
                for s in self
                    .sessions
                    .iter()
                    .rev()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    session_row(ui, &s, false);
                }
            });
            theme::surface().show(&mut cols[1], |ui| {
                ui.heading("Live activity");
                ui.add_space(8.0);
                ui.label(
                    RichText::new(if self.live {
                        "Listening for Pi and session events…"
                    } else {
                        "SSE disconnected"
                    })
                    .color(theme::TEXT_SECONDARY),
                );
                ui.separator();
                ui.small("Select a session to open its live workspace.");
            });
        });
    }
    fn explorer(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!("{} sessions", self.filtered().len()))
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(12.0);
        theme::surface().show(ui, |ui| {
            for s in self.filtered() {
                let id = s.id;
                if session_row(ui, &s, self.ui.selected == Some(id)).clicked() {
                    self.select(id);
                }
            }
        });
    }
    fn board(&mut self, ui: &mut egui::Ui) {
        let lanes = [
            ("Inbox", None, theme::TEXT_SECONDARY),
            ("Active", Some("active"), theme::GREEN),
            ("Paused", Some("paused"), theme::ORANGE),
            ("Completed", Some("completed"), theme::BLUE),
        ];
        ui.horizontal_wrapped(|ui| {
            for (name, filter, color) in lanes {
                theme::surface().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(color, "●");
                        ui.heading(name);
                    });
                    ui.separator();
                    for s in self
                        .sessions
                        .iter()
                        .filter(|s| filter.is_none_or(|f| status(s) == f))
                        .cloned()
                        .collect::<Vec<_>>()
                    {
                        if session_row(ui, &s, false).clicked() {
                            self.select(s.id);
                        }
                    }
                });
            }
        });
    }
    fn workspace(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.selected() else {
            ui.centered_and_justified(|ui| ui.label("Select a session from Explorer or Board."));
            return;
        };
        ui.horizontal(|ui| {
            if ui.button("Back").clicked() {
                self.ui.view = AppView::Explorer;
            }
            ui.heading(&session.title);
            ui.colored_label(theme::GREEN, format!("● {}", status(&session)));
        });
        ui.label(
            RichText::new(format!(
                "{}  ·  {}",
                session.project,
                session.source_path.as_deref().unwrap_or("Pi session")
            ))
            .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(12.0);
        ui.columns(3, |cols| {
            theme::surface().show(&mut cols[0], |ui| {
                ui.heading("Context");
                ui.separator();
                ui.label("Related sessions");
                ui.label("Project files");
                ui.label("Session diagnostics");
            });
            theme::surface().show(&mut cols[1], |ui| {
                ui.heading("Conversation");
                ui.separator();
                ui.label(
                    RichText::new("Pi events arrive through SSE.").color(theme::TEXT_SECONDARY),
                );
                ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    ui.label("Attach state and tool output will appear here.");
                });
            });
            theme::surface().show(&mut cols[2], |ui| {
                ui.heading("Inspector");
                ui.separator();
                ui.label(format!("Status: {}", status(&session)));
                ui.label(format!("ID: {}", session.id));
                ui.add_space(12.0);
                ui.heading("Prompt");
                ui.add(egui::TextEdit::multiline(&mut self.prompt).desired_rows(5));
                ui.horizontal(|ui| {
                    if ui.button("Send").clicked() {
                        let text = std::mem::take(&mut self.prompt);
                        if !text.trim().is_empty() {
                            self.command("sessions.prompt", session.id, Some(text));
                        }
                    }
                    if ui.button("Steer").clicked() {
                        let text = std::mem::take(&mut self.prompt);
                        if !text.trim().is_empty() {
                            self.command("sessions.steer", session.id, Some(text));
                        }
                    }
                    if ui.button("Abort").clicked() {
                        self.command("sessions.abort", session.id, None);
                    }
                });
            });
        });
    }
}
fn spawn_server() -> Result<Child, String> {
    let path = std::env::var_os("AGENTIFI_SERVER_COMMAND")
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
    Command::new(path)
        .env("AGENTIFI_BIND", ENDPOINT.trim_start_matches("http://"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())
}
