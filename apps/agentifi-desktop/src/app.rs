//! Application shell.
//!
//! Owns the window chrome (navigation rail, status rail, toast layer), routing,
//! keyboard shortcuts, and all network wiring. Views stay pure: they read a
//! [`ViewContext`] and hand back [`Action`]s that this module executes.

use crate::{
    api::{self, Api, Command, StreamMessage},
    components::{agentifi_mark, nav_item, rail_connection, rail_project, toast_layer, Icon},
    events::{ActivityEvent, ActivityKind},
    state::{AppView, Tone, UiState},
    theme,
    views::{board, explorer, overview, session_workspace, Action, ViewContext},
};
use agentifi_domain::AgentSession;
use eframe::egui::{self, Align, Align2, Key, Layout, Pos2, RichText, Sense, Vec2};
use std::{
    process::{Child, Stdio},
    sync::mpsc::Receiver,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8787";
/// Activity history is bounded so a long-running session cannot grow without limit.
const MAX_EVENTS: usize = 200;
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

pub struct AgentifiApp {
    api: Api,
    state: UiState,
    sessions: Vec<AgentSession>,
    events: Vec<ActivityEvent>,
    stream: Option<Receiver<StreamMessage>>,
    live: bool,
    status_line: String,
    local_server: Option<Child>,
    /// Set while `g` is held as a navigation prefix (`g o`, `g e`, `g b`).
    goto_pending: bool,
    theme_installed: bool,
    last_frame: Instant,
    reconnect_at: Option<Instant>,
}

impl AgentifiApp {
    #[must_use]
    pub fn new() -> Self {
        let endpoint =
            std::env::var("AGENTIFI_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned());
        let mut app = Self {
            api: Api::new(endpoint),
            state: UiState::default(),
            sessions: Vec::new(),
            events: Vec::new(),
            stream: None,
            live: false,
            status_line: "Connecting…".to_owned(),
            local_server: None,
            goto_pending: false,
            theme_installed: false,
            last_frame: Instant::now(),
            reconnect_at: None,
        };
        app.connect_or_start();
        app
    }

    /// Uses a running server if there is one, otherwise starts the bundled binary.
    fn connect_or_start(&mut self) {
        if self.refresh().is_err() {
            if let Ok(child) = spawn_server(self.api.endpoint()) {
                self.local_server = Some(child);
                for _ in 0..40 {
                    if self.refresh().is_ok() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        self.open_stream();
    }

    fn open_stream(&mut self) {
        self.stream = Some(self.api.stream());
        self.reconnect_at = None;
    }

    fn refresh(&mut self) -> Result<(), String> {
        match self.api.list_sessions() {
            Ok(sessions) => {
                self.status_line = format!(
                    "{} sessions · {}",
                    sessions.len(),
                    self.api.endpoint().trim_start_matches("http://")
                );
                self.sessions = sessions;
                Ok(())
            }
            Err(error) => {
                self.status_line = format!("Catalog unavailable: {error}");
                Err(error)
            }
        }
    }

    /// Drains the SSE channel and keeps the activity log bounded.
    fn pump_stream(&mut self) {
        let mut disconnected = None;
        let mut refresh_needed = false;
        if let Some(stream) = &self.stream {
            for message in stream.try_iter() {
                match message {
                    StreamMessage::Connected => {
                        self.live = true;
                        self.events.push(api::local_event(
                            ActivityKind::Connection,
                            "Connected to the event stream",
                        ));
                        refresh_needed = true;
                    }
                    StreamMessage::Event(event) => {
                        if matches!(event.kind, ActivityKind::Connection) {
                            refresh_needed = true;
                        }
                        self.events.push(event);
                    }
                    StreamMessage::Disconnected(reason) => disconnected = Some(reason),
                }
            }
        }

        if self.events.len() > MAX_EVENTS {
            let overflow = self.events.len() - MAX_EVENTS;
            self.events.drain(0..overflow);
        }

        if let Some(reason) = disconnected {
            self.live = false;
            self.stream = None;
            self.reconnect_at = Some(Instant::now() + RECONNECT_DELAY);
            self.state
                .notify(Tone::Warning, format!("Event stream lost: {reason}"));
        }
        if refresh_needed {
            let _ = self.refresh();
        }
        if self.reconnect_at.is_some_and(|at| Instant::now() >= at) {
            self.open_stream();
        }
    }

    fn run_command(&mut self, command: Command, session: Uuid, message: Option<String>) {
        let echo = message.clone();
        match self.api.command(command, session, message) {
            Ok(()) => {
                if let Some(text) = echo {
                    let mut event = api::local_event(ActivityKind::UserMessage, text);
                    event.session_id = Some(session.to_string());
                    self.events.push(event);
                }
                self.state
                    .notify(Tone::Success, format!("{} accepted", command.label()));
                let _ = self.refresh();
            }
            Err(error) => {
                let mut event =
                    api::local_event(ActivityKind::Error, format!("{}: {error}", command.label()));
                event.session_id = Some(session.to_string());
                self.events.push(event);
                self.state
                    .notify(Tone::Error, format!("{} failed: {error}", command.label()));
            }
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::Refresh => {
                if self.refresh().is_ok() {
                    self.state.notify(Tone::Info, "Catalog refreshed");
                } else {
                    self.state.notify(Tone::Error, "Refresh failed");
                }
            }
            Action::Attach(id) => self.run_command(Command::Attach, id, None),
            Action::Prompt(id, text) => self.run_command(Command::Prompt, id, Some(text)),
            Action::Steer(id, text) => self.run_command(Command::Steer, id, Some(text)),
            Action::FollowUp(id, text) => self.run_command(Command::FollowUp, id, Some(text)),
            Action::Abort(id) => self.run_command(Command::Abort, id, None),
        }
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            agentifi_mark(ui, 18.0);
            ui.label(
                RichText::new("agentifi")
                    .font(theme::font_row_title())
                    .color(theme::TEXT_PRIMARY),
            );
        });
        ui.add_space(theme::SPACE_XL);

        let items = [
            (AppView::Overview, Icon::Overview, overview::LABEL, "g o"),
            (AppView::Explorer, Icon::Explorer, explorer::LABEL, "g e"),
            (AppView::Board, Icon::Board, board::LABEL, "g b"),
        ];
        for (view, icon, label, key) in items {
            if nav_item(ui, self.state.view == view, icon, label, key) {
                self.state.go(view);
            }
        }
        if self.state.view == AppView::Workspace {
            nav_item(
                ui,
                true,
                Icon::Workspace,
                session_workspace::LABEL,
                "esc back",
            );
        }

        ui.add_space(theme::SPACE_XL);
        crate::components::text::section_label(ui, "Projects");
        let projects = project_counts(&self.sessions);
        if projects.is_empty() {
            ui.label(
                RichText::new("No sessions discovered")
                    .font(theme::font_meta())
                    .color(theme::TEXT_MUTED),
            );
        }
        for (project, count) in projects.into_iter().take(8) {
            let selected = self.state.project_filter.as_deref() == Some(project.as_str());
            if rail_project(ui, &project, count, selected) {
                self.state.project_filter = if selected { None } else { Some(project) };
                self.state.go(AppView::Explorer);
            }
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            rail_connection(ui, self.live, self.api.endpoint());
        });
    }

    fn status_rail(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    self.state.view.title(),
                    self.status_line
                ))
                .font(theme::font_label())
                .color(theme::TEXT_MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (label, color) = if self.live {
                    ("stream live", theme::GREEN)
                } else {
                    ("stream offline", theme::TEXT_MUTED)
                };
                let (rect, _) = ui.allocate_exact_size(Vec2::new(120.0, 14.0), Sense::hover());
                let painter = ui.painter();
                crate::components::text::dot(
                    painter,
                    Pos2::new(rect.left() + 4.0, rect.center().y),
                    color,
                );
                crate::components::text::line(
                    painter,
                    Pos2::new(rect.left() + 14.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    theme::font_label(),
                    theme::TEXT_MUTED,
                    100.0,
                );
            });
        });
    }

    /// Global shortcuts. Text input keeps priority, so nothing fires while typing.
    fn shortcuts(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let mut actions = Vec::new();
        let typing = ctx.memory(egui::Memory::focused).is_some();

        let keys: Vec<Key> = ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if !modifiers.command && !modifiers.ctrl && !modifiers.alt => Some(*key),
                    _ => None,
                })
                .collect()
        });

        if ctx.input(|input| input.key_pressed(Key::Escape)) {
            if typing {
                ctx.memory_mut(egui::Memory::stop_text_input);
            } else if self.state.view == AppView::Workspace {
                self.state.back();
            } else {
                self.state.clear_filters();
            }
        }
        if ctx.input(|input| input.key_pressed(Key::F5)) {
            actions.push(Action::Refresh);
        }
        if typing {
            self.goto_pending = false;
            return actions;
        }

        for key in keys {
            if self.goto_pending {
                self.goto_pending = false;
                match key {
                    Key::O => self.state.go(AppView::Overview),
                    Key::E => self.state.go(AppView::Explorer),
                    Key::B => self.state.go(AppView::Board),
                    _ => {}
                }
                continue;
            }
            match key {
                Key::G => self.goto_pending = true,
                Key::Slash => {
                    self.state.go(AppView::Explorer);
                    self.state.focus_search = true;
                }
                Key::R if self.state.view == AppView::Overview => {
                    if let Some(session) = self.sessions.first() {
                        let id = session.id;
                        self.state.open_session(id);
                        actions.push(Action::Attach(id));
                    }
                }
                Key::E if self.state.view == AppView::Overview => {
                    self.state.go(AppView::Explorer);
                }
                Key::B if self.state.view == AppView::Overview => self.state.go(AppView::Board),
                Key::Enter => {
                    if let Some(id) = self.state.selected {
                        self.state.open_session(id);
                        actions.push(Action::Attach(id));
                    }
                }
                _ => {
                    if let Some(index) = digit_index(key) {
                        if let Some(session) = self.sessions.get(index) {
                            let id = session.id;
                            self.state.open_session(id);
                            actions.push(Action::Attach(id));
                        }
                    }
                }
            }
        }
        actions
    }

    /// Composer shortcuts, which intentionally do fire while the draft has focus.
    fn composer_shortcuts(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.state.view != AppView::Workspace {
            return actions;
        }
        let Some(id) = self.state.selected else {
            return actions;
        };
        let send = ctx.input(|input| {
            (input.modifiers.command || input.modifiers.ctrl) && input.key_pressed(Key::Enter)
        });
        let steer = ctx.input(|input| {
            (input.modifiers.command || input.modifiers.ctrl) && input.key_pressed(Key::S)
        });
        let draft = self.state.composer.trim().to_owned();
        if (send || steer) && draft.is_empty() {
            self.state.notify(Tone::Warning, "Write a prompt first");
            return actions;
        }
        let working = self
            .sessions
            .iter()
            .find(|session| session.id == id)
            .is_some_and(|session| session.status.is_working());
        if send {
            self.state.composer.clear();
            actions.push(if working {
                Action::FollowUp(id, draft)
            } else {
                Action::Prompt(id, draft)
            });
        } else if steer {
            self.state.composer.clear();
            actions.push(Action::Steer(id, draft));
        }
        actions
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
        // Fonts and styles are uploaded once; re-installing every frame is wasteful.
        if !self.theme_installed {
            theme::install(ctx);
            self.theme_installed = true;
        }

        let delta = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        self.state.tick_toasts(delta);
        self.pump_stream();

        let mut actions = self.shortcuts(ctx);
        actions.extend(self.composer_shortcuts(ctx));
        let lane_delta = ctx.input(|input| {
            i32::from(input.key_pressed(Key::CloseBracket))
                - i32::from(input.key_pressed(Key::OpenBracket))
        });

        egui::SidePanel::left("navigation")
            .exact_width(theme::RAIL_WIDTH)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR_BG)
                    .inner_margin(egui::Margin::symmetric(14, 18)),
            )
            .show(ctx, |ui| self.navigation(ui));

        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR_BG)
                    .inner_margin(egui::Margin::symmetric(20, 6)),
            )
            .show(ctx, |ui| self.status_rail(ui));

        let now = unix_now();
        let mut context = ViewContext {
            state: &mut self.state,
            sessions: &self.sessions,
            events: &self.events,
            now,
            live: self.live,
            endpoint: self.api.endpoint(),
            actions: Vec::new(),
        };

        if lane_delta != 0 && context.state.view == AppView::Board {
            board::move_selected_lane(&mut context, lane_delta);
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::APP_BG)
                    .inner_margin(egui::Margin::symmetric(22, 18)),
            )
            .show(ctx, |ui| match context.state.view {
                AppView::Overview => overview::show(ui, &mut context),
                AppView::Explorer => explorer::show(ui, &mut context),
                AppView::Board => board::show(ui, &mut context),
                AppView::Workspace => session_workspace::show(ui, &mut context),
            });

        actions.extend(std::mem::take(&mut context.actions));
        toast_layer(ctx, &self.state.toasts.clone());

        for action in dedupe(actions) {
            self.perform(action);
        }

        ctx.request_repaint_after(Duration::from_millis(400));
    }
}

/// Collapses duplicate actions produced in the same frame (for example a click and
/// an Enter press on the same session).
fn dedupe(actions: Vec<Action>) -> Vec<Action> {
    let mut unique: Vec<Action> = Vec::with_capacity(actions.len());
    for action in actions {
        if !unique.contains(&action) {
            unique.push(action);
        }
    }
    unique
}

fn digit_index(key: Key) -> Option<usize> {
    match key {
        Key::Num1 => Some(0),
        Key::Num2 => Some(1),
        Key::Num3 => Some(2),
        Key::Num4 => Some(3),
        Key::Num5 => Some(4),
        Key::Num6 => Some(5),
        Key::Num7 => Some(6),
        Key::Num8 => Some(7),
        Key::Num9 => Some(8),
        _ => None,
    }
}

fn project_counts(sessions: &[AgentSession]) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for session in sessions {
        *counts.entry(session.project.clone()).or_default() += 1;
    }
    let mut projects: Vec<(String, usize)> = counts.into_iter().collect();
    projects.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    projects
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or_default())
        .unwrap_or_default()
}

fn spawn_server(endpoint: &str) -> Result<Child, String> {
    let path = std::env::var_os("AGENTIFI_SERVER_COMMAND")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|dir| dir.join("agentifi-server")))
        })
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|dir| dir.join("target/debug/agentifi-server"))
        })
        .ok_or("server executable not found")?;
    std::process::Command::new(path)
        .env(
            "AGENTIFI_BIND",
            endpoint
                .trim_start_matches("http://")
                .trim_start_matches("https://"),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{dedupe, digit_index, project_counts};
    use crate::views::Action;
    use eframe::egui::Key;
    use uuid::Uuid;

    #[test]
    fn digits_map_to_zero_based_indices() {
        assert_eq!(digit_index(Key::Num1), Some(0));
        assert_eq!(digit_index(Key::Num9), Some(8));
        assert_eq!(digit_index(Key::A), None);
    }

    #[test]
    fn duplicate_actions_collapse() {
        let id = Uuid::nil();
        let actions = vec![Action::Attach(id), Action::Attach(id), Action::Refresh];
        assert_eq!(dedupe(actions).len(), 2);
    }

    #[test]
    fn projects_sort_by_session_count() {
        let mut sessions = Vec::new();
        for project in ["alpha", "beta", "beta"] {
            let mut session = agentifi_domain::AgentSession::new(project, project);
            session.project = project.to_owned();
            sessions.push(session);
        }
        let counts = project_counts(&sessions);
        assert_eq!(counts.first().map(|(name, _)| name.as_str()), Some("beta"));
    }
}
