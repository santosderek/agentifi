use agentifi_adapters::PiSessionRepository;
use agentifi_application::SessionService;
use agentifi_domain::{AgentSession, AttachmentState};
use async_stream::stream;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{broadcast, mpsc, Mutex},
};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Clone)]
struct AppState {
    sessions: Arc<SessionService>,
    events: broadcast::Sender<ServerEvent>,
    pi: Arc<PiSupervisor>,
}

struct PiSupervisor {
    writers: Mutex<HashMap<String, mpsc::Sender<Value>>>,
    events: broadcast::Sender<ServerEvent>,
}

impl PiSupervisor {
    async fn attach(&self, session_id: String, session_path: String) -> anyhow::Result<()> {
        let mut writers = self.writers.lock().await;
        if writers.contains_key(&session_id) {
            return Ok(());
        }
        let command = std::env::var("AGENTIFI_PI_COMMAND").unwrap_or_else(|_| "pi".into());
        let mut child = Command::new(command)
            .args(["--mode", "rpc", "--session", &session_path])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Pi stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Pi stdout unavailable"))?;
        let (tx, mut rx) = mpsc::channel::<Value>(64);
        let events = self.events.clone();
        let id = session_id.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                tokio::select! {
                    Some(command) = rx.recv() => { let mut line = serde_json::to_vec(&command).unwrap_or_default(); line.push(b'\n'); if stdin.write_all(&line).await.is_err() { break; } }
                    result = lines.next_line() => { match result { Ok(Some(line)) => { if let Ok(value) = serde_json::from_str::<Value>(&line) { let _ = events.send(ServerEvent { event: "pi.event".into(), data: json!({"session_id": id, "payload": value}) }); } }, _ => break } }
                }
            }
            let _ = child.kill().await;
        });
        writers.insert(session_id, tx);
        Ok(())
    }
    async fn is_attached(&self, session_id: &str) -> bool {
        self.writers.lock().await.contains_key(session_id)
    }
    async fn send(&self, session_id: &str, command: Value) -> anyhow::Result<()> {
        let writers = self.writers.lock().await;
        writers
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session is not attached"))?
            .send(command)
            .await
            .map_err(|_| anyhow::anyhow!("Pi process is unavailable"))
    }
}

#[derive(Clone, Debug, Serialize)]
struct ServerEvent {
    event: String,
    data: Value,
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: &'static str,
    message: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let repository = Arc::new(PiSessionRepository::discover());
    let (events, _) = broadcast::channel(256);
    let state = AppState {
        sessions: Arc::new(SessionService::new(repository)),
        pi: Arc::new(PiSupervisor {
            writers: Mutex::new(HashMap::new()),
            events: events.clone(),
        }),
        events,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/sessions", get(list_sessions))
        .route("/api/v1/sessions/{id}/messages", get(session_messages))
        .route("/api/v1/events", get(events_stream))
        .route("/api/v1/rpc", post(rpc))
        .with_state(state);
    let address: SocketAddr = std::env::var("AGENTIFI_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "Agentifi server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentSession>>, (StatusCode, String)> {
    let mut sessions = state
        .sessions
        .list_sessions()
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    for session in &mut sessions {
        if state.pi.is_attached(&session.id.to_string()).await {
            session.attachment = AttachmentState::Attached;
        }
    }
    Ok(Json(sessions))
}

async fn session_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Value>>, (StatusCode, String)> {
    let sessions = state
        .sessions
        .list_sessions()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let session = sessions
        .iter()
        .find(|session| session.id.to_string() == id)
        .ok_or((StatusCode::NOT_FOUND, "session not found".into()))?;
    let Some(path) = &session.source_path else {
        return Ok(Json(Vec::new()));
    };
    let contents = std::fs::read_to_string(path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let entries = contents
        .lines()
        .skip(1)
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
    Ok(Json(entries))
}

async fn events_stream(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.events.subscribe();
    let requested_session = query.session_id;
    let stream = BroadcastStream::new(receiver).filter_map(move |message| {
        let requested_session = requested_session.clone();
        match message {
            Ok(event) => {
                if requested_session.is_some()
                    && event.data.get("session_id").and_then(Value::as_str)
                        != requested_session.as_deref()
                {
                    return None;
                }
                Some(Ok(Event::default()
                    .event(event.event)
                    .json_data(event.data)
                    .unwrap_or_else(|_| Event::default().event("error"))))
            }
            Err(_) => Some(Ok(Event::default()
                .event("error")
                .data("event stream lagged"))),
        }
    });
    let stream = stream! {
        yield Ok(Event::default().event("connected").data("agentifi"));
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            yield item;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn rpc(State(state): State<AppState>, Json(request): Json<RpcRequest>) -> Json<RpcResponse> {
    let response = match request.method.as_str() {
        "sessions.list" => match state.sessions.list_sessions().await {
            Ok(sessions) => RpcResponse {
                jsonrpc: "2.0",
                id: request.id,
                result: Some(json!(sessions)),
                error: None,
            },
            Err(error) => error_response(request.id, "INTERNAL_ERROR", error.to_string()),
        },
        "sessions.attach" => {
            let id = request
                .params
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let sessions = state.sessions.list_sessions().await.unwrap_or_default();
            let Some(session) = sessions.iter().find(|s| s.id.to_string() == id) else {
                return Json(error_response(
                    request.id,
                    "SESSION_NOT_FOUND",
                    "Session not found".into(),
                ));
            };
            let Some(path) = session.source_path.clone() else {
                return Json(error_response(
                    request.id,
                    "SESSION_SOURCE_MISSING",
                    "Session has no Pi source path".into(),
                ));
            };
            match state.pi.attach(id.clone(), path).await {
                Ok(()) => {
                    let _ = state.events.send(ServerEvent {
                        event: "session.attached".into(),
                        data: json!({"session_id": id, "state":"attached"}),
                    });
                    RpcResponse {
                        jsonrpc: "2.0",
                        id: request.id,
                        result: Some(json!({"state":"attached","session_id":id})),
                        error: None,
                    }
                }
                Err(error) => error_response(request.id, "PI_START_FAILED", error.to_string()),
            }
        }
        "sessions.prompt" | "sessions.steer" | "sessions.follow_up" | "sessions.abort" => {
            let id = request
                .params
                .get("session_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let typ = match request.method.as_str() {
                "sessions.prompt" => "prompt",
                "sessions.steer" => "steer",
                "sessions.follow_up" => "follow_up",
                _ => "abort",
            };
            let mut command = json!({"id": request.id.clone().unwrap_or(Value::String("desktop-command".into())), "type": typ});
            if typ != "abort" {
                command["message"] = request
                    .params
                    .get("message")
                    .cloned()
                    .unwrap_or(Value::String(String::new()));
            }
            match state.pi.send(id, command).await {
                Ok(()) => RpcResponse {
                    jsonrpc: "2.0",
                    id: request.id,
                    result: Some(json!({"accepted":true,"session_id":id})),
                    error: None,
                },
                Err(error) => error_response(request.id, "PI_UNAVAILABLE", error.to_string()),
            }
        }
        _ => error_response(
            request.id,
            "METHOD_NOT_FOUND",
            format!("Unsupported method: {}", request.method),
        ),
    };
    Json(response)
}

fn error_response(id: Option<Value>, code: &'static str, message: String) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message }),
    }
}
