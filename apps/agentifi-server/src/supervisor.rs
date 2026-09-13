//! Supervision of `pi --mode rpc` processes.
//!
//! One supervised process per attached session. Commands are correlated with
//! their responses by request id, asynchronous agent events are forwarded to the
//! event bus, and working-state transitions (`agent_start` … `agent_settled`)
//! are tracked so the catalog can report live status.
//!
//! Protocol reference: https://pi.dev/docs/latest/rpc

use crate::events::ServerEvent;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{broadcast, mpsc, oneshot, Mutex},
};
use uuid::Uuid;

/// How long the server waits for a Pi command response before giving up.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Live state of one attached session, as seen by the catalog.
#[derive(Clone, Copy, Debug)]
pub struct AttachmentSnapshot {
    /// Pi is executing a turn (`agent_start` seen, `agent_settled` not yet).
    pub working: bool,
}

struct AttachedSession {
    commands: mpsc::Sender<Value>,
    working: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct PiSupervisor {
    inner: Arc<Inner>,
}

struct Inner {
    sessions: Mutex<HashMap<String, AttachedSession>>,
    /// Pending command responses, keyed by request id.
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    events: broadcast::Sender<ServerEvent>,
}

impl PiSupervisor {
    #[must_use]
    pub fn new(events: broadcast::Sender<ServerEvent>) -> Self {
        Self {
            inner: Arc::new(Inner {
                sessions: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
                events,
            }),
        }
    }

    /// Live attachment state for every session the supervisor owns.
    pub async fn snapshot(&self) -> HashMap<String, AttachmentSnapshot> {
        self.inner
            .sessions
            .lock()
            .await
            .iter()
            .map(|(id, session)| {
                (
                    id.clone(),
                    AttachmentSnapshot {
                        working: session.working.load(Ordering::Relaxed),
                    },
                )
            })
            .collect()
    }

    /// Spawns (or reuses) the Pi RPC process for a session and returns its state.
    pub async fn attach(&self, session_id: &str, session_path: &str) -> Result<Value> {
        if self.inner.sessions.lock().await.contains_key(session_id) {
            // Already attached: refresh state rather than spawning a second process.
            return self
                .request(session_id, json!({ "type": "get_state" }))
                .await
                .map_err(|error| anyhow!("attached session is unresponsive: {error}"));
        }

        let command = std::env::var("AGENTIFI_PI_COMMAND").unwrap_or_else(|_| "pi".into());
        let mut child = Command::new(command)
            .args(["--mode", "rpc", "--session", session_path])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|error| anyhow!("failed to start Pi: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Pi stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Pi stdout unavailable"))?;

        let (sender, receiver) = mpsc::channel::<Value>(64);
        let working = Arc::new(AtomicBool::new(false));
        self.inner.sessions.lock().await.insert(
            session_id.to_owned(),
            AttachedSession {
                commands: sender,
                working: working.clone(),
            },
        );
        spawn_reader(
            self.inner.clone(),
            session_id.to_owned(),
            child,
            stdin,
            stdout,
            receiver,
            working,
        );

        // Readiness is proven by a successful get_state round trip.
        let state = match self
            .request(session_id, json!({ "type": "get_state" }))
            .await
        {
            Ok(state) => state,
            Err(error) => {
                self.remove(session_id).await;
                return Err(error);
            }
        };
        let _ = self.inner.events.send(ServerEvent::new(
            "session.attached",
            json!({ "session_id": session_id, "state": "attached" }),
        ));
        Ok(state)
    }

    /// Stops the Pi process for a session. Idempotent per session.
    pub async fn detach(&self, session_id: &str) -> Result<()> {
        if self.remove(session_id).await {
            let _ = self.inner.events.send(ServerEvent::new(
                "session.detached",
                json!({ "session_id": session_id, "state": "detached" }),
            ));
            Ok(())
        } else {
            Err(anyhow!("session is not attached"))
        }
    }

    /// Sends a command and waits for its correlated response.
    pub async fn request(&self, session_id: &str, mut command: Value) -> Result<Value> {
        let request_id = format!("agentifi-{}", Uuid::new_v4().simple());
        command["id"] = json!(request_id);
        let (sender, receiver) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .await
            .insert(request_id.clone(), sender);
        if let Err(error) = self.send(session_id, command).await {
            self.inner.pending.lock().await.remove(&request_id);
            return Err(error);
        }
        let response = tokio::time::timeout(REQUEST_TIMEOUT, receiver)
            .await
            .map_err(|_| anyhow!("Pi did not answer within 10s"))?
            .map_err(|_| anyhow!("Pi process exited before answering"))?;
        if response.get("success").and_then(Value::as_bool) != Some(true) {
            let message = response
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("command rejected by Pi");
            return Err(anyhow!(message.to_owned()));
        }
        Ok(response.get("data").cloned().unwrap_or(Value::Null))
    }

    async fn send(&self, session_id: &str, command: Value) -> Result<()> {
        let session = self
            .inner
            .sessions
            .lock()
            .await
            .get(session_id)
            .ok_or_else(|| anyhow!("session is not attached"))?
            .commands
            .clone();
        session
            .send(command)
            .await
            .map_err(|_| anyhow!("Pi process is unavailable"))
    }

    /// Removes a session from the map; true when it was attached.
    /// Dropping the command sender makes the reader task exit and kill the child.
    async fn remove(&self, session_id: &str) -> bool {
        self.inner
            .sessions
            .lock()
            .await
            .remove(session_id)
            .is_some()
    }
}

/// Reads Pi's stdout for one session until the process or the attachment ends.
fn spawn_reader(
    inner: Arc<Inner>,
    session_id: String,
    mut child: Child,
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    mut commands: mpsc::Receiver<Value>,
    working: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(command) => {
                        let mut line = serde_json::to_vec(&command).unwrap_or_default();
                        line.push(b'\n');
                        if stdin.write_all(&line).await.is_err() {
                            break;
                        }
                    }
                    None => break, // detached
                },
                line = lines.next_line() => match line {
                    Ok(Some(line)) => handle_frame(&inner, &session_id, &line, &working).await,
                    _ => break, // Pi exited or closed stdout
                },
            }
        }
        let _ = child.kill().await;
        // Only report detachment when the attachment was still considered live
        // (an explicit detach already removed it from the map).
        let was_attached = inner.sessions.lock().await.remove(&session_id).is_some();
        if was_attached {
            let _ = inner.events.send(ServerEvent::new(
                "session.detached",
                json!({ "session_id": session_id, "state": "detached", "reason": "process-exited" }),
            ));
        }
    });
}

/// Routes one stdout frame: responses resolve pending requests; agent events
/// update working state and are fanned out.
async fn handle_frame(inner: &Arc<Inner>, session_id: &str, line: &str, working: &AtomicBool) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let frame_type = value.get("type").and_then(Value::as_str).unwrap_or("");

    if frame_type == "response" {
        if let Some(request_id) = value.get("id").and_then(Value::as_str) {
            if let Some(sender) = inner.pending.lock().await.remove(request_id) {
                let _ = sender.send(value.clone());
            }
        }
        let _ = inner.events.send(ServerEvent::new(
            "pi.response",
            json!({ "session_id": session_id, "payload": value }),
        ));
        return;
    }

    // Working-state transitions: `agent_start` begins a run, `agent_settled`
    // is the authoritative "fully done" signal.
    let target = match frame_type {
        "agent_start" | "turn_start" | "message_start" | "tool_execution_start" => Some(true),
        "agent_settled" => Some(false),
        _ => None,
    };
    if let Some(target) = target {
        if working.swap(target, Ordering::Relaxed) != target {
            let _ = inner.events.send(ServerEvent::new(
                "session.updated",
                json!({
                    "session_id": session_id,
                    "state": if target { "running" } else { "idle" },
                }),
            ));
        }
    }
    let _ = inner.events.send(ServerEvent::new(
        "pi.event",
        json!({ "session_id": session_id, "payload": value }),
    ));
}
