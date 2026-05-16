use clap::Parser;
use quantsys_telemetry::init_json_logging;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "once")]
    mode: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_json_logging("info,raw_archive=debug");
    let args = Args::parse();
    tracing::info!(service = "raw-archive", mode = %args.mode, "raw-archive initialized");
    Ok(())
}
