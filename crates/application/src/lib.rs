use agentifi_domain::AgentSession;
use agentifi_ports::SessionRepository;
use anyhow::Result;
use std::sync::Arc;

pub struct SessionService {
    repository: Arc<dyn SessionRepository>,
}

impl SessionService {
    pub fn new(repository: Arc<dyn SessionRepository>) -> Self {
        Self { repository }
    }

    pub async fn list_sessions(&self) -> Result<Vec<AgentSession>> {
        self.repository.list().await
    }
}
