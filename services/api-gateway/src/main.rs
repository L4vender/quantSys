use anyhow::Context;
use quantsys_api_gateway::build_router;
use quantsys_telemetry::init_json_logging;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_json_logging("info,quantsys_api_gateway=debug");
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let address = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("binding api-gateway to {address}"))?;

    tracing::info!(service = "api-gateway", address, "api-gateway listening");
    axum::serve(listener, build_router())
        .await
        .context("serving api-gateway")?;
    Ok(())
}
