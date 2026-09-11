//! View composition.
//!
//! Views read a [`ViewContext`] and push [`Action`]s. They never talk to the
//! network themselves, which keeps rendering pure and the shell in charge of
//! command sequencing.

pub mod board;
pub mod explorer;
pub mod overview;
pub mod session_workspace;

use crate::{events::ActivityEvent, state::UiState};
use agentifi_domain::AgentSession;
use uuid::Uuid;

/// Work the shell should perform after a view has been drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Refresh,
    Attach(Uuid),
    Prompt(Uuid, String),
    Steer(Uuid, String),
    FollowUp(Uuid, String),
    Abort(Uuid),
}

/// Everything a view is allowed to see.
pub struct ViewContext<'a> {
    pub state: &'a mut UiState,
    pub sessions: &'a [AgentSession],
    pub events: &'a [ActivityEvent],
    /// Current unix time, so relative ages are computed once per frame.
    pub now: i64,
    pub live: bool,
    pub endpoint: &'a str,
    pub actions: Vec<Action>,
}

impl ViewContext<'_> {
    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Currently selected session, if it still exists in the catalog.
    #[must_use]
    pub fn selected(&self) -> Option<&AgentSession> {
        let id = self.state.selected?;
        self.sessions.iter().find(|session| session.id == id)
    }

    /// Projects with session counts, most sessions first.
    #[must_use]
    pub fn projects(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for session in self.sessions {
            *counts.entry(session.project.clone()).or_default() += 1;
        }
        let mut projects: Vec<(String, usize)> = counts.into_iter().collect();
        projects.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        projects
    }

    /// Distinct models present in the catalog.
    #[must_use]
    pub fn models(&self) -> Vec<String> {
        let mut models: Vec<String> = self
            .sessions
            .iter()
            .filter_map(|session| session.model.clone())
            .collect();
        models.sort();
        models.dedup();
        models
    }
}
