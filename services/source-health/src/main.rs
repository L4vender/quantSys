use clap::Parser;
use quantsys_telemetry::init_json_logging;
use source_health::{build_router, SourceHealthAppState};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "8085")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_json_logging("info,source_health=debug");
    let args = Args::parse();
    let state = SourceHealthAppState::new(
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    let address = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!(
        service = "source-health",
        address,
        "source-health listening"
    );
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
