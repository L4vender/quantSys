use adapter_polymarket_market::{
    app, config, watchlist as watchlist_config, ws_proxy::connect_ws_with_optional_proxy,
};
use anyhow::{bail, Context};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use quantsys_eventbus::InMemoryEventProducer;
use quantsys_source_sdk::polymarket::{
    build_market_subscription_payload, InMemoryDlqSink, PolymarketMarketAdapter,
    ReqwestPolymarketRestTransport,
};
use quantsys_storage::LocalCsvSink;
use quantsys_telemetry::init_json_logging;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Parser)]
#[command(name = "adapter-polymarket-market")]
#[command(about = "Polymarket Phase 4 market raw ingestion adapter")]
struct Args {
    #[arg(long, default_value = "configs/sources/polymarket.example.toml")]
    config: PathBuf,
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long, default_value = "0.0.0.0:8094")]
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
    Discovery,
    MarketWs,
    Geoblock,
    TimeProbe,
    Health,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_json_logging("info,adapter_polymarket_market=debug,quantsys_source_sdk=debug");
    let args = Args::parse();

    if matches!(args.mode, Mode::Health) {
        return serve_health(&args.health_bind).await;
    }

    let source_config = config::load_config(&args.config)?;
    let adapter_config = config::adapter_config(&source_config);
    let csv_sink = config::local_csv_sink(&source_config, args.csv_output.clone())?;
    let mut adapter = PolymarketMarketAdapter::new(
        adapter_config,
        ReqwestPolymarketRestTransport::new(),
        InMemoryEventProducer::default(),
        InMemoryDlqSink::default(),
    );

    match args.mode {
        Mode::Discovery => {
            let result = adapter.discover_markets().await?;
            write_local_csv(csv_sink.as_ref(), &result.raw)?;
            println!(
                "polymarket discovery ok active_sports_markets={} filtered_closed={} filtered_non_sports={} token_cache_tokens={} topic=raw.polymarket.market",
                result.markets.len(),
                result.filtered_closed,
                result.filtered_non_sports,
                adapter.token_cache().all_token_ids().len()
            );
        }
        Mode::MarketWs => {
            let watchlist = if args.disable_watchlist {
                None
            } else {
                Some(watchlist_config::load_watchlist(&args.watchlist).with_context(|| {
                    "Polymarket market WS now uses a matched watchlist by default; run `make live-watchlist` first or pass --disable-watchlist for an intentional full subscription"
                        .to_string()
                })?)
            };
            let subscription = if let Some(watchlist) = watchlist.as_ref() {
                adapter.discover_markets().await?;
                let asset_ids = watchlist_config::market_assets_for_watchlist(watchlist)?;
                tracing::info!(
                    asset_count = asset_ids.len(),
                    "using matched websocket watchlist for Polymarket market subscription"
                );
                build_market_subscription_payload(&asset_ids, source_config.custom_feature_enabled)?
            } else if source_config.market_channel.assets_ids.is_empty() {
                adapter.discover_markets().await?;
                adapter.market_subscription_payload(source_config.custom_feature_enabled)?
            } else {
                build_market_subscription_payload(
                    &source_config.market_channel.assets_ids,
                    source_config.custom_feature_enabled,
                )?
            };
            run_market_ws_loop(
                &mut adapter,
                &source_config.market_ws_url,
                subscription,
                MarketWsRuntime {
                    connect_timeout: Duration::from_millis(source_config.ws_connect_timeout_ms),
                    heartbeat_interval: Duration::from_secs(
                        source_config.heartbeat_interval_seconds,
                    ),
                    stale_after: Duration::from_secs(source_config.stale_after_seconds),
                    max_attempts: source_config.max_reconnect_attempts,
                    csv_sink: csv_sink.clone(),
                },
            )
            .await?;
        }
        Mode::Geoblock => {
            let status = adapter.probe_geoblock().await?;
            println!(
                "polymarket geoblock blocked={} country={:?} region={:?} ip=<redacted-ip> live_execution_allowed=false",
                status.blocked, status.country, status.region
            );
        }
        Mode::TimeProbe => {
            let probe = adapter.probe_time_at(Utc::now()).await?;
            println!(
                "polymarket time offset_ms={} large_offset_warning={}",
                probe.offset_ms, probe.large_offset_warning
            );
        }
        Mode::Health => unreachable!("handled before config loading"),
    }

    Ok(())
}

async fn serve_health(bind: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding adapter-polymarket-market health server to {bind}"))?;
    tracing::info!(
        service = "adapter-polymarket-market",
        bind,
        "health server listening"
    );
    axum::serve(listener, app::build_router())
        .await
        .context("serving adapter-polymarket-market health endpoints")
}

#[derive(Clone, Debug)]
struct MarketWsRuntime {
    connect_timeout: Duration,
    heartbeat_interval: Duration,
    stale_after: Duration,
    max_attempts: u32,
    csv_sink: Option<LocalCsvSink>,
}

async fn run_market_ws_loop<T>(
    adapter: &mut PolymarketMarketAdapter<T>,
    url: &str,
    subscription: serde_json::Value,
    runtime: MarketWsRuntime,
) -> anyhow::Result<()>
where
    T: quantsys_source_sdk::polymarket::PolymarketRestTransport,
{
    let mut attempt = 0_u32;
    loop {
        if attempt > runtime.max_attempts {
            bail!("Polymarket market websocket reconnect attempts exceeded configured max");
        }
        let connection =
            tokio::time::timeout(runtime.connect_timeout, connect_ws_with_optional_proxy(url))
                .await;
        let (mut stream, _) = match connection {
            Ok(Ok(value)) => value,
            Ok(Err(err)) => {
                let delay = adapter
                    .next_reconnect_delay()
                    .unwrap_or_else(|| Duration::from_millis(500));
                tracing::warn!(error = %err, delay_ms = delay.as_millis(), "market ws connect failed");
                tokio::time::sleep(delay).await;
                attempt = attempt.saturating_add(1);
                continue;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                attempt = attempt.saturating_add(1);
                continue;
            }
        };

        stream
            .send(Message::Text(subscription.to_string().into()))
            .await
            .context("sending Polymarket market subscription")?;
        attempt = 0;
        let mut heartbeat = tokio::time::interval(runtime.heartbeat_interval);
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    stream.send(Message::Ping(Vec::new().into())).await.context("sending market ws ping")?;
                }
                message = tokio::time::timeout(runtime.stale_after, stream.next()) => {
                    match message {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            let received_at = Utc::now();
                            let payload: serde_json::Value = serde_json::from_str(text.as_str())
                                .context("parsing Polymarket market websocket JSON")?;
                            let raw = adapter.handle_market_ws_json(
                                payload,
                                received_at,
                                received_at.timestamp_nanos_opt().unwrap_or_default() as u64,
                            ).await?;
                            write_local_csv(runtime.csv_sink.as_ref(), &enrich_raw_for_local_csv(adapter, &raw))?;
                        }
                        Ok(Some(Ok(Message::Pong(_)))) => adapter.mark_pong(Utc::now()),
                        Ok(Some(Ok(Message::Ping(bytes)))) => {
                            stream.send(Message::Pong(bytes)).await.context("responding to market ws ping")?;
                        }
                        Ok(Some(Ok(Message::Close(frame)))) => {
                            tracing::warn!(?frame, "Polymarket market websocket closed");
                            break;
                        }
                        Ok(Some(Ok(_))) => {}
                        Ok(Some(Err(err))) => {
                            tracing::warn!(error = %err, "Polymarket market websocket read failed");
                            break;
                        }
                        Ok(None) => break,
                        Err(_) => {
                            adapter.detect_stale(Utc::now());
                            tracing::warn!("Polymarket market websocket stale timeout reached");
                            break;
                        }
                    }
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

fn enrich_raw_for_local_csv<T>(
    adapter: &PolymarketMarketAdapter<T>,
    raw: &quantsys_domain::RawMessage,
) -> quantsys_domain::RawMessage
where
    T: quantsys_source_sdk::polymarket::PolymarketRestTransport,
{
    if raw.source_channel != quantsys_domain::SourceChannel::WsMarket {
        return raw.clone();
    }

    let condition_id = raw
        .provider_event_id
        .as_deref()
        .or_else(|| raw.payload.get("market").and_then(Value::as_str));
    let asset_id = raw
        .provider_market_id
        .as_deref()
        .or_else(|| raw.payload.get("asset_id").and_then(Value::as_str))
        .or_else(|| {
            raw.payload
                .get("changes")
                .and_then(Value::as_array)
                .and_then(|changes| changes.first())
                .and_then(|change| change.get("asset_id"))
                .and_then(Value::as_str)
        });

    let market = condition_id
        .and_then(|condition_id| adapter.token_cache().market_for_condition(condition_id))
        .or_else(|| asset_id.and_then(|asset_id| adapter.token_cache().market_for_token(asset_id)));
    let Some(market) = market else {
        return raw.clone();
    };

    let mut enriched = raw.clone();
    let mut outcomes_by_token = Map::new();
    for (idx, token_id) in market.token_ids.iter().enumerate() {
        if let Some(outcome) = market.outcome_names.get(idx) {
            outcomes_by_token.insert(token_id.clone(), Value::String(outcome.clone()));
        }
    }
    let event_name = market
        .event_title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| market.market_title.clone());
    enriched.payload["_local_csv"] = json!({
        "sport": market.sport,
        "league": market.league,
        "event_name": event_name,
        "event_start_time_utc": market.start_time,
        "market_type": market.market_type,
        "line": market.line,
        "event_id": market.event_id,
        "condition_id": market.condition_id,
        "outcomes_by_token": outcomes_by_token,
    });
    enriched
}
