use agentifi_domain::{AgentSession, SessionStatus};
use agentifi_ports::SessionRepository;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};
use uuid::Uuid;

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
}

#[derive(Debug, Deserialize)]
struct PiSessionHeader {
    #[serde(rename = "id")]
    id: Option<Uuid>,
    cwd: Option<String>,
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
        let first = fs::read_to_string(path).ok()?.lines().next()?.to_owned();
        let header: PiSessionHeader = serde_json::from_str(&first).ok()?;
        let id = header
            .id
            .or_else(|| {
                Some(Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    path.to_string_lossy().as_bytes(),
                ))
            })
            .expect("session id");
        let project = header.cwd.unwrap_or_else(|| "unknown project".into());
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Pi session")
            .to_owned();
        Some(AgentSession {
            id,
            project,
            title,
            status: SessionStatus::Completed,
            source_path: Some(path.to_string_lossy().into_owned()),
        })
    }
}
#[async_trait]
impl SessionRepository for PiSessionRepository {
    async fn list(&self) -> Result<Vec<AgentSession>> {
        Ok(self
            .files()
            .iter()
            .filter_map(|p| Self::read_session(p))
            .collect())
    }
}
