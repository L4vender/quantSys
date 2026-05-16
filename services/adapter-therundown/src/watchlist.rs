use anyhow::{bail, Context};
use quantsys_domain::WsWatchlist;
use quantsys_source_sdk::therundown::SubscriptionFilters;
use serde_json::Value;
use std::path::Path;

pub fn load_watchlist(path: impl AsRef<Path>) -> anyhow::Result<WsWatchlist> {
    WsWatchlist::load_from_path(path.as_ref())
        .with_context(|| format!("loading websocket watchlist {}", path.as_ref().display()))
}

pub fn filters_for_watchlist(
    mut base: SubscriptionFilters,
    watchlist: &WsWatchlist,
) -> anyhow::Result<SubscriptionFilters> {
    if watchlist.is_empty() {
        bail!("websocket watchlist has no matched market items");
    }
    let event_ids = watchlist.therundown_event_ids();
    let market_ids = watchlist.therundown_market_ids();
    if event_ids.is_empty() || market_ids.is_empty() {
        bail!("websocket watchlist is missing TheRundown event_ids or market_ids");
    }
    base.event_ids = event_ids;
    base.market_ids = market_ids;
    Ok(base)
}

pub fn therundown_market_price_allowed_by_watchlist(
    watchlist: &WsWatchlist,
    payload: &Value,
) -> bool {
    if payload.pointer("/meta/type").and_then(Value::as_str) != Some("market_price") {
        return true;
    }
    let Some(event_id) = payload.pointer("/data/event_id").and_then(value_to_string) else {
        return false;
    };
    let Some(market_id) = payload
        .pointer("/data/market_id")
        .and_then(value_to_string)
        .and_then(|value| value.parse::<u32>().ok())
    else {
        return false;
    };
    let line = payload.pointer("/data/line").and_then(value_to_f64);
    watchlist.allows_therundown_market_price(&event_id, market_id, line)
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64().map(f64::abs),
        Value::String(value) => value.parse::<f64>().ok().map(f64::abs),
        _ => None,
    }
}
