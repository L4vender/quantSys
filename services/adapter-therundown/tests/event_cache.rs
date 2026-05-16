use adapter_therundown::event_cache::TheRundownEventCache;
use chrono::{TimeZone, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel};

fn fixture(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/../../tests/fixtures/external/therundown/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn raw(payload: serde_json::Value) -> RawMessage {
    RawMessage::new(
        Provider::TheRundown,
        SourceChannel::WsMarket,
        Some("message-1".to_string()),
        payload.pointer("/data/event_id").and_then(value_to_string),
        payload.pointer("/data/market_id").and_then(value_to_string),
        Utc.with_ymd_and_hms(2026, 5, 16, 0, 0, 3).unwrap(),
        1234,
        "raw/test/ref.json".to_string(),
        "test.schema.v1".to_string(),
        payload,
    )
}

fn value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

#[test]
fn event_cache_enriches_ws_price_with_bootstrap_game_metadata() {
    let mut cache = TheRundownEventCache::default();
    cache.upsert_bootstrap_payload(&fixture("events_bootstrap.json"));

    let enriched = cache.enrich_raw_for_local_csv(&raw(fixture("ws_market_price.json")));

    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/sport")
            .and_then(value_to_string),
        Some("nba".to_string())
    );
    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/event_name")
            .and_then(value_to_string),
        Some("Los Angeles Lakers vs Boston Celtics".to_string())
    );
    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/event_start_time_utc")
            .and_then(value_to_string),
        Some("2026-05-15T23:30:00Z".to_string())
    );
    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/outcomes_by_participant/501")
            .and_then(value_to_string),
        Some("Boston Celtics".to_string())
    );
}

#[test]
fn event_cache_leaves_heartbeat_unenriched() {
    let mut cache = TheRundownEventCache::default();
    cache.upsert_bootstrap_payload(&fixture("events_bootstrap.json"));

    let enriched = cache.enrich_raw_for_local_csv(&raw(fixture("ws_heartbeat.json")));

    assert!(enriched.payload.get("_local_csv").is_none());
}

#[test]
fn event_cache_supports_real_nested_teams_lines_prices_shape() {
    let mut cache = TheRundownEventCache::default();
    cache.upsert_bootstrap_payload(&serde_json::json!({
        "events": [{
            "event_id": "ce54fa14c19b3d5bcaba1a480318ff0d",
            "sport_id": 4,
            "event_date": "2026-05-15T23:00:00Z",
            "teams": [
                {"team_id": 8, "name": "Detroit", "mascot": "Pistons", "is_away": true, "is_home": false},
                {"team_id": 7, "name": "Cleveland", "mascot": "Cavaliers", "is_away": false, "is_home": true}
            ],
            "markets": [{
                "market_id": 1,
                "participants": [
                    {"id": 7, "name": "Cleveland Cavaliers", "lines": [{"prices": {"23": {"id": "204506997", "price": -172}}}]},
                    {"id": 8, "name": "Detroit Pistons", "lines": [{"prices": {"23": {"id": "204503682", "price": 145}}}]}
                ]
            }]
        }]
    }));
    let mut payload = fixture("ws_market_price.json");
    payload["data"]["event_id"] = serde_json::json!("ce54fa14c19b3d5bcaba1a480318ff0d");
    payload["data"]["market_participant_id"] = serde_json::json!(204506997);
    payload["data"]["normalized_market_participant_id"] = serde_json::json!(7);

    let enriched = cache.enrich_raw_for_local_csv(&raw(payload));

    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/event_name")
            .and_then(value_to_string),
        Some("Detroit Pistons vs Cleveland Cavaliers".to_string())
    );
    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/outcomes_by_participant/204506997")
            .and_then(value_to_string),
        Some("Cleveland Cavaliers".to_string())
    );
    assert_eq!(
        enriched
            .payload
            .pointer("/_local_csv/outcomes_by_participant/7")
            .and_then(value_to_string),
        Some("Cleveland Cavaliers".to_string())
    );
}
