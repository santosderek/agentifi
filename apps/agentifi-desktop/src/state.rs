//! Desktop UI state.
//!
//! Selection, search, filters, sort, board lanes, inspector visibility, and the
//! composer draft all live here so that navigating between views never loses the
//! user's place, and so the filtering rules can be tested without a window.

use agentifi_domain::{AgentSession, BoardLane, SessionStatus};
use std::collections::HashMap;
use uuid::Uuid;

/// Top-level destinations in the shared shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AppView {
    #[default]
    Overview,
    Explorer,
    Board,
    Workspace,
    Settings,
}

impl AppView {
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Explorer => "Explorer",
            Self::Board => "Board",
            Self::Workspace => "Session workspace",
            Self::Settings => "Settings",
        }
    }
}

/// Explorer sort order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    LastActive,
    Title,
    Messages,
}

impl SortKey {
    pub const ALL: [Self; 3] = [Self::LastActive, Self::Title, Self::Messages];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LastActive => "Last active",
            Self::Title => "Title",
            Self::Messages => "Messages",
        }
    }
}

/// Inspector tabs. Protocol and storage details live behind `Diagnostics`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorTab {
    #[default]
    Details,
    Activity,
    Files,
    Diagnostics,
}

impl InspectorTab {
    pub const ALL: [Self; 4] = [
        Self::Details,
        Self::Activity,
        Self::Files,
        Self::Diagnostics,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Details => "Details",
            Self::Activity => "Activity",
            Self::Files => "Files",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

/// Timeline event filter in the session workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimelineFilter {
    #[default]
    All,
    Messages,
    Tools,
}

impl TimelineFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::Messages, Self::Tools];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Messages => "Messages",
            Self::Tools => "Tools",
        }
    }
}

/// Severity of a transient notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Warning,
    Error,
}

/// Transient feedback for commands, connection changes, and errors.
#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    /// Seconds of remaining life; the shell counts this down each frame.
    pub remaining: f32,
}

/// Everything the shell needs to render, minus the session data itself.
#[derive(Clone, Debug, Default)]
pub struct UiState {
    pub view: AppView,
    /// View to return to when the workspace is closed.
    pub previous_view: AppView,
    pub selected: Option<Uuid>,
    pub search: String,
    pub status_filter: Option<SessionStatus>,
    pub project_filter: Option<String>,
    pub model_filter: Option<String>,
    pub sort: SortKey,
    pub inspector_open: bool,
    pub inspector_tab: InspectorTab,
    pub timeline_filter: TimelineFilter,
    pub composer: String,
    /// Operational lane overrides, kept separate from Pi process status.
    pub lanes: HashMap<Uuid, BoardLane>,
    pub confirm_abort: bool,
    pub focus_search: bool,
    pub toasts: Vec<Toast>,
}

impl UiState {
    /// Navigates while remembering where the user came from.
    pub fn go(&mut self, view: AppView) {
        if view != self.view {
            self.previous_view = self.view;
            self.view = view;
        }
    }

    /// Returns to the previous view, defaulting to Explorer from the workspace.
    pub fn back(&mut self) {
        let target = if self.previous_view == AppView::Workspace {
            AppView::Explorer
        } else {
            self.previous_view
        };
        self.view = target;
        self.confirm_abort = false;
    }

    /// Opens a session in the workspace, keeping filters and scroll intent intact.
    pub fn open_session(&mut self, id: Uuid) {
        self.selected = Some(id);
        self.confirm_abort = false;
        self.go(AppView::Workspace);
    }

    pub fn notify(&mut self, tone: Tone, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            tone,
            remaining: 4.5,
        });
        // Keep the stack short so notifications never cover the content.
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    /// Ages toasts out. Returns true when a repaint is still needed.
    pub fn tick_toasts(&mut self, delta: f32) -> bool {
        for toast in &mut self.toasts {
            toast.remaining -= delta;
        }
        self.toasts.retain(|toast| toast.remaining > 0.0);
        !self.toasts.is_empty()
    }

    /// Lane a session belongs to, honouring the user's own filing.
    #[must_use]
    pub fn lane_of(&self, session: &AgentSession) -> BoardLane {
        self.lanes
            .get(&session.id)
            .copied()
            .unwrap_or_else(|| session.effective_lane())
    }

    pub fn set_lane(&mut self, id: Uuid, lane: BoardLane) {
        self.lanes.insert(id, lane);
    }

    #[must_use]
    pub fn has_filters(&self) -> bool {
        self.status_filter.is_some()
            || self.project_filter.is_some()
            || self.model_filter.is_some()
            || !self.search.trim().is_empty()
    }

    pub fn clear_filters(&mut self) {
        self.search.clear();
        self.status_filter = None;
        self.project_filter = None;
        self.model_filter = None;
    }

    /// True when a session passes the current search and filters.
    #[must_use]
    pub fn matches(&self, session: &AgentSession) -> bool {
        let query = self.search.trim().to_lowercase();
        if !query.is_empty() && !session.search_text().contains(&query) {
            return false;
        }
        if let Some(status) = self.status_filter {
            if session.status != status {
                return false;
            }
        }
        if let Some(project) = &self.project_filter {
            if &session.project != project {
                return false;
            }
        }
        if let Some(model) = &self.model_filter {
            if session.model.as_deref() != Some(model.as_str()) {
                return false;
            }
        }
        true
    }

    /// Search + filter + sort, in one pass, for the Explorer and Board.
    #[must_use]
    pub fn visible(&self, sessions: &[AgentSession]) -> Vec<AgentSession> {
        let mut result: Vec<AgentSession> = sessions
            .iter()
            .filter(|session| self.matches(session))
            .cloned()
            .collect();
        match self.sort {
            SortKey::LastActive => {
                result.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
            }
            SortKey::Title => result.sort_by_key(|session| session.display_title().to_lowercase()),
            SortKey::Messages => {
                result.sort_by_key(|session| std::cmp::Reverse(session.message_count));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(project: &str, title: &str, status: SessionStatus) -> AgentSession {
        let mut session = AgentSession::new(project, title);
        session.status = status;
        session
    }

    #[test]
    fn search_matches_user_facing_fields_only() {
        let mut state = UiState::default();
        let mut target = session(
            "agentifi",
            "Fix SSE reconnect handling",
            SessionStatus::Active,
        );
        target.source_path = Some("/home/deck/.pi/agent/sessions/20260910.jsonl".into());

        state.search = "reconnect".into();
        assert!(state.matches(&target));

        state.search = "jsonl".into();
        assert!(!state.matches(&target));
    }

    #[test]
    fn status_and_project_filters_combine() {
        let mut state = UiState::default();
        let active = session("agentifi", "Fix reconnect", SessionStatus::Active);
        let paused = session("homelab", "Storage audit", SessionStatus::Paused);

        state.status_filter = Some(SessionStatus::Active);
        assert!(state.matches(&active));
        assert!(!state.matches(&paused));

        state.project_filter = Some("homelab".into());
        assert!(!state.matches(&active));
    }

    #[test]
    fn sorting_by_title_is_case_insensitive() {
        let state = UiState {
            sort: SortKey::Title,
            ..UiState::default()
        };
        let sessions = vec![
            session("agentifi", "zeta task", SessionStatus::Idle),
            session("agentifi", "Alpha task", SessionStatus::Idle),
        ];
        let visible = state.visible(&sessions);
        assert_eq!(visible[0].display_title(), "Alpha task");
    }

    #[test]
    fn board_lane_overrides_survive_process_status() {
        let mut state = UiState::default();
        let running = session("agentifi", "Fix reconnect", SessionStatus::Active);
        assert_eq!(state.lane_of(&running), BoardLane::Active);

        state.set_lane(running.id, BoardLane::NeedsReview);
        assert_eq!(state.lane_of(&running), BoardLane::NeedsReview);
        assert_eq!(running.status, SessionStatus::Active);
    }

    #[test]
    fn closing_the_workspace_returns_to_the_previous_view() {
        let mut state = UiState::default();
        state.go(AppView::Explorer);
        state.open_session(Uuid::new_v4());
        state.back();
        assert_eq!(state.view, AppView::Explorer);
    }
}
