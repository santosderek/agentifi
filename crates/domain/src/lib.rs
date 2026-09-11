//! Agentifi domain model.
//!
//! The desktop renders user-facing session information, so the domain carries the
//! presentation-relevant metadata (title, summary, project, branch, model, counts,
//! timestamps) instead of leaking storage details such as JSONL file names or raw
//! UUIDs. Storage paths remain available for diagnostics only.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lifecycle of a Pi session as reported by discovery and supervision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Pi is currently producing output for this session.
    Active,
    /// Attached or resumable, but not currently working.
    #[default]
    Idle,
    /// Intentionally stopped or waiting for user input.
    Paused,
    /// Finished work that still needs a human decision.
    NeedsReview,
    /// Finished and archived.
    Completed,
    /// Ended in an error the user should inspect.
    Failed,
}

impl SessionStatus {
    /// Every status in display order.
    pub const ALL: [Self; 6] = [
        Self::Active,
        Self::Idle,
        Self::Paused,
        Self::NeedsReview,
        Self::Completed,
        Self::Failed,
    ];

    /// Short lowercase label used in filters and metadata lines.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Paused => "paused",
            Self::NeedsReview => "needs review",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// True when Pi is executing and steer/abort are meaningful.
    #[must_use]
    pub fn is_working(self) -> bool {
        matches!(self, Self::Active)
    }

    /// True when the session is finished and the composer should be read-only.
    #[must_use]
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

/// Whether the desktop is following this session's live stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentState {
    /// No supervised Pi process for this session.
    #[default]
    Detached,
    /// A supervised Pi process is running and streaming events.
    Attached,
    /// Persisted session that can be attached on demand.
    Resumable,
}

impl AttachmentState {
    /// Short label for the inspector and session header.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Detached => "detached",
            Self::Attached => "attached",
            Self::Resumable => "resumable",
        }
    }
}

/// User-controlled board organization, deliberately separate from [`SessionStatus`].
///
/// A session can be running while the user files it under `NeedsReview`, so process
/// state and operational state are never the same field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardLane {
    /// Newly discovered or unclassified.
    #[default]
    Inbox,
    /// Being worked on right now.
    Active,
    /// Deliberately on hold.
    Paused,
    /// Waiting on a human decision.
    NeedsReview,
    /// Done, kept for reference.
    Completed,
}

impl BoardLane {
    /// Lanes in board order.
    pub const ALL: [Self; 5] = [
        Self::Inbox,
        Self::Active,
        Self::Paused,
        Self::NeedsReview,
        Self::Completed,
    ];

    /// Column heading.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Active => "Active",
            Self::Paused => "Paused",
            Self::NeedsReview => "Needs review",
            Self::Completed => "Completed",
        }
    }

    /// Default lane for a session that the user has not filed yet.
    #[must_use]
    pub fn from_status(status: SessionStatus) -> Self {
        match status {
            SessionStatus::Active => Self::Active,
            SessionStatus::Idle => Self::Inbox,
            SessionStatus::Paused => Self::Paused,
            SessionStatus::NeedsReview | SessionStatus::Failed => Self::NeedsReview,
            SessionStatus::Completed => Self::Completed,
        }
    }

    /// Neighbouring lane, used by the keyboard lane-move shortcuts.
    #[must_use]
    pub fn shifted(self, delta: i32) -> Self {
        let lanes = Self::ALL;
        let current = lanes.iter().position(|lane| *lane == self).unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, lanes.len() as i32 - 1) as usize;
        lanes[next]
    }
}

/// A Pi agent session as presented by Agentifi.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: Uuid,
    /// Short project label, normally the last component of the working directory.
    pub project: String,
    /// Raw title from storage; may be a file stem and is never displayed directly.
    pub title: String,
    pub status: SessionStatus,
    /// Storage location. Diagnostics only.
    pub source_path: Option<String>,
    /// Explicit user-provided session name, when one exists.
    #[serde(default)]
    pub name: Option<String>,
    /// One-line synopsis, normally derived from the first user prompt.
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub message_count: Option<u32>,
    #[serde(default)]
    pub tool_call_count: Option<u32>,
    /// Unix seconds.
    #[serde(default)]
    pub created_at: Option<i64>,
    /// Unix seconds.
    #[serde(default)]
    pub updated_at: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub attachment: AttachmentState,
    /// Lane the user filed this session under, when different from the default.
    #[serde(default)]
    pub lane: Option<BoardLane>,
}

impl AgentSession {
    /// Creates a minimal session, mostly useful for seeding and tests.
    #[must_use]
    pub fn new(project: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project: project.into(),
            title: title.into(),
            status: SessionStatus::Idle,
            source_path: None,
            name: None,
            summary: None,
            working_directory: None,
            branch: None,
            provider: None,
            model: None,
            message_count: None,
            tool_call_count: None,
            created_at: None,
            updated_at: None,
            tags: Vec::new(),
            attachment: AttachmentState::Detached,
            lane: None,
        }
    }

    /// User-facing title.
    ///
    /// Resolution order: explicit name, human-readable stored title, first prompt,
    /// project directory, then a shortened id as the last resort. Storage-shaped
    /// titles (UUIDs, timestamped file stems) are never shown as the primary label.
    #[must_use]
    pub fn display_title(&self) -> String {
        if let Some(name) = non_empty(self.name.as_deref()) {
            return name.to_owned();
        }
        if !looks_like_storage_name(&self.title) {
            return self.title.clone();
        }
        if let Some(summary) = non_empty(self.summary.as_deref()) {
            return truncate_words(summary, 64);
        }
        if !self.project.trim().is_empty() && self.project != "unknown project" {
            return format!("Session in {}", self.project);
        }
        format!("Session {}", self.short_id())
    }

    /// First eight characters of the id, for diagnostics and fallback titles.
    #[must_use]
    pub fn short_id(&self) -> String {
        self.id.to_string().chars().take(8).collect()
    }

    /// `provider / model` when known.
    #[must_use]
    pub fn model_label(&self) -> Option<String> {
        match (
            non_empty(self.provider.as_deref()),
            non_empty(self.model.as_deref()),
        ) {
            (Some(provider), Some(model)) => Some(format!("{provider} · {model}")),
            (None, Some(model)) => Some(model.to_owned()),
            (Some(provider), None) => Some(provider.to_owned()),
            (None, None) => None,
        }
    }

    /// Lane the board should render this session in.
    #[must_use]
    pub fn effective_lane(&self) -> BoardLane {
        self.lane
            .unwrap_or_else(|| BoardLane::from_status(self.status))
    }

    /// Compact relative age such as `4m` or `2d`, given the current unix time.
    #[must_use]
    pub fn age_label(&self, now: i64) -> String {
        let Some(updated) = self.updated_at else {
            return "—".to_owned();
        };
        relative_age(now.saturating_sub(updated))
    }

    /// Lowercase haystack for search across user-facing fields.
    #[must_use]
    pub fn search_text(&self) -> String {
        let mut text = format!(
            "{} {} {} {}",
            self.display_title(),
            self.project,
            self.status.label(),
            self.tags.join(" ")
        );
        for field in [
            self.summary.as_deref(),
            self.working_directory.as_deref(),
            self.branch.as_deref(),
            self.provider.as_deref(),
            self.model.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            text.push(' ');
            text.push_str(field);
        }
        text.to_lowercase()
    }
}

/// Formats an age in seconds as `now`, `12m`, `3h`, or `5d`.
#[must_use]
pub fn relative_age(seconds: i64) -> String {
    match seconds {
        s if s < 45 => "now".to_owned(),
        s if s < 3_600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s => format!("{}d", s / 86_400),
    }
}

/// True for titles that are storage artefacts rather than human labels.
#[must_use]
pub fn looks_like_storage_name(title: &str) -> bool {
    let title = title.trim();
    if title.is_empty() {
        return true;
    }
    if Uuid::parse_str(title).is_ok() {
        return true;
    }
    let alphanumeric = title
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    let digit_ratio = if alphanumeric.is_empty() {
        1.0
    } else {
        alphanumeric.chars().filter(char::is_ascii_digit).count() as f32 / alphanumeric.len() as f32
    };
    !title.contains(' ') && (digit_ratio > 0.4 || title.len() > 48)
}

/// Truncates on a word boundary and appends an ellipsis when shortened.
#[must_use]
pub fn truncate_words(text: &str, max_chars: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut result = String::new();
    for word in text.split(' ') {
        if result.chars().count() + word.chars().count() + 1 > max_chars {
            break;
        }
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    if result.is_empty() {
        result = text.chars().take(max_chars).collect();
    }
    format!("{result}…")
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_and_timestamp_titles_are_treated_as_storage_names() {
        assert!(looks_like_storage_name(
            "5f4d0d5c-6a5b-4f2c-9a1a-0b3d5f7c9e11"
        ));
        assert!(looks_like_storage_name("20260910-231245-session"));
        assert!(!looks_like_storage_name("Fix SSE reconnect handling"));
    }

    #[test]
    fn display_title_falls_back_to_the_first_prompt() {
        let mut session = AgentSession::new("agentifi", "20260910-231245-session");
        session.summary = Some("Restore the stream after the server restarts".into());
        assert_eq!(
            session.display_title(),
            "Restore the stream after the server restarts"
        );
    }

    #[test]
    fn display_title_prefers_an_explicit_name() {
        let mut session = AgentSession::new("agentifi", "20260910-231245-session");
        session.summary = Some("Restore the stream".into());
        session.name = Some("Fix SSE reconnect handling".into());
        assert_eq!(session.display_title(), "Fix SSE reconnect handling");
    }

    #[test]
    fn display_title_never_exposes_a_raw_identifier() {
        let session = AgentSession::new("", "9f1c1c8e-1a2b-4c3d-9e8f-0a1b2c3d4e5f");
        assert!(session.display_title().starts_with("Session "));
        assert_eq!(session.display_title().len(), "Session ".len() + 8);
    }

    #[test]
    fn lanes_default_from_status_and_shift_within_bounds() {
        assert_eq!(
            BoardLane::from_status(SessionStatus::Failed),
            BoardLane::NeedsReview
        );
        assert_eq!(BoardLane::Inbox.shifted(-1), BoardLane::Inbox);
        assert_eq!(BoardLane::Inbox.shifted(1), BoardLane::Active);
        assert_eq!(BoardLane::Completed.shifted(1), BoardLane::Completed);
    }

    #[test]
    fn relative_ages_are_compact() {
        assert_eq!(relative_age(10), "now");
        assert_eq!(relative_age(600), "10m");
        assert_eq!(relative_age(7_200), "2h");
        assert_eq!(relative_age(172_800), "2d");
    }
}
