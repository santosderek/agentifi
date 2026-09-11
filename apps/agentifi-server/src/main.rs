use agentifi_adapters::PiSessionRepository;
use agentifi_application::SessionService;
use agentifi_domain::AgentSession;
use async_stream::stream;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{convert::Infallible, net::SocketAddr, sync::Arc};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Clone)]
struct AppState {
    sessions: Arc<SessionService>,
    events: broadcast::Sender<ServerEvent>,
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
        events,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/sessions", get(list_sessions))
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
    state
        .sessions
        .list_sessions()
        .await
        .map(Json)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
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
            let session_id = request
                .params
                .get("session_id")
                .cloned()
                .unwrap_or(Value::Null);
            let event = ServerEvent {
                event: "session.attached".into(),
                data: json!({ "session_id": session_id, "state": "attached" }),
            };
            let _ = state.events.send(event);
            RpcResponse {
                jsonrpc: "2.0",
                id: request.id,
                result: Some(json!({ "state": "attached", "session_id": session_id })),
                error: None,
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
