use adapter_polymarket_user::{app, config};
use anyhow::{bail, Context};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use quantsys_eventbus::InMemoryEventProducer;
use quantsys_source_sdk::polymarket::{
    build_user_subscription_payload, InMemoryDlqSink, PolymarketUserAdapter,
};
use quantsys_telemetry::init_json_logging;
use std::path::PathBuf;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Parser)]
#[command(name = "adapter-polymarket-user")]
#[command(about = "Polymarket Phase 4 user raw ingestion adapter")]
struct Args {
    #[arg(long, default_value = "configs/sources/polymarket.example.toml")]
    config: PathBuf,
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long, default_value = "0.0.0.0:8095")]
    health_bind: String,
    #[arg(
        long,
        default_value = "output/live-mapping/polymarket_user_markets.json"
    )]
    markets_file: PathBuf,
}

#[derive(Clone, Debug, ValueEnum)]
enum Mode {
    UserWs,
    AuthCheck,
    Health,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_json_logging("info,adapter_polymarket_user=debug,quantsys_source_sdk=debug");
    let args = Args::parse();

    if matches!(args.mode, Mode::Health) {
        return serve_health(&args.health_bind).await;
    }

    let source_config = config::load_config(&args.config)?;
    let mut adapter = PolymarketUserAdapter::new(
        config::adapter_config(&source_config),
        InMemoryEventProducer::default(),
        InMemoryDlqSink::default(),
    );
    let credentials = config::load_l2_credentials(&source_config)?;

    match args.mode {
        Mode::AuthCheck => {
            if credentials.is_some() {
                println!("polymarket user auth configured; credentials redacted");
            } else {
                let (api, secret, passphrase) = source_config.l2_auth_env_names();
                adapter.mark_auth_missing();
                println!(
                    "polymarket user auth_missing; set env vars {api}, {secret}, {passphrase}; user ws disabled"
                );
            }
        }
        Mode::UserWs => {
            let Some(credentials) = credentials else {
                let (api, secret, passphrase) = source_config.l2_auth_env_names();
                adapter.mark_auth_missing();
                println!(
                    "polymarket user auth_missing; set env vars {api}, {secret}, {passphrase}; not connecting user ws"
                );
                return Ok(());
            };
            let markets = if source_config.user_channel.markets.is_empty() {
                config::load_markets_file(&args.markets_file)?
            } else {
                source_config.user_channel.markets.clone()
            };
            if markets.is_empty() {
                adapter.mark_auth_missing();
                println!(
                    "polymarket user subscription requires user_channel.markets condition IDs or {}; not connecting user ws",
                    args.markets_file.display()
                );
                return Ok(());
            }
            let subscription = build_user_subscription_payload(&credentials, &markets)?;
            run_user_ws_loop(
                &mut adapter,
                &source_config.user_ws_url,
                subscription,
                Duration::from_millis(source_config.ws_connect_timeout_ms),
                Duration::from_secs(source_config.heartbeat_interval_seconds),
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
        .with_context(|| format!("binding adapter-polymarket-user health server to {bind}"))?;
    tracing::info!(
        service = "adapter-polymarket-user",
        bind,
        "health server listening"
    );
    axum::serve(listener, app::build_router())
        .await
        .context("serving adapter-polymarket-user health endpoints")
}

async fn run_user_ws_loop(
    adapter: &mut PolymarketUserAdapter,
    url: &str,
    subscription: serde_json::Value,
    connect_timeout: Duration,
    heartbeat_interval: Duration,
    stale_after: Duration,
    max_attempts: u32,
) -> anyhow::Result<()> {
    let mut attempt = 0_u32;
    loop {
        if attempt > max_attempts {
            bail!("Polymarket user websocket reconnect attempts exceeded configured max");
        }
        let connection = tokio::time::timeout(connect_timeout, connect_async(url)).await;
        let (mut stream, _) = match connection {
            Ok(Ok(value)) => value,
            Ok(Err(err)) => {
                let delay = adapter
                    .next_reconnect_delay()
                    .unwrap_or_else(|| Duration::from_millis(500));
                tracing::warn!(error = %err, delay_ms = delay.as_millis(), "user ws connect failed");
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
            .context("sending Polymarket user subscription")?;
        attempt = 0;
        let mut heartbeat = tokio::time::interval(heartbeat_interval);
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    stream.send(Message::Ping(Vec::new().into())).await.context("sending user ws ping")?;
                }
                message = tokio::time::timeout(stale_after, stream.next()) => {
                    match message {
                        Ok(Some(Ok(Message::Text(text)))) => {
                            let received_at = Utc::now();
                            let payload: serde_json::Value = serde_json::from_str(text.as_str())
                                .context("parsing Polymarket user websocket JSON")?;
                            adapter.handle_user_ws_json(
                                payload,
                                received_at,
                                received_at.timestamp_nanos_opt().unwrap_or_default() as u64,
                            ).await?;
                        }
                        Ok(Some(Ok(Message::Pong(_)))) => adapter.mark_pong(Utc::now()),
                        Ok(Some(Ok(Message::Ping(bytes)))) => {
                            stream.send(Message::Pong(bytes)).await.context("responding to user ws ping")?;
                        }
                        Ok(Some(Ok(Message::Close(frame)))) => {
                            tracing::warn!(?frame, "Polymarket user websocket closed");
                            break;
                        }
                        Ok(Some(Ok(_))) => {}
                        Ok(Some(Err(err))) => {
                            tracing::warn!(error = %err, "Polymarket user websocket read failed");
                            break;
                        }
                        Ok(None) => break,
                        Err(_) => {
                            adapter.detect_stale(Utc::now());
                            tracing::warn!("Polymarket user websocket stale timeout reached");
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
