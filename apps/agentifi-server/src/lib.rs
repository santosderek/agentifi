//! Agentifi local server library.
//!
//! Owns session discovery, supervision of `pi --mode rpc` processes, and the
//! HTTP surface the desktop consumes: the session catalog, an SSE event stream,
//! and a JSON-RPC endpoint for session control. See docs/PI_CONTROL_PROTOCOL.md.

pub mod catalog;
pub mod events;
pub mod rpc;
pub mod supervisor;

use crate::{
    catalog::Catalog,
    events::ServerEvent,
    rpc::{dispatch, RpcContext, RpcRequest},
    supervisor::PiSupervisor,
};
use agentifi_adapters::PiSessionRepository;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

/// How often the catalog watcher rescans the Pi session directory.
const WATCH_INTERVAL: Duration = Duration::from_secs(2);

pub struct AppState {
    pub catalog: Arc<Catalog>,
    pub supervisor: PiSupervisor,
    pub events: broadcast::Sender<ServerEvent>,
}

#[must_use]
pub fn build_state() -> Arc<AppState> {
    let repository = Arc::new(PiSessionRepository::discover());
    let (events, _) = broadcast::channel(1024);
    let supervisor = PiSupervisor::new(events.clone());
    let catalog = Arc::new(Catalog::new(
        Arc::clone(&repository),
        supervisor.clone(),
        events.clone(),
    ));
    catalog.watch(WATCH_INTERVAL);
    Arc::new(AppState {
        catalog,
        supervisor,
        events,
    })
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/v1/sessions", get(list_sessions))
        .route("/api/v1/events", get(events_stream))
        .route("/api/v1/rpc", post(rpc))
        .with_state(state)
}

async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<agentifi_domain::AgentSession>>, (StatusCode, String)> {
    state
        .catalog
        .list()
        .await
        .map(Json)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    session_id: Option<String>,
}

async fn events_stream(
    State(state): State<Arc<AppState>>,
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
    let stream = async_stream::stream! {
        yield Ok(Event::default()
            .event("connected")
            .data(connected_payload()));
        tokio::pin!(stream);
        while let Some(item) = stream.next().await {
            yield item;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn connected_payload() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("connected at {now}")
}

async fn rpc(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RpcRequest>,
) -> Json<rpc::RpcResponse> {
    // Locking keeps the supervisor's session map stable for the duration of one
    // command so attach/prompt sequences from one client stay ordered.
    let context = RpcContext {
        catalog: Arc::clone(&state.catalog),
        supervisor: state.supervisor.clone(),
    };
    let _guard = COMMAND_LOCK.lock().await;
    Json(dispatch(&context, request).await)
}

/// Serializes RPC handling: one in-flight command at a time.
static COMMAND_LOCK: Mutex<()> = Mutex::const_new(());
