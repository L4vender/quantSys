use chrono::{TimeZone, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel};
use quantsys_storage::LocalCsvSink;
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let output_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/tmp/local-csv-sample"));
    let sink = LocalCsvSink::new(&output_dir)?;

    for (provider, fixture_provider, fixture_name) in [
        (Provider::TheRundown, "therundown", "ws_market_price.json"),
        (Provider::Polymarket, "polymarket", "market_book.json"),
        (
            Provider::Polymarket,
            "polymarket",
            "market_best_bid_ask.json",
        ),
    ] {
        let raw = raw_message(provider, fixture(fixture_provider, fixture_name)?)?;
        sink.write_raw_message(&raw)?;
    }

    println!("{}", output_dir.display());
    Ok(())
}

fn fixture(provider: &str, name: &str) -> Result<Value, Box<dyn Error>> {
    let path = format!(
        "{}/../../tests/fixtures/external/{provider}/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text)?)
}

fn raw_message(provider: Provider, payload: Value) -> Result<RawMessage, Box<dyn Error>> {
    let (provider_event_id, provider_market_id) = match provider {
        Provider::TheRundown => (
            payload.pointer("/data/event_id").and_then(value_to_string),
            payload.pointer("/data/market_id").and_then(value_to_string),
        ),
        Provider::Polymarket => (
            payload.get("market").and_then(value_to_string),
            payload.get("asset_id").and_then(value_to_string),
        ),
    };
    Ok(RawMessage::new(
        provider,
        SourceChannel::WsMarket,
        Some("fixture-replay".to_string()),
        provider_event_id,
        provider_market_id,
        Utc.with_ymd_and_hms(2026, 5, 16, 0, 0, 3).unwrap(),
        1234,
        "raw/fixture/replay.json".to_string(),
        "local_csv.fixture_replay.v1".to_string(),
        payload,
    ))
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}
