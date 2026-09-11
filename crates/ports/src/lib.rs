use agentifi_domain::AgentSession;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<AgentSession>>;
}

#[async_trait]
pub trait SessionController: Send + Sync {
    async fn resume(&self, id: uuid::Uuid) -> Result<()>;
    async fn stop(&self, id: uuid::Uuid) -> Result<()>;
}
