use crate::{
    components::{
        agentifi_mark, display_title, nav_button, session_row, status, status_color, Icon,
    },
    state::{AppView, UiState},
    theme, views,
};
use agentifi_domain::{AgentSession, SessionStatus};
use eframe::egui::{self, Align, Layout, RichText, ScrollArea, Stroke};
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
        ui.horizontal(|ui| {
            agentifi_mark(ui, 32.0);
            ui.heading(
                RichText::new("Agentifi")
                    .size(21.0)
                    .color(theme::TEXT_PRIMARY),
            );
        });
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
        ui.add_space(28.0);
        ui.heading(
            RichText::new("Good evening.")
                .size(26.0)
                .monospace()
                .color(theme::TEXT_PRIMARY),
        );
        ui.add_space(6.0);
        let active = self
            .sessions
            .iter()
            .filter(|s| matches!(s.status, SessionStatus::Active))
            .count();
        ui.label(
            RichText::new(format!(
                "{} sessions  ·  {} active  ·  connected to local Pi server",
                self.sessions.len(),
                active
            ))
            .monospace()
            .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(28.0);
        ui.separator();
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            overview_stat(ui, "SESSIONS", self.sessions.len(), theme::TEXT_PRIMARY);
            ui.separator();
            overview_stat(ui, "ACTIVE", active, theme::GREEN);
            ui.separator();
            overview_stat(
                ui,
                "NEEDS REVIEW",
                self.sessions
                    .iter()
                    .filter(|s| matches!(s.status, SessionStatus::Paused))
                    .count(),
                theme::ORANGE,
            );
            ui.separator();
            overview_stat(
                ui,
                "COMPLETED",
                self.sessions
                    .iter()
                    .filter(|s| matches!(s.status, SessionStatus::Completed))
                    .count(),
                theme::TEXT_SECONDARY,
            );
        });
        ui.add_space(34.0);
        ui.label(
            RichText::new("RECENT SESSIONS")
                .monospace()
                .color(theme::TEXT_SECONDARY),
        );
        ui.separator();
        ui.add_space(4.0);
        let recent = self
            .sessions
            .iter()
            .rev()
            .take(6)
            .cloned()
            .collect::<Vec<_>>();
        for session in recent {
            ui.horizontal(|ui| {
                ui.colored_label(status_color(&session), "●");
                ui.vertical(|ui| {
                    ui.label(RichText::new(display_title(&session)).strong());
                    ui.small(
                        RichText::new(format!("{}  ·  {}", session.project, status(&session)))
                            .color(theme::TEXT_SECONDARY),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.small(
                        RichText::new("Now")
                            .monospace()
                            .color(theme::TEXT_SECONDARY),
                    );
                });
            });
            ui.separator();
        }
        ui.add_space(34.0);
        ui.columns(2, |cols| {
            cols[0].label(
                RichText::new("LIVE")
                    .monospace()
                    .color(theme::TEXT_SECONDARY),
            );
            cols[0].separator();
            cols[0].add_space(12.0);
            cols[0].colored_label(
                if self.live { theme::GREEN } else { theme::RED },
                "●  No session attached",
            );
            cols[0].small("Open a session to stream Pi activity.");
            if cols[0].button("Browse sessions").clicked() {
                self.ui.view = AppView::Explorer;
            }
            cols[1].label(
                RichText::new("RECENT ACTIVITY")
                    .monospace()
                    .color(theme::TEXT_SECONDARY),
            );
            cols[1].separator();
            cols[1].add_space(12.0);
            cols[1].label(
                RichText::new("Server connected   localhost")
                    .monospace()
                    .color(theme::TEXT_SECONDARY),
            );
            cols[1].label(
                RichText::new("Session discovery  local Pi server")
                    .monospace()
                    .color(theme::TEXT_SECONDARY),
            );
            cols[1].label(
                RichText::new("SSE stream         listening")
                    .monospace()
                    .color(theme::TEXT_SECONDARY),
            );
        });
    }
    fn explorer(&mut self, ui: &mut egui::Ui) {
        let sessions = self.filtered();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} sessions", sessions.len())).color(theme::TEXT_SECONDARY),
            );
            ui.separator();
            ui.add_sized(
                [300.0, 28.0],
                egui::TextEdit::singleline(&mut self.ui.search)
                    .hint_text("Search sessions, prompts, paths…"),
            );
            if ui.button("Status").clicked() {
                self.ui.status_filter = if self.ui.status_filter.is_some() {
                    None
                } else {
                    Some("active".into())
                };
            }
            let _ = ui.button("Project");
            let _ = ui.button("Model");
            let _ = ui.button("Last active");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let _ = ui.button("Refresh");
            });
        });
        ui.add_space(16.0);
        ui.columns(2, |cols| {
            theme::surface().show(&mut cols[0], |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Session");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.small("Status   Activity   Model");
                    });
                });
                ui.separator();
                for session in sessions.clone() {
                    let id = session.id;
                    if session_row(ui, &session, self.ui.selected == Some(id)).clicked() {
                        self.select(id);
                    }
                }
            });
            theme::surface().show(&mut cols[1], |ui| {
                if let Some(session) = self.selected() {
                    ui.horizontal(|ui| {
                        ui.colored_label(status_color(&session), "●");
                        ui.heading(display_title(&session));
                    });
                    ui.add_space(14.0);
                    ui.label(RichText::new("Project").color(theme::TEXT_SECONDARY));
                    ui.label(&session.project);
                    ui.add_space(10.0);
                    ui.label(RichText::new("Working directory").color(theme::TEXT_SECONDARY));
                    ui.label(session.source_path.as_deref().unwrap_or("Not reported"));
                    ui.add_space(10.0);
                    ui.label(RichText::new("Status").color(theme::TEXT_SECONDARY));
                    ui.label(status(&session));
                    ui.add_space(20.0);
                    if ui.button("Open workspace").clicked() {
                        self.ui.view = AppView::Workspace;
                        self.command("sessions.attach", session.id, None);
                    }
                    if ui.button("Copy session ID").clicked() {
                        ui.ctx().copy_text(session.id.to_string());
                    }
                } else {
                    ui.centered_and_justified(|ui| ui.label("Select a session to inspect it."));
                }
            });
        });
    }
    fn board(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} sessions", self.sessions.len()))
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_sized(
                [250.0, 28.0],
                egui::TextEdit::singleline(&mut self.ui.search).hint_text("Search board…"),
            );
            let _ = ui.button("Project: All");
            let _ = ui.button("Model");
            let _ = ui.button("Labels");
            let _ = ui.button("Group by status");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let _ = ui.button("Refresh");
            });
        });
        ui.label(
            RichText::new("Organize agent work by operational state.").color(theme::TEXT_SECONDARY),
        );
        ui.add_space(18.0);
        let lanes = [
            ("Inbox", theme::PURPLE, None),
            ("Active", theme::GREEN, Some("active")),
            ("Needs review", theme::ORANGE, Some("paused")),
            ("Completed", theme::TEXT_SECONDARY, Some("completed")),
        ];
        ui.columns(4, |cols| {
            for (column, (name, color, filter)) in lanes.iter().enumerate() {
                let matching = self
                    .sessions
                    .iter()
                    .filter(|s| {
                        filter.is_none_or(|f| status(s) == f)
                            && (self.ui.search.is_empty()
                                || format!("{} {}", display_title(s), s.project)
                                    .to_lowercase()
                                    .contains(&self.ui.search.to_lowercase()))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let total = matching.len();
                cols[column].horizontal(|ui| {
                    ui.colored_label(*color, "●");
                    ui.heading(*name);
                    ui.small(total.to_string());
                });
                cols[column].separator();
                if matching.is_empty() {
                    cols[column].add_space(18.0);
                    cols[column].label(RichText::new("No sessions").color(theme::TEXT_MUTED));
                }
                for session in matching {
                    if board_card(
                        &mut cols[column],
                        &session,
                        column == 1 && self.ui.selected == Some(session.id),
                    )
                    .clicked()
                    {
                        self.select(session.id);
                    }
                }
            }
        });
    }
    fn workspace(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.selected() else {
            ui.centered_and_justified(|ui| ui.label("Select a session from Explorer or Board."));
            return;
        };
        theme::surface().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Back to Explorer").clicked() {
                    self.ui.view = AppView::Explorer;
                }
                ui.separator();
                ui.small("Explorer");
                ui.label("/");
                ui.small(&session.project);
                ui.label("/");
                ui.strong(display_title(&session));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let _ = ui.button("More");
                    if ui.button("Attach").clicked() {
                        self.command("sessions.attach", session.id, None);
                    }
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.colored_label(theme::GREEN, "●");
                ui.heading(display_title(&session));
                ui.small(format!(
                    "{}  ·  {}  ·  {}",
                    session.project,
                    status(&session),
                    session.source_path.as_deref().unwrap_or("Pi session")
                ));
            });
        });
        ui.add_space(10.0);
        ui.columns(3, |cols| {
            theme::surface().show(&mut cols[0], |ui| {
                ui.heading("Context");
                ui.separator();
                ui.collapsing("Files changed", |ui| {
                    ui.small("No file changes reported yet");
                });
                ui.collapsing("Git", |ui| {
                    ui.small("Branch information unavailable");
                });
                ui.separator();
                ui.heading("Related sessions");
                for related in self.sessions.iter().filter(|s| s.id != session.id).take(5) {
                    ui.horizontal(|ui| {
                        ui.colored_label(theme::GREEN, "●");
                        ui.small(display_title(related));
                    });
                }
            });
            theme::surface().show(&mut cols[1], |ui| {
                ui.heading("Conversation & activity");
                ui.horizontal(|ui| {
                    let _ = ui.button("All");
                    let _ = ui.button("Messages");
                    let _ = ui.button("Tools");
                });
                ui.separator();
                ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    ui.label(RichText::new("You").strong());
                    ui.label("Session attached through Agentifi JSON-RPC.");
                    ui.add_space(18.0);
                    ui.label(RichText::new("Pi").strong());
                    ui.label(
                        RichText::new("Pi events and tool output will stream here through SSE.")
                            .color(theme::TEXT_SECONDARY),
                    );
                });
            });
            theme::surface().show(&mut cols[2], |ui| {
                ui.heading("Session inspector");
                ui.horizontal(|ui| {
                    let _ = ui.button("Details");
                    let _ = ui.button("Activity");
                    let _ = ui.button("Diagnostics");
                });
                ui.separator();
                inspector_row(ui, "Status", status(&session));
                inspector_row(ui, "Attachment", "Attached");
                inspector_row(ui, "Project", &session.project);
                inspector_row(
                    ui,
                    "Working directory",
                    session.source_path.as_deref().unwrap_or("Not reported"),
                );
                inspector_row(ui, "Session ID", &session.id.to_string());
            });
        });
        ui.add_space(10.0);
        theme::surface().show(ui, |ui| {
            ui.small(
                RichText::new("Message will be sent to the attached Pi session.")
                    .color(theme::TEXT_SECONDARY),
            );
            ui.horizontal(|ui| {
                ui.add_sized(
                    [ui.available_width() - 300.0, 58.0],
                    egui::TextEdit::multiline(&mut self.prompt)
                        .hint_text("Ask Pi to continue, investigate, or change direction…"),
                );
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
    }
}
fn board_card(ui: &mut egui::Ui, session: &AgentSession, selected: bool) -> egui::Response {
    let response = theme::surface()
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(status_color(session), "●");
                ui.label(RichText::new(display_title(session)).strong());
            });
            ui.add_space(5.0);
            ui.label(
                RichText::new(format!(
                    "{} session requiring operational attention",
                    status(session)
                ))
                .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(8.0);
            ui.small(format!(
                "Folder  {}  ·  {}",
                session.project,
                status(session)
            ));
        })
        .response;
    let response = ui.interact(
        response.rect,
        ui.id().with(session.id),
        egui::Sense::click(),
    );
    if selected {
        ui.painter().rect_stroke(
            response.rect,
            8.0,
            Stroke::new(1.0_f32, theme::BLUE),
            egui::StrokeKind::Inside,
        );
    }
    ui.add_space(10.0);
    response
}
fn overview_stat(ui: &mut egui::Ui, label: &str, value: usize, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "●");
        ui.label(
            RichText::new(label)
                .monospace()
                .color(theme::TEXT_SECONDARY),
        );
        ui.strong(value.to_string());
    });
}
fn inspector_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT_SECONDARY));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.small(value);
        });
    });
    ui.add_space(8.0);
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
