//! Session discovery adapters.
//!
//! `PiSessionRepository` reads Pi's persisted JSONL transcripts without starting any
//! Pi process, and derives the user-facing metadata the desktop needs: a readable
//! title, a one-line summary, project, working directory, model, counts, and
//! timestamps.

use agentifi_domain::{AgentSession, AttachmentState, MessageRole, SessionMessage, SessionStatus};
use agentifi_ports::SessionRepository;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// Sessions younger than this are reported as idle rather than completed.
const RECENT_SESSION_SECONDS: i64 = 3_600;
/// Upper bound on lines parsed per transcript so discovery stays responsive.
const MAX_SCANNED_LINES: usize = 4_000;

pub struct InMemorySessionRepository {
    sessions: RwLock<Vec<AgentSession>>,
}

impl Default for InMemorySessionRepository {
    fn default() -> Self {
        Self {
            sessions: RwLock::new(Vec::new()),
        }
    }
}
impl InMemorySessionRepository {
    pub fn seed(&self, session: AgentSession) {
        self.sessions
            .write()
            .expect("repository lock")
            .push(session);
    }
}
#[async_trait]
impl SessionRepository for InMemorySessionRepository {
    async fn list(&self) -> Result<Vec<AgentSession>> {
        Ok(self.sessions.read().expect("repository lock").clone())
    }

    async fn transcript(&self, _id: Uuid) -> Result<Vec<SessionMessage>> {
        Ok(Vec::new())
    }
}

/// Header fields Pi writes on the first line of a session transcript.
#[derive(Debug, Default, Deserialize)]
struct PiSessionHeader {
    id: Option<Uuid>,
    cwd: Option<String>,
    #[serde(alias = "title", alias = "session_name")]
    name: Option<String>,
    #[serde(alias = "git_branch")]
    branch: Option<String>,
    provider: Option<String>,
    model: Option<String>,
}

/// Reads Pi's persisted JSONL session headers without starting Pi processes.
pub struct PiSessionRepository {
    root: PathBuf,
}
impl PiSessionRepository {
    pub fn discover() -> Self {
        let root = std::env::var_os("PI_CODING_AGENT_SESSION_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".pi/agent/sessions")))
            .unwrap_or_else(|| PathBuf::from(".pi/agent/sessions"));
        Self { root }
    }
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    fn files(&self) -> Vec<PathBuf> {
        fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, out);
                } else if path.extension().is_some_and(|e| e == "jsonl") {
                    out.push(path);
                }
            }
        }
        let mut result = Vec::new();
        visit(&self.root, &mut result);
        result.sort();
        result
    }

    fn read_session(path: &Path) -> Option<AgentSession> {
        let contents = fs::read_to_string(path).ok()?;
        let mut lines = contents.lines();
        let header: PiSessionHeader = lines
            .next()
            .and_then(|line| serde_json::from_str(line).ok())
            .unwrap_or_default();

        let id = header.id.unwrap_or_else(|| {
            Uuid::new_v5(&Uuid::NAMESPACE_URL, path.to_string_lossy().as_bytes())
        });
        let transcript = Transcript::scan(contents.lines().skip(1));
        let (created_at, updated_at) = timestamps(path);
        let now = unix_now();
        let status = if updated_at.is_some_and(|at| now.saturating_sub(at) < RECENT_SESSION_SECONDS)
        {
            SessionStatus::Idle
        } else {
            SessionStatus::Completed
        };

        let working_directory = header.cwd.clone();
        let project = working_directory
            .as_deref()
            .and_then(project_label)
            .unwrap_or_else(|| "unknown project".into());
        let title = header
            .name
            .clone()
            .or_else(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Pi session".into());

        Some(AgentSession {
            id,
            project,
            title,
            status,
            source_path: Some(path.to_string_lossy().into_owned()),
            name: header.name,
            summary: transcript.first_prompt,
            working_directory,
            branch: header.branch,
            provider: header.provider,
            model: transcript.model.or(header.model),
            message_count: Some(transcript.messages),
            tool_call_count: Some(transcript.tool_calls),
            created_at,
            updated_at,
            tags: Vec::new(),
            attachment: AttachmentState::Resumable,
            lane: None,
        })
    }
}
impl PiSessionRepository {
    /// Session id for a transcript file, from its header or a stable path hash.
    fn header_id(path: &Path) -> Option<Uuid> {
        let contents = fs::read_to_string(path).ok()?;
        let header: PiSessionHeader = contents
            .lines()
            .next()
            .and_then(|line| serde_json::from_str(line).ok())
            .unwrap_or_default();
        Some(header.id.unwrap_or_else(|| {
            Uuid::new_v5(&Uuid::NAMESPACE_URL, path.to_string_lossy().as_bytes())
        }))
    }

    /// Parses the stored conversation, oldest first. Non-message entries
    /// (model changes, labels, compactions) are skipped.
    fn read_transcript(path: &Path) -> Vec<SessionMessage> {
        let Ok(contents) = fs::read_to_string(path) else {
            return Vec::new();
        };
        let mut messages: Vec<SessionMessage> = contents
            .lines()
            .skip(1) // session header
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|entry| parse_message_entry(&entry))
            .collect();
        if messages.len() > MAX_TRANSCRIPT_MESSAGES {
            let overflow = messages.len() - MAX_TRANSCRIPT_MESSAGES;
            messages.drain(0..overflow);
        }
        messages
    }
}
#[async_trait]
impl SessionRepository for PiSessionRepository {
    async fn list(&self) -> Result<Vec<AgentSession>> {
        let mut sessions: Vec<AgentSession> = self
            .files()
            .iter()
            .filter_map(|path| Self::read_session(path))
            .collect();
        // Most recently touched first: every view leads with recent work.
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        Ok(sessions)
    }

    async fn transcript(&self, id: Uuid) -> Result<Vec<SessionMessage>> {
        let path = self
            .files()
            .into_iter()
            .find(|path| Self::header_id(path) == Some(id));
        Ok(path
            .map(|path| Self::read_transcript(&path))
            .unwrap_or_default())
    }
}

/// Upper bound on messages returned per transcript request.
const MAX_TRANSCRIPT_MESSAGES: usize = 500;

/// Converts one stored JSONL entry into a display-ready message.
///
/// Pi writes tree entries (`{"type":"message","id",...,"message":{role,...}}`);
/// a flat `{"role":...}` line is also accepted for hand-written fixtures.
fn parse_message_entry(entry: &Value) -> Option<SessionMessage> {
    let message = entry.get("message").unwrap_or(entry);
    let role_name = message.get("role").and_then(Value::as_str)?;
    let role = match role_name {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "toolResult" | "tool_result" | "tool" => MessageRole::Tool,
        _ => return None,
    };

    let (text, tool_name) = message_parts(message, role);
    if text.trim().is_empty() && tool_name.is_none() {
        return None;
    }
    Some(SessionMessage {
        entry_id: entry.get("id").and_then(Value::as_str).map(str::to_owned),
        role,
        text: agentifi_domain::truncate_words(text.trim(), 240),
        tool_name,
        timestamp: entry
            .get("timestamp")
            .or_else(|| message.get("timestamp"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

/// Flattens content blocks into display text plus an optional tool name.
fn message_parts(message: &Value, role: MessageRole) -> (String, Option<String>) {
    let content = message.get("content");
    let mut text = String::new();
    let mut tool_name = message
        .get("toolName")
        .or_else(|| message.get("tool_name"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    match content {
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
                    Some("tool_call") if role == MessageRole::Assistant => {
                        tool_name = tool_name.or_else(|| {
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
    if role == MessageRole::Tool && text.is_empty() {
        if let Some(name) = &tool_name {
            text = format!("Tool {name} finished");
        }
    }
    (text, tool_name)
}

/// Counts and metadata derived from the transcript body.
#[derive(Debug, Default)]
struct Transcript {
    messages: u32,
    tool_calls: u32,
    first_prompt: Option<String>,
    model: Option<String>,
}

impl Transcript {
    fn scan<'a>(lines: impl Iterator<Item = &'a str>) -> Self {
        let mut transcript = Self::default();
        for line in lines.take(MAX_SCANNED_LINES) {
            let Ok(entry) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let role = entry
                .get("role")
                .and_then(Value::as_str)
                .or_else(|| entry.get("type").and_then(Value::as_str))
                .unwrap_or_default();
            match role {
                "user" | "assistant" => {
                    transcript.messages += 1;
                    if role == "user" && transcript.first_prompt.is_none() {
                        transcript.first_prompt = entry_text(&entry)
                            .map(|text| agentifi_domain::truncate_words(&text, 120));
                    }
                }
                "tool_use" | "tool_result" | "tool" => transcript.tool_calls += 1,
                _ => {}
            }
            if transcript.model.is_none() {
                transcript.model = entry
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        }
        transcript
    }
}

/// Extracts plain text from a string or block-array `content` field.
fn entry_text(entry: &Value) -> Option<String> {
    let content = entry.get("content").or_else(|| entry.get("text"))?;
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    let blocks = content.as_array()?;
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
    (!text.trim().is_empty()).then_some(text)
}

/// Last path component of the working directory, used as the project label.
fn project_label(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn timestamps(path: &Path) -> (Option<i64>, Option<i64>) {
    let Ok(metadata) = fs::metadata(path) else {
        return (None, None);
    };
    let seconds = |time: std::io::Result<SystemTime>| {
        time.ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
    };
    (seconds(metadata.created()), seconds(metadata.modified()))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session(dir: &Path, name: &str, lines: &[&str]) {
        fs::create_dir_all(dir).expect("create session dir");
        fs::write(dir.join(name), format!("{}\n", lines.join("\n"))).expect("write session");
    }

    #[tokio::test]
    async fn reads_stored_transcripts_in_entry_order() {
        let root = std::env::temp_dir().join(format!("agentifi-{}", Uuid::new_v4()));
        write_session(
            &root,
            "20260912-220000-transcript.jsonl",
            &[
                r#"{"type":"session","version":3,"id":"11111111-2222-3333-4444-555555555555","cwd":"/work/agentifi"}"#,
                r#"{"type":"message","id":"e1","parentId":null,"timestamp":"2026-09-12T22:00:01Z","message":{"role":"user","content":"Restore the SSE stream"}}"#,
                r#"{"type":"model_change","id":"e2","model":"claude-sonnet"}"#,
                r#"{"type":"message","id":"e3","parentId":"e1","timestamp":"2026-09-12T22:00:05Z","message":{"role":"assistant","content":[{"type":"text","text":"Inspecting"},{"type":"text","text":"the reconnect path"}]}}"#,
                r#"{"type":"message","id":"e4","parentId":"e3","timestamp":"2026-09-12T22:00:07Z","message":{"role":"assistant","content":[{"type":"tool_call","name":"bash","arguments":{}}]}}"#,
                r#"{"type":"message","id":"e5","parentId":"e4","timestamp":"2026-09-12T22:00:09Z","message":{"role":"toolResult","toolName":"bash","content":"ok"}}"#,
            ],
        );

        let repository = PiSessionRepository::new(&root);
        let messages = repository
            .transcript(Uuid::parse_str("11111111-2222-3333-4444-555555555555").expect("id"))
            .await
            .expect("transcript");
        fs::remove_dir_all(&root).ok();

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, agentifi_domain::MessageRole::User);
        assert_eq!(messages[0].text, "Restore the SSE stream");
        assert_eq!(messages[0].entry_id.as_deref(), Some("e1"));
        assert_eq!(messages[1].text, "Inspecting the reconnect path");
        assert_eq!(messages[2].tool_name.as_deref(), Some("bash"));
        assert_eq!(messages[3].role, agentifi_domain::MessageRole::Tool);
    }

    #[tokio::test]
    async fn derives_user_facing_metadata_from_a_transcript() {
        let root = std::env::temp_dir().join(format!("agentifi-{}", Uuid::new_v4()));
        write_session(
            &root,
            "20260910-231245-9f1c1c8e.jsonl",
            &[
                r#"{"cwd":"/home/deck/worktrees/agentifi","provider":"anthropic","branch":"main"}"#,
                r#"{"role":"user","content":"Restore the SSE stream after the server restarts"}"#,
                r#"{"role":"assistant","model":"claude-sonnet","content":[{"type":"text","text":"Inspecting the reconnect path"}]}"#,
                r#"{"type":"tool_use","name":"cargo test"}"#,
            ],
        );

        let sessions = PiSessionRepository::new(&root)
            .list()
            .await
            .expect("list sessions");
        fs::remove_dir_all(&root).ok();

        let session = sessions.first().expect("one discovered session");
        assert_eq!(session.project, "agentifi");
        assert_eq!(
            session.working_directory.as_deref(),
            Some("/home/deck/worktrees/agentifi")
        );
        assert_eq!(session.branch.as_deref(), Some("main"));
        assert_eq!(session.provider.as_deref(), Some("anthropic"));
        assert_eq!(session.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(session.message_count, Some(2));
        assert_eq!(session.tool_call_count, Some(1));
        assert_eq!(session.attachment, AttachmentState::Resumable);
        assert_eq!(
            session.display_title(),
            "Restore the SSE stream after the server restarts"
        );
        assert!(session.source_path.is_some());
    }

    #[tokio::test]
    async fn falls_back_to_the_project_when_no_prompt_exists() {
        let root = std::env::temp_dir().join(format!("agentifi-{}", Uuid::new_v4()));
        write_session(
            &root,
            "20260910-999999-abcdef12.jsonl",
            &[r#"{"cwd":"/home/deck/worktrees/homelab"}"#],
        );

        let sessions = PiSessionRepository::new(&root)
            .list()
            .await
            .expect("list sessions");
        fs::remove_dir_all(&root).ok();

        let session = sessions.first().expect("one discovered session");
        assert_eq!(session.display_title(), "Session in homelab");
        assert_eq!(session.message_count, Some(0));
    }
}
