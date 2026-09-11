use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: Uuid,
    pub project: String,
    pub title: String,
    pub status: SessionStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Paused,
    Completed,
}

impl AgentSession {
    pub fn new(project: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project: project.into(),
            title: title.into(),
            status: SessionStatus::Active,
        }
    }
}
