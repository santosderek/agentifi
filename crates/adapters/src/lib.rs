use agentifi_domain::AgentSession;
use agentifi_ports::SessionRepository;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::RwLock;

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
