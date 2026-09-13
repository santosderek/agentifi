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
    /// Catalog change: session discovered, updated, or removed.
    Session,
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
            Self::Session => "session",
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
        "session.discovered" | "session.updated" | "session.removed" => (
            ActivityKind::Session,
            session_change_text(event, payload.as_ref()),
            session_id.clone().map(short_id),
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

/// Maps a Pi agent event (https://pi.dev/docs/latest/rpc#events) onto a
/// timeline entry. Delta and bookkeeping frames are dropped so the timeline
/// shows finished thoughts, not streaming noise.
fn classify_pi_event(payload: Option<&Value>) -> Option<(ActivityKind, String, Option<String>)> {
    let payload = payload?;
    let event_type = payload.get("type").and_then(Value::as_str)?;

    match event_type {
        // Streaming deltas and turn bookkeeping: too noisy for the timeline.
        "message_update"
        | "message_start"
        | "turn_start"
        | "turn_end"
        | "agent_start"
        | "agent_end"
        | "bash_execution_update"
        | "tool_execution_update" => None,
        "agent_settled" => Some((ActivityKind::Info, "Agent settled".into(), None)),
        "message_end" => {
            let message = payload.get("message")?;
            let role = message.get("role").and_then(Value::as_str)?;
            let kind = match role {
                "user" => ActivityKind::UserMessage,
                "assistant" => ActivityKind::AgentMessage,
                _ => ActivityKind::Tool,
            };
            let (text, tool) = message_parts(message, kind);
            Some((kind, text, tool))
        }
        "tool_execution_start" | "tool_execution_end" => {
            let tool = payload
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("tool");
            let detail = payload
                .get("args")
                .map(|args| agentifi_domain::truncate_words(&args.to_string(), 80));
            Some((
                ActivityKind::Tool,
                detail.unwrap_or_else(|| tool.to_owned()),
                Some(tool.to_owned()),
            ))
        }
        "queue_update" => {
            let steering = payload
                .get("steering")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            let follow_up = payload
                .get("followUp")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            Some((
                ActivityKind::Info,
                format!("{steering} steering, {follow_up} follow-up queued"),
                None,
            ))
        }
        "compaction_start" => Some((ActivityKind::Info, "Compacting context".into(), None)),
        "extension_error" | "auto_retry_start" => {
            let text = extract_text(payload).unwrap_or_else(|| event_type.to_owned());
            Some((ActivityKind::Error, text, None))
        }
        _ => {
            let text = extract_text(payload).unwrap_or_else(|| event_type.to_owned());
            Some((ActivityKind::Info, text, Some(event_type.to_owned())))
        }
    }
}

/// Flattens one `AgentMessage` into display text plus an optional tool name.
fn message_parts(message: &Value, kind: ActivityKind) -> (String, Option<String>) {
    let mut text = String::new();
    let mut tool = message
        .get("toolName")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match message.get("content") {
        Some(Value::String(value)) => text.push_str(value),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(value) = block.get("text").and_then(Value::as_str) {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(value);
                        }
                    }
                    Some("tool_call") => {
                        tool = tool.or_else(|| {
                            block
                                .get("name")
                                .or_else(|| block.get("toolName"))
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    if kind == ActivityKind::Tool && text.is_empty() {
        text = tool
            .as_deref()
            .map(|name| format!("Tool {name} finished"))
            .unwrap_or_default();
    }
    (text, tool)
}

/// Human line for a catalog change event.
fn session_change_text(event: &str, payload: Option<&Value>) -> String {
    let payload = payload.unwrap_or(&Value::Null);
    match event {
        "session.discovered" => format!(
            "New session: {}",
            payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("untitled")
        ),
        "session.updated" => format!(
            "Session updated: {} ({})",
            payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("untitled"),
            payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("changed"),
        ),
        "session.removed" => "Session removed from the catalog".to_owned(),
        other => other.to_owned(),
    }
}

/// Pulls displayable text out of a Pi payload, handling block arrays.
fn extract_text(payload: &Value) -> Option<String> {
    for key in ["text", "message", "content", "summary"] {
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
            r#"{"session_id":"abc","payload":{"type":"tool_execution_start","toolName":"bash","args":{"command":"cargo test"}}}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::Tool);
        assert_eq!(event.detail.as_deref(), Some("bash"));
        assert!(event.belongs_to("abc"));
        assert!(!event.belongs_to("other"));
    }

    #[test]
    fn message_end_maps_by_role() {
        let event = parse(
            "pi.event",
            r#"{"payload":{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"All tests pass"}]}}}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::AgentMessage);
        assert_eq!(event.text, "All tests pass");
    }

    #[test]
    fn streaming_deltas_are_dropped() {
        assert!(parse(
            "pi.event",
            r#"{"payload":{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Hel"}}"}"#,
        )
        .is_none());
    }

    #[test]
    fn catalog_changes_are_session_events() {
        let event = parse(
            "session.discovered",
            r#"{"session_id":"abc","title":"Refactor the adapter cache"}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::Session);
        assert_eq!(event.text, "New session: Refactor the adapter cache");
    }

    #[test]
    fn assistant_block_content_is_flattened() {
        let event = parse(
            "pi.event",
            r#"{"payload":{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"Inspecting"},{"type":"text","text":"the stream"}]}}}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::AgentMessage);
        assert_eq!(event.text, "Inspecting the stream");
    }

    #[test]
    fn user_message_end_is_a_user_entry() {
        let event = parse(
            "pi.event",
            r#"{"payload":{"type":"message_end","message":{"role":"user","content":"Run the tests again"}}}"#,
        )
        .expect("event");
        assert_eq!(event.kind, ActivityKind::UserMessage);
        assert_eq!(event.text, "Run the tests again");
    }
}
