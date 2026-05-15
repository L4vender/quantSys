use adapter_therundown::{app, config};
use anyhow::{bail, Context};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use futures_util::StreamExt;
use quantsys_eventbus::InMemoryEventProducer;
use quantsys_source_sdk::therundown::{
    build_ws_url, redact_ws_url, InMemoryDlqSink, ReqwestRestTransport, TheRundownAdapter,
};
use quantsys_telemetry::init_json_logging;
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Parser)]
#[command(name = "adapter-therundown")]
#[command(about = "TheRundown Phase 3 raw ingestion adapter")]
struct Args {
    #[arg(long, default_value = "configs/sources/therundown.example.toml")]
    config: PathBuf,
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long)]
    date: Option<String>,
    #[arg(long)]
    sport_id: Option<u32>,
    #[arg(long)]
    last_id: Option<String>,
    #[arg(long, default_value = "0.0.0.0:8093")]
    health_bind: String,
}

#[derive(Clone, Debug, ValueEnum)]
enum Mode {
    Probe,
    Bootstrap,
    Delta,
    Ws,
    Health,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_json_logging("info,adapter_therundown=debug,quantsys_source_sdk=debug");
    let args = Args::parse();

    if matches!(args.mode, Mode::Health) {
        return serve_health(&args.health_bind).await;
    }

    let source_config = config::load_config(&args.config)?;
    let api_key = config::load_api_key(&source_config)?;
    let filters = config::subscription_filters(&source_config)?;
    let adapter_config = config::adapter_config(&source_config);
    let mut adapter = TheRundownAdapter::new(
        adapter_config,
        api_key.clone(),
        ReqwestRestTransport::new(),
        InMemoryEventProducer::default(),
        InMemoryDlqSink::default(),
    );

    match args.mode {
        Mode::Probe => {
            let headers = adapter.probe().await?;
            println!(
                "therundown probe ok tier={:?} delay={:?} websocket_access={:?} rate_limit={:?} datapoints_remaining={:?} live_signal_allowed={} live_execution_allowed=false",
                headers.tier,
                headers.data_delay_seconds,
                headers.websocket_access,
                headers.rate_limit,
                headers.datapoints_remaining,
                adapter.state().live_signal_allowed,
            );
        }
        Mode::Bootstrap => {
            let sport_id = args
                .sport_id
                .context("--sport-id is required for --mode bootstrap")?;
            let date = normalize_date(args.date.as_deref());
            let raw = adapter.bootstrap_events(sport_id, &date).await?;
            println!(
                "published raw.therundown channel=rest_bootstrap raw_id={} provider_event_id={:?} cursor={:?}",
                raw.raw_id,
                raw.provider_event_id,
                adapter.cursor().last_id()
            );
        }
        Mode::Delta => {
            let last_id = args
                .last_id
                .as_deref()
                .context("--last-id is required for --mode delta")?;
            let raw = adapter.markets_delta(last_id).await?;
            println!(
                "published raw.therundown channel=rest_delta raw_id={} cursor={:?}",
                raw.raw_id,
                adapter.cursor().last_id()
            );
        }
        Mode::Ws => {
            let url = build_ws_url(&source_config.ws_url, &api_key, &filters)?;
            println!("connecting therundown ws {}", redact_ws_url(&url));
            run_ws_loop(
                &mut adapter,
                &url,
                Duration::from_millis(source_config.ws_connect_timeout_ms),
                Duration::from_secs(source_config.stale_after_seconds),
                source_config.max_reconnect_attempts,
            )
            .await?;
        }
        Mode::Health => unreachable!("handled before config loading"),
    }

    Ok(())
}

async fn serve_health(bind: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding adapter-therundown health server to {bind}"))?;
    tracing::info!(
        service = "adapter-therundown",
        bind,
        "health server listening"
    );
    axum::serve(listener, app::build_router())
        .await
        .context("serving adapter-therundown health endpoints")
}

async fn run_ws_loop<T>(
    adapter: &mut TheRundownAdapter<T>,
    url: &str,
    connect_timeout: Duration,
    stale_after: Duration,
    max_attempts: u32,
) -> anyhow::Result<()>
where
    T: quantsys_source_sdk::therundown::RestTransport,
{
    let mut attempt = 0_u32;
    loop {
        if attempt > max_attempts {
            bail!("TheRundown websocket reconnect attempts exceeded configured max");
        }
        let connection = tokio::time::timeout(connect_timeout, connect_async(url)).await;
        let (mut stream, _) = match connection {
            Ok(Ok(value)) => value,
            Ok(Err(err)) => {
                let delay = adapter
                    .next_reconnect_delay()
                    .unwrap_or_else(|| Duration::from_millis(500));
                tracing::warn!(error = %err, delay_ms = delay.as_millis(), "ws connect failed");
                tokio::time::sleep(delay).await;
                attempt = attempt.saturating_add(1);
                continue;
            }
            Err(_) => {
                let delay = Duration::from_millis(500);
                tracing::warn!(delay_ms = delay.as_millis(), "ws connect timed out");
                tokio::time::sleep(delay).await;
                attempt = attempt.saturating_add(1);
                continue;
            }
        };

        attempt = 0;
        loop {
            match tokio::time::timeout(stale_after, stream.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    let payload: serde_json::Value = serde_json::from_str(text.as_str())
                        .context("parsing TheRundown websocket JSON")?;
                    let received_at = Utc::now();
                    let raw = adapter
                        .handle_ws_json(
                            payload,
                            received_at,
                            received_at.timestamp_nanos_opt().unwrap_or_default() as u64,
                        )
                        .await?;
                    tracing::debug!(raw_id = %raw.raw_id, "published raw.therundown from ws");
                }
                Ok(Some(Ok(Message::Ping(_)))) | Ok(Some(Ok(Message::Pong(_)))) => {}
                Ok(Some(Ok(Message::Close(frame)))) => {
                    tracing::warn!(?frame, "TheRundown websocket closed");
                    break;
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(err))) => {
                    tracing::warn!(error = %err, "TheRundown websocket read failed");
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    adapter.detect_stale(Utc::now());
                    tracing::warn!("TheRundown websocket stale timeout reached");
                    break;
                }
            }
        }

        let delay = adapter
            .next_reconnect_delay()
            .unwrap_or_else(|| Duration::from_millis(500));
        tokio::time::sleep(delay).await;
        attempt = attempt.saturating_add(1);
    }
}

fn normalize_date(date: Option<&str>) -> String {
    match date {
        Some("today") | None => Utc::now().format("%Y-%m-%d").to_string(),
        Some(value) => value.to_string(),
    }
}
