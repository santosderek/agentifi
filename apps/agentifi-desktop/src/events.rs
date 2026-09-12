//! Live activity model.
//!
//! Server-sent events arrive as `event:`/`data:` pairs. They are parsed into
//! structured [`ActivityEvent`] values so the Overview feed, the workspace
//! timeline, and the inspector all render the same data instead of raw JSON.

use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// What kind of activity an event represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityKind {
    /// Stream opened or session attached.
    Connection,
    /// A user prompt.
    UserMessage,
    /// Assistant output.
    AgentMessage,
    /// Tool invocation or tool result.
    Tool,
    /// Error reported by the server or Pi.
    Error,
    /// Anything else worth showing in the feed.
    Info,
}

impl ActivityKind {
    /// Short gutter label used in the timeline.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Connection => "link",
            Self::UserMessage => "you",
            Self::AgentMessage => "pi",
            Self::Tool => "tool",
            Self::Error => "error",
            Self::Info => "info",
        }
    }
}

/// One entry in the activity stream.
#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub session_id: Option<String>,
    pub kind: ActivityKind,
    /// Primary line, already trimmed for display.
    pub text: String,
    /// Optional secondary line such as a tool name or error code.
    pub detail: Option<String>,
    /// Unix seconds when the desktop observed the event.
    pub at: i64,
}

impl ActivityEvent {
    #[must_use]
    pub fn local(kind: ActivityKind, text: impl Into<String>) -> Self {
        Self {
            session_id: None,
            kind,
            text: text.into(),
            detail: None,
            at: now(),
        }
    }

    /// True when this event should be shown for `session`.
    #[must_use]
    pub fn belongs_to(&self, session: &str) -> bool {
        self.session_id.as_deref().is_none_or(|id| id == session)
    }

    /// `HH:MM`-style clock label derived from the unix timestamp.
    #[must_use]
    pub fn clock(&self) -> String {
        let minutes_of_day = (self.at.rem_euclid(86_400)) / 60;
        format!("{:02}:{:02}", minutes_of_day / 60, minutes_of_day % 60)
    }
}

/// Parses one SSE `event:`/`data:` pair into an activity entry.
#[must_use]
pub fn parse(event: &str, data: &str) -> Option<ActivityEvent> {
    let payload = serde_json::from_str::<Value>(data).ok();
    let session_id = payload
        .as_ref()
        .and_then(|value| value.get("session_id"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let (kind, text, detail) = match event {
        "connected" => (
            ActivityKind::Connection,
            "Event stream connected".into(),
            None,
        ),
        "session.attached" => (
            ActivityKind::Connection,
            "Session attached".into(),
            session_id.clone().map(short_id),
        ),
        "error" => (
            ActivityKind::Error,
            first_text(payload.as_ref(), data),
            None,
        ),
        "pi.event" => {
            let inner = payload.as_ref().and_then(|value| value.get("payload"));
            classify_pi_event(inner)?
        }
        other => (
            ActivityKind::Info,
            first_text(payload.as_ref(), data),
            Some(other.to_owned()),
        ),
    };

    Some(ActivityEvent {
        session_id,
        kind,
        text: agentifi_domain::truncate_words(&text, 240),
        detail,
        at: now(),
    })
}

/// Converts a persisted Pi JSONL entry into a timeline event.
#[must_use]
pub fn from_history(entry: &Value, session_id: &str) -> Option<ActivityEvent> {
    let payload = entry.get("message").unwrap_or(entry);
    let (kind, text, detail) = classify_pi_event(Some(payload))?;
    Some(ActivityEvent {
        session_id: Some(session_id.to_owned()),
        kind,
        text: agentifi_domain::truncate_words(&text, 240),
        detail,
        at: now(),
    })
}

/// Maps a Pi RPC payload onto a timeline entry.
fn classify_pi_event(payload: Option<&Value>) -> Option<(ActivityKind, String, Option<String>)> {
    let payload = payload?;
    let kind_name = payload
        .get("type")
        .or_else(|| payload.get("event"))
        .or_else(|| payload.get("role"))
        .and_then(Value::as_str)
        .unwrap_or("event");
    let text = extract_text(payload).unwrap_or_else(|| kind_name.to_owned());
    let tool = payload
        .get("name")
        .or_else(|| payload.get("tool"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let kind = match kind_name {
        "user" | "prompt" | "steer" | "follow_up" => ActivityKind::UserMessage,
        "assistant" | "message" | "text" | "delta" => ActivityKind::AgentMessage,
        "tool_use" | "tool_result" | "tool" | "toolCall" | "toolResult" => ActivityKind::Tool,
        "error" | "aborted" => ActivityKind::Error,
        _ => ActivityKind::Info,
    };
    Some((kind, text, tool.or_else(|| Some(kind_name.to_owned()))))
}

/// Pulls displayable text out of a Pi payload, handling block arrays.
fn extract_text(payload: &Value) -> Option<String> {
    for key in ["text", "content", "summary"] {
        match payload.get(key) {
            Some(Value::String(text)) if !text.trim().is_empty() => return Some(text.clone()),
            Some(Value::Array(blocks)) => {
                let text = blocks
                    .iter()
                    .filter_map(|block| {
                        block
                            .get("text")
                            .and_then(Value::as_str)
                            .or_else(|| block.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
            Some(Value::Object(_)) if key == "message" => return extract_text(payload.get(key)?),
            _ => {}
        }
    }
    None
}

fn first_text(payload: Option<&Value>, fallback: &str) -> String {
    payload
        .and_then(extract_text)
        .unwrap_or_else(|| fallback.trim().to_owned())
}

fn short_id(id: String) -> String {
    id.chars().take(8).collect()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_events_are_connection_entries() {
        let event = parse(
            "session.attached",
            r#"{"session_id":"9f1c1c8e-1a2b","state":"attached"}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::Connection);
        assert_eq!(event.detail.as_deref(), Some("9f1c1c8e"));
    }

    #[test]
    fn pi_tool_events_are_classified_and_labelled() {
        let event = parse(
            "pi.event",
            r#"{"session_id":"abc","payload":{"type":"tool_use","name":"cargo test","text":"Running tests"}}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::Tool);
        assert_eq!(event.text, "Running tests");
        assert_eq!(event.detail.as_deref(), Some("cargo test"));
        assert!(event.belongs_to("abc"));
        assert!(!event.belongs_to("other"));
    }

    #[test]
    fn assistant_block_content_is_flattened() {
        let event = parse(
            "pi.event",
            r#"{"payload":{"role":"assistant","content":[{"type":"text","text":"Inspecting"},{"type":"text","text":"the stream"}]}}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::AgentMessage);
        assert_eq!(event.text, "Inspecting the stream");
    }
}
