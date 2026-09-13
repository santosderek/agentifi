//! Session catalog: discovery plus the live overlay from the supervisor.
//!
//! The repository owns what is on disk; the supervisor owns what is running.
//! `list` merges them so the desktop always sees attachment and working state
//! that reflect reality, and the watcher pushes diffs over the event bus.

use crate::{events::ServerEvent, supervisor::PiSupervisor};
use agentifi_adapters::PiSessionRepository;
use agentifi_domain::{AgentSession, AttachmentState, SessionStatus};
use agentifi_ports::SessionRepository;
use anyhow::Result;
use serde_json::json;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use uuid::Uuid;

pub struct Catalog {
    repository: Arc<PiSessionRepository>,
    supervisor: PiSupervisor,
    events: broadcast::Sender<ServerEvent>,
}

/// Fields whose change is worth pushing to clients.
type Signature = (Option<i64>, Option<u32>, SessionStatus, AttachmentState);

impl Catalog {
    #[must_use]
    pub fn new(
        repository: Arc<PiSessionRepository>,
        supervisor: PiSupervisor,
        events: broadcast::Sender<ServerEvent>,
    ) -> Self {
        Self {
            repository,
            supervisor,
            events,
        }
    }

    /// Discovered sessions with live attachment and working state overlaid.
    pub async fn list(&self) -> Result<Vec<AgentSession>> {
        let mut sessions = self.repository.list().await?;
        let snapshot = self.supervisor.snapshot().await;
        for session in &mut sessions {
            if let Some(attached) = snapshot.get(&session.id.to_string()) {
                session.attachment = AttachmentState::Attached;
                session.status = if attached.working {
                    SessionStatus::Active
                } else {
                    SessionStatus::Idle
                };
            }
        }
        Ok(sessions)
    }

    /// Stored transcript for a session that is not attached.
    pub async fn transcript(&self, id: Uuid) -> Result<Vec<agentifi_domain::SessionMessage>> {
        self.repository.transcript(id).await
    }

    /// Emits `session.discovered` / `session.updated` / `session.removed` as the
    /// Pi session directory changes, so clients refresh without polling.
    pub fn watch(self: &Arc<Self>, interval: Duration) {
        let catalog = Arc::clone(self);
        tokio::spawn(async move {
            // Seed the baseline so a fresh start does not announce every
            // existing session as newly discovered.
            let mut previous: HashMap<Uuid, Signature> = match catalog.list().await {
                Ok(sessions) => sessions.iter().map(|s| (s.id, signature(s))).collect(),
                Err(_) => HashMap::new(),
            };
            loop {
                tokio::time::sleep(interval).await;
                let Ok(sessions) = catalog.list().await else {
                    continue;
                };
                let mut current = HashMap::new();
                for session in &sessions {
                    let signature = signature(session);
                    let id = session.id.to_string();
                    match previous.remove(&session.id) {
                        None => {
                            let _ = catalog.events.send(ServerEvent::new(
                                "session.discovered",
                                json!({
                                    "session_id": id,
                                    "title": session.display_title(),
                                    "project": session.project,
                                }),
                            ));
                        }
                        Some(old) if old != signature => {
                            let _ = catalog.events.send(ServerEvent::new(
                                "session.updated",
                                json!({
                                    "session_id": id,
                                    "title": session.display_title(),
                                    "status": session.status.label(),
                                    "message_count": session.message_count,
                                }),
                            ));
                        }
                        Some(_) => {}
                    }
                    current.insert(session.id, signature);
                }
                for id in previous.keys() {
                    let _ = catalog.events.send(ServerEvent::new(
                        "session.removed",
                        json!({ "session_id": id.to_string() }),
                    ));
                }
                previous = current;
            }
        });
    }
}

fn signature(session: &AgentSession) -> Signature {
    (
        session.updated_at,
        session.message_count,
        session.status,
        session.attachment,
    )
}
