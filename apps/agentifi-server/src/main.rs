use agentifi_adapters::InMemorySessionRepository;
use agentifi_application::SessionService;
use agentifi_domain::AgentSession;
use axum::{extract::State, routing::get, Json, Router};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    sessions: Arc<SessionService>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let repository = Arc::new(InMemorySessionRepository::default());
    repository.seed(AgentSession::new("agentifi", "Welcome to Agentifi"));
    let state = AppState {
        sessions: Arc::new(SessionService::new(repository)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/sessions", get(list_sessions))
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
) -> Result<Json<Vec<AgentSession>>, (axum::http::StatusCode, String)> {
    state
        .sessions
        .list_sessions()
        .await
        .map(Json)
        .map_err(|error| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            )
        })
}
