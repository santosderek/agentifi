//! Agentifi local server binary.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let state = agentifi_server::build_state();
    let address: std::net::SocketAddr = std::env::var("AGENTIFI_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "Agentifi server listening");
    axum::serve(listener, agentifi_server::build_router(state)).await?;
    Ok(())
}
