//! Transport layer: session catalog, JSON-RPC commands, and the SSE stream.
//!
//! All network work is isolated here so views only ever read view-model data.

use crate::events::{self, ActivityEvent, ActivityKind};
use agentifi_domain::AgentSession;
use reqwest::blocking::Client;
use std::{
    io::{BufRead, BufReader},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use uuid::Uuid;

/// Messages the background stream sends to the UI thread.
pub enum StreamMessage {
    /// The stream is open.
    Connected,
    /// A parsed activity entry.
    Event(ActivityEvent),
    /// The stream ended; the shell shows offline state and retries.
    Disconnected(String),
}

/// JSON-RPC session control methods supported by the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Attach,
    Prompt,
    Steer,
    FollowUp,
    Abort,
}

impl Command {
    #[must_use]
    pub fn method(self) -> &'static str {
        match self {
            Self::Attach => "sessions.attach",
            Self::Prompt => "sessions.prompt",
            Self::Steer => "sessions.steer",
            Self::FollowUp => "sessions.follow_up",
            Self::Abort => "sessions.abort",
        }
    }

    /// Human phrasing used in notifications.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Attach => "Attach",
            Self::Prompt => "Prompt",
            Self::Steer => "Steer",
            Self::FollowUp => "Follow-up",
            Self::Abort => "Abort",
        }
    }
}

/// Blocking client for the local Agentifi server.
pub struct Api {
    endpoint: String,
    client: Client,
}

impl Api {
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Fetches the session catalog.
    pub fn list_sessions(&self) -> Result<Vec<AgentSession>, String> {
        self.client
            .get(format!("{}/api/v1/sessions", self.endpoint))
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?
            .json()
            .map_err(|error| error.to_string())
    }

    pub fn session_history(&self, session: Uuid) -> Result<Vec<ActivityEvent>, String> {
        let values: Vec<serde_json::Value> = self
            .client
            .get(format!(
                "{}/api/v1/sessions/{session}/messages",
                self.endpoint
            ))
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| error.to_string())?
            .json()
            .map_err(|error| error.to_string())?;
        Ok(values
            .iter()
            .filter_map(|value| events::from_history(value, &session.to_string()))
            .collect())
    }

    /// Sends a session control command and reports the server's verdict.
    pub fn command(
        &self,
        command: Command,
        session: Uuid,
        message: Option<String>,
    ) -> Result<(), String> {
        let mut params = serde_json::json!({ "session_id": session });
        if let Some(message) = message {
            params["message"] = message.into();
        }
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "desktop",
            "method": command.method(),
            "params": params,
        });
        let response = self
            .client
            .post(format!("{}/api/v1/rpc", self.endpoint))
            .json(&body)
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("server returned {}", response.status()));
        }
        // The server answers with a JSON-RPC envelope, so a 200 can still carry an error.
        let payload: serde_json::Value = response.json().map_err(|error| error.to_string())?;
        if let Some(error) = payload.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("command rejected");
            return Err(message.to_owned());
        }
        Ok(())
    }

    /// Opens the SSE stream on a background thread and returns its receiver.
    #[must_use]
    pub fn stream(&self) -> Receiver<StreamMessage> {
        let endpoint = self.endpoint.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || read_stream(&endpoint, &sender));
        receiver
    }
}

/// Reads `event:`/`data:` pairs until the stream closes.
fn read_stream(endpoint: &str, sender: &Sender<StreamMessage>) {
    let request = Client::new()
        .get(format!("{endpoint}/api/v1/events"))
        .header("Accept", "text/event-stream")
        .send();
    let response = match request {
        Ok(response) => response,
        Err(error) => {
            let _ = sender.send(StreamMessage::Disconnected(error.to_string()));
            return;
        }
    };
    let _ = sender.send(StreamMessage::Connected);

    let mut event_name = String::from("message");
    for line in BufReader::new(response).lines().map_while(Result::ok) {
        if let Some(name) = line.strip_prefix("event:") {
            event_name = name.trim().to_owned();
        } else if let Some(data) = line.strip_prefix("data:") {
            if let Some(event) = events::parse(&event_name, data.trim()) {
                if sender.send(StreamMessage::Event(event)).is_err() {
                    return;
                }
            }
            event_name = String::from("message");
        }
    }
    let _ = sender.send(StreamMessage::Disconnected("event stream closed".into()));
}

/// Convenience constructor for a locally generated activity entry.
#[must_use]
pub fn local_event(kind: ActivityKind, text: impl Into<String>) -> ActivityEvent {
    ActivityEvent::local(kind, text)
}
