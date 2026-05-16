use adapter_therundown::{
    app, config, event_cache::TheRundownEventCache, watchlist as watchlist_config,
};
use anyhow::{bail, Context};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use futures_util::StreamExt;
use quantsys_domain::WsWatchlist;
use quantsys_eventbus::InMemoryEventProducer;
use quantsys_source_sdk::therundown::{
    build_ws_url, redact_ws_url, InMemoryDlqSink, ReqwestRestTransport, TheRundownAdapter,
};
use quantsys_storage::LocalCsvSink;
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
    #[arg(long)]
    csv_output: Option<PathBuf>,
    #[arg(long, default_value = "output/live-mapping/ws_watchlist.json")]
    watchlist: PathBuf,
    #[arg(long)]
    disable_watchlist: bool,
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
    let base_filters = config::subscription_filters(&source_config)?;
    let adapter_config = config::adapter_config(&source_config);
    let csv_sink = config::local_csv_sink(&source_config, args.csv_output.clone())?;
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
            write_local_csv(csv_sink.as_ref(), &raw)?;
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
            write_local_csv(csv_sink.as_ref(), &raw)?;
            println!(
                "published raw.therundown channel=rest_delta raw_id={} cursor={:?}",
                raw.raw_id,
                adapter.cursor().last_id()
            );
        }
        Mode::Ws => {
            let watchlist = if args.disable_watchlist {
                None
            } else {
                Some(watchlist_config::load_watchlist(&args.watchlist).with_context(|| {
                    "TheRundown WS now uses a matched watchlist by default; run `make live-watchlist` first or pass --disable-watchlist for an intentional full subscription"
                        .to_string()
                })?)
            };
            let filters = if let Some(watchlist) = watchlist.as_ref() {
                let filters =
                    watchlist_config::filters_for_watchlist(base_filters.clone(), watchlist)?;
                tracing::info!(
                    event_count = filters.event_ids.len(),
                    market_count = filters.market_ids.len(),
                    "using matched websocket watchlist for TheRundown subscription"
                );
                filters
            } else {
                base_filters
            };
            let csv_event_cache = if csv_sink.is_some() {
                let date = normalize_date(args.date.as_deref());
                let bootstrap_csv_sink = if watchlist.is_some() {
                    None
                } else {
                    csv_sink.as_ref()
                };
                bootstrap_csv_event_cache(
                    &mut adapter,
                    &source_config.sport_ids,
                    &date,
                    bootstrap_csv_sink,
                )
                .await
            } else {
                TheRundownEventCache::default()
            };
            let url = build_ws_url(&source_config.ws_url, &api_key, &filters)?;
            println!("connecting therundown ws {}", redact_ws_url(&url));
            run_ws_loop(
                &mut adapter,
                &url,
                TheRundownWsRuntime {
                    connect_timeout: Duration::from_millis(source_config.ws_connect_timeout_ms),
                    stale_after: Duration::from_secs(source_config.stale_after_seconds),
                    max_attempts: source_config.max_reconnect_attempts,
                    csv_sink: csv_sink.clone(),
                    csv_event_cache,
                    watchlist,
                },
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

#[derive(Clone, Debug)]
struct TheRundownWsRuntime {
    connect_timeout: Duration,
    stale_after: Duration,
    max_attempts: u32,
    csv_sink: Option<LocalCsvSink>,
    csv_event_cache: TheRundownEventCache,
    watchlist: Option<WsWatchlist>,
}

async fn run_ws_loop<T>(
    adapter: &mut TheRundownAdapter<T>,
    url: &str,
    runtime: TheRundownWsRuntime,
) -> anyhow::Result<()>
where
    T: quantsys_source_sdk::therundown::RestTransport,
{
    let mut attempt = 0_u32;
    loop {
        if attempt > runtime.max_attempts {
            bail!("TheRundown websocket reconnect attempts exceeded configured max");
        }
        let connection = tokio::time::timeout(runtime.connect_timeout, connect_async(url)).await;
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
            match tokio::time::timeout(runtime.stale_after, stream.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    let received_at = Utc::now();
                    let payload: serde_json::Value = serde_json::from_str(text.as_str())
                        .context("parsing TheRundown websocket JSON")?;
                    if let Some(watchlist) = runtime.watchlist.as_ref() {
                        if !watchlist_config::therundown_market_price_allowed_by_watchlist(
                            watchlist, &payload,
                        ) {
                            tracing::debug!(
                                "skipped TheRundown websocket payload outside matched watchlist"
                            );
                            continue;
                        }
                    }
                    let raw = adapter
                        .handle_ws_json(
                            payload,
                            received_at,
                            received_at.timestamp_nanos_opt().unwrap_or_default() as u64,
                        )
                        .await?;
                    write_local_csv(
                        runtime.csv_sink.as_ref(),
                        &runtime.csv_event_cache.enrich_raw_for_local_csv(&raw),
                    )?;
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

async fn bootstrap_csv_event_cache<T>(
    adapter: &mut TheRundownAdapter<T>,
    sport_ids: &[u32],
    date: &str,
    csv_sink: Option<&LocalCsvSink>,
) -> TheRundownEventCache
where
    T: quantsys_source_sdk::therundown::RestTransport,
{
    let mut cache = TheRundownEventCache::default();
    for sport_id in sport_ids {
        match adapter.bootstrap_events(*sport_id, date).await {
            Ok(raw) => {
                cache.upsert_bootstrap_payload(&raw.payload);
                if let Err(err) = write_local_csv(csv_sink, &raw) {
                    tracing::warn!(
                        sport_id,
                        date,
                        error = %err,
                        "failed to write TheRundown bootstrap local CSV"
                    );
                }
                tracing::info!(
                    sport_id,
                    date,
                    "bootstrapped TheRundown event metadata for local CSV"
                );
            }
            Err(err) => {
                tracing::warn!(
                    sport_id,
                    date,
                    error = %err,
                    "failed to bootstrap TheRundown event metadata for local CSV"
                );
            }
        }
    }
    cache
}

fn normalize_date(date: Option<&str>) -> String {
    match date {
        Some("today") | None => Utc::now().format("%Y-%m-%d").to_string(),
        Some(value) => value.to_string(),
    }
}

fn write_local_csv(
    csv_sink: Option<&LocalCsvSink>,
    raw: &quantsys_domain::RawMessage,
) -> anyhow::Result<()> {
    if let Some(sink) = csv_sink {
        let results = sink.write_raw_message(raw)?;
        for result in results {
            tracing::debug!(
                provider_file = %result.provider_file.display(),
                comparison_file = ?result.comparison_file.as_ref().map(|path| path.display().to_string()),
                comparison_status = ?result.comparison_status,
                "appended local CSV row"
            );
        }
    }
    Ok(())
}
