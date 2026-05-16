use chrono::{TimeZone, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel};
use quantsys_storage::{
    american_odds_to_implied_probability, market_decimal_mid, records_from_raw, CsvProvider,
    CsvProviderRecord, LocalCsvSink, MarketFileKey, MarketLine,
};

fn fixture(provider: &str, name: &str) -> serde_json::Value {
    let path = format!(
        "{}/../../tests/fixtures/external/{provider}/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn raw(
    provider: Provider,
    source_channel: SourceChannel,
    payload: serde_json::Value,
) -> RawMessage {
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
    RawMessage::new(
        provider,
        source_channel,
        Some("message-1".to_string()),
        provider_event_id,
        provider_market_id,
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

fn with_polymarket_local_csv_metadata(mut payload: serde_json::Value) -> serde_json::Value {
    payload["_local_csv"] = serde_json::json!({
        "sport": "nba",
        "league": "nba",
        "event_name": "Los Angeles Lakers vs Boston Celtics",
        "event_start_time_utc": "2026-05-15T23:00:00Z",
        "market_type": "moneyline",
        "line": null,
        "event_id": "pm_event_mock_lal_bos",
        "outcomes_by_token": {
            "pm_asset_yes_mock_001": "Lakers",
            "pm_asset_no_mock_001": "Celtics"
        }
    });
    payload
}

fn with_therundown_local_csv_metadata(mut payload: serde_json::Value) -> serde_json::Value {
    payload["_local_csv"] = serde_json::json!({
        "sport": "nba",
        "league": "nba",
        "event_name": "Los Angeles Lakers vs Boston Celtics",
        "event_start_time_utc": "2026-05-15T23:30:00Z",
        "market_type": "moneyline",
        "line": null,
        "outcomes_by_participant": {
            "9001": "Boston Celtics",
            "9002": "Los Angeles Lakers",
            "501": "Boston Celtics",
            "502": "Los Angeles Lakers"
        }
    });
    payload
}

fn provider_record(
    provider: CsvProvider,
    market_key: MarketFileKey,
    price: &str,
) -> CsvProviderRecord {
    CsvProviderRecord {
        provider,
        market_key,
        row_id: format!("{provider:?}-row"),
        schema_version: "local_csv.v1".to_string(),
        side: "team".to_string(),
        outcome_name: "Los Angeles Lakers".to_string(),
        provider_generated_at: Some("2026-05-15T22:15:12Z".to_string()),
        fetched_at: "2026-05-15T22:15:13Z".to_string(),
        ingest_mono_ns: 44,
        event_id: Some("event-1".to_string()),
        market_id: Some("market-1".to_string()),
        market_participant_id: Some("participant-1".to_string()),
        normalized_market_participant_id: Some("normalized-1".to_string()),
        affiliate_id: Some("19".to_string()),
        sport_id: Some("4".to_string()),
        price_raw: Some(price.to_string()),
        previous_price_raw: Some("-120".to_string()),
        price_delta: Some("2".to_string()),
        is_main_line: Some("true".to_string()),
        event_type: Some("best_bid_ask".to_string()),
        condition_id: Some("condition-1".to_string()),
        token_id: Some("token-1".to_string()),
        asset_id: Some("asset-1".to_string()),
        best_bid: Some("0.47".to_string()),
        best_ask: Some("0.49".to_string()),
        last_trade_price: None,
        mid_price: Some("0.48".to_string()),
        book_depth: Some("4".to_string()),
        updated_at: Some("2026-05-15T22:15:12Z".to_string()),
        quality_flags: vec![],
        raw_ref: "raw/test/ref.json".to_string(),
        payload_hash: "sha256:testhash".to_string(),
        trace_id: "trace-1".to_string(),
    }
}

#[test]
fn market_file_key_generates_safe_stable_readable_names() {
    let key = MarketFileKey::new(
        "NBA",
        "NBA",
        "Los Angeles Lakers / Boston Celtics: Game 7?",
        Some("2026-05-16T23:30:00Z"),
        "spread",
        "full_game",
        MarketLine::Point(-5.5),
    );

    assert_eq!(
        key.file_name("ignored_provider_suffix"),
        "2026-05-16T233000Z_los_angeles_lakers_boston_celtics_game_7_spread_minus_5_5.csv"
    );
    assert!(!key.file_name("ignored").contains('/'));
    assert!(!key.file_name("ignored").contains('?'));
    assert_eq!(key.market_key(), "nba|nba|2026-05-16T23:30:00Z|los_angeles_lakers_boston_celtics_game_7|spread|full_game|minus_5_5");
}

#[test]
fn long_event_names_are_truncated_with_hash_suffix() {
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Los Angeles Lakers and Boston Celtics ".repeat(8),
        Some("2026-05-16T23:30:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );
    let filename = key.file_name("therundown");

    assert!(filename.len() < 180);
    assert!(filename.ends_with("_moneyline.csv"));
    assert!(filename.contains("_h"));
}

#[test]
fn same_market_appends_header_once_and_different_lines_use_different_files() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let moneyline = MarketFileKey::new(
        "nba",
        "nba",
        "Lakers vs Warriors",
        Some("2026-05-16T23:30:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );
    let spread_a = MarketFileKey::new(
        "nba",
        "nba",
        "Lakers vs Warriors",
        Some("2026-05-16T23:30:00Z"),
        "spread",
        "full_game",
        MarketLine::Point(-5.5),
    );
    let spread_b = MarketFileKey::new(
        "nba",
        "nba",
        "Lakers vs Warriors",
        Some("2026-05-16T23:30:00Z"),
        "spread",
        "full_game",
        MarketLine::Point(-6.5),
    );

    sink.write_provider_record(&provider_record(CsvProvider::TheRundown, moneyline, "-118"))
        .unwrap();
    sink.write_provider_record(&provider_record(
        CsvProvider::TheRundown,
        spread_a.clone(),
        "-110",
    ))
    .unwrap();
    let spread_a_path = sink
        .write_provider_record(&provider_record(CsvProvider::TheRundown, spread_a, "-112"))
        .unwrap()
        .provider_file;
    let spread_b_path = sink
        .write_provider_record(&provider_record(CsvProvider::TheRundown, spread_b, "-108"))
        .unwrap()
        .provider_file;

    assert_ne!(spread_a_path, spread_b_path);
    assert!(spread_a_path.to_string_lossy().contains(
        "/therundown/nba/draftkings/2026-05-16T233000Z_lakers_vs_warriors_spread_minus_5_5.csv"
    ));
    let text = std::fs::read_to_string(spread_a_path).unwrap();
    assert_eq!(
        text.lines()
            .filter(|line| line
                .starts_with("data_generated_at,data_fetched_at,bookmaker,affiliate_id,team_a_polymarket_format,team_b_polymarket_format"))
            .count(),
        1
    );
    assert_eq!(text.lines().count(), 3);
}

#[test]
fn concurrent_writes_to_same_market_file_keep_one_header_and_complete_rows() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Lakers vs Warriors",
        Some("2026-05-16T23:30:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );

    let handles = (0..20)
        .map(|idx| {
            let sink = sink.clone();
            let key = key.clone();
            std::thread::spawn(move || {
                let mut record =
                    provider_record(CsvProvider::TheRundown, key, &format!("-{}", 110 + idx));
                record.row_id = format!("row-{idx}");
                sink.write_provider_record(&record).unwrap().provider_file
            })
        })
        .collect::<Vec<_>>();
    let provider_files = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let provider_file = provider_files.first().unwrap();

    let text = std::fs::read_to_string(provider_file).unwrap();
    assert_eq!(
        text.lines()
            .filter(|line| line
                .starts_with("data_generated_at,data_fetched_at,bookmaker,affiliate_id,team_a_polymarket_format,team_b_polymarket_format"))
            .count(),
        1
    );
    assert_eq!(text.lines().count(), 21);
    let comma_count = text.lines().next().unwrap().matches(',').count();
    assert!(text
        .lines()
        .skip(1)
        .all(|line| line.matches(',').count() == comma_count));
}

#[test]
fn independent_sinks_share_index_without_corrupting_latest_files_json() {
    let temp = tempfile::tempdir().unwrap();
    let sink_a = LocalCsvSink::new(temp.path()).unwrap();
    let sink_b = LocalCsvSink::new(temp.path()).unwrap();
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Detroit Pistons vs Cleveland Cavaliers",
        Some("2026-05-15T23:00:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );

    let handles = (0..40)
        .map(|idx| {
            let sink = if idx % 2 == 0 {
                sink_a.clone()
            } else {
                sink_b.clone()
            };
            let key = key.clone();
            std::thread::spawn(move || {
                let mut record = if idx % 3 == 0 {
                    provider_record(CsvProvider::Polymarket, key, "0.49")
                } else {
                    provider_record(CsvProvider::TheRundown, key, "-175")
                };
                record.row_id = format!("row-{idx}");
                record.payload_hash = format!("sha256:testhash{idx}");
                record
                    .affiliate_id
                    .clone_from(&Some(if idx % 2 == 0 { "19" } else { "23" }.to_string()));
                sink.write_provider_record(&record).unwrap();
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().unwrap();
    }

    let latest_path = temp.path().join("_index/latest_files.json");
    let latest_text = std::fs::read_to_string(&latest_path).unwrap();
    let latest: serde_json::Value = serde_json::from_str(&latest_text).unwrap();
    assert!(latest.as_object().is_some_and(|object| !object.is_empty()));
    assert!(!temp.path().join("_index/.write.lock").exists());
}

#[test]
fn empty_latest_files_json_is_recovered_on_next_write() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    std::fs::write(temp.path().join("_index/latest_files.json"), "").unwrap();
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Detroit Pistons vs Cleveland Cavaliers",
        Some("2026-05-15T23:00:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );

    sink.write_provider_record(&provider_record(CsvProvider::TheRundown, key, "-175"))
        .unwrap();

    let latest_text =
        std::fs::read_to_string(temp.path().join("_index/latest_files.json")).unwrap();
    let latest: serde_json::Value = serde_json::from_str(&latest_text).unwrap();
    assert!(latest.as_object().is_some_and(|object| !object.is_empty()));
}

#[test]
fn provider_csv_rows_are_four_column_polymarket_format_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Los Angeles Lakers vs Golden State Warriors",
        Some("2026-05-16T23:30:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );
    let mut lakers = provider_record(CsvProvider::TheRundown, key.clone(), "-118");
    lakers.outcome_name = "Los Angeles Lakers".to_string();
    let mut warriors = provider_record(CsvProvider::TheRundown, key, "+105");
    warriors.outcome_name = "Golden State Warriors".to_string();

    let first = sink.write_provider_record(&lakers).unwrap();
    let second = sink.write_provider_record(&warriors).unwrap();

    assert!(first.comparison_file.is_none());
    assert_eq!(second.comparison_status, None);
    let text = std::fs::read_to_string(second.provider_file).unwrap();
    assert!(text.starts_with(
        "data_generated_at,data_fetched_at,bookmaker,affiliate_id,team_a_polymarket_format,team_b_polymarket_format"
    ));
    assert!(text.contains("2026-05-15T22:15:12Z,2026-05-15T22:15:13Z,DraftKings,19,0.541284,"));
    assert!(
        text.contains("2026-05-15T22:15:12Z,2026-05-15T22:15:13Z,DraftKings,19,0.541284,0.487805")
    );
    assert!(!text.to_ascii_lowercase().contains("order.intent"));
    assert!(!text.to_ascii_lowercase().contains("signal.event"));
}

#[test]
fn therundown_different_affiliates_write_to_different_bookmaker_folders() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Detroit Pistons vs Cleveland Cavaliers",
        Some("2026-05-15T23:00:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );
    let mut affiliate_19 = provider_record(CsvProvider::TheRundown, key.clone(), "-175");
    affiliate_19.outcome_name = "Cleveland Cavaliers".to_string();
    affiliate_19.affiliate_id = Some("19".to_string());
    let mut affiliate_23 = provider_record(CsvProvider::TheRundown, key, "-172");
    affiliate_23.outcome_name = "Cleveland Cavaliers".to_string();
    affiliate_23.affiliate_id = Some("23".to_string());

    let path_19 = sink
        .write_provider_record(&affiliate_19)
        .unwrap()
        .provider_file;
    let path_23 = sink
        .write_provider_record(&affiliate_23)
        .unwrap()
        .provider_file;

    assert_ne!(path_19, path_23);
    assert!(path_19
        .to_string_lossy()
        .contains("/therundown/nba/draftkings/"));
    assert!(path_23
        .to_string_lossy()
        .contains("/therundown/nba/fanduel/"));
}

#[test]
fn polymarket_csv_uses_discovery_metadata_for_league_time_and_event_filename() {
    let tempdir = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(tempdir.path()).unwrap();
    let mut payload = fixture("polymarket", "market_best_bid_ask.json");
    payload["_local_csv"] = serde_json::json!({
        "sport": "nba",
        "league": "nba",
        "event_name": "Los Angeles Lakers vs Boston Celtics",
        "event_start_time_utc": "2026-05-15T23:00:00Z",
        "market_type": "moneyline",
        "line": null,
        "outcomes_by_token": {
            "pm_asset_yes_mock_001": "Lakers",
            "pm_asset_no_mock_001": "Celtics"
        }
    });
    let raw = raw(Provider::Polymarket, SourceChannel::WsMarket, payload);

    let result = sink.write_raw_message(&raw).unwrap();

    assert_eq!(result.len(), 1);
    assert!(result[0].provider_file.ends_with(
        "polymarket/nba/2026-05-15T230000Z_los_angeles_lakers_vs_boston_celtics_moneyline.csv"
    ));
    let content = std::fs::read_to_string(&result[0].provider_file).unwrap();
    assert!(content.contains("Polymarket,,0.48,0.52"));
}

#[test]
fn therundown_csv_uses_bootstrap_metadata_for_time_and_team_filename() {
    let tempdir = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(tempdir.path()).unwrap();
    let payload = with_therundown_local_csv_metadata(fixture("therundown", "ws_market_price.json"));
    let raw = raw(Provider::TheRundown, SourceChannel::WsMarket, payload);

    let result = sink.write_raw_message(&raw).unwrap();

    assert_eq!(result.len(), 1);
    assert!(result[0].provider_file.ends_with(
        "therundown/nba/draftkings/2026-05-15T233000Z_los_angeles_lakers_vs_boston_celtics_moneyline.csv"
    ));
    let content = std::fs::read_to_string(&result[0].provider_file).unwrap();
    assert!(content
        .contains("2026-05-15T22:15:12Z,2026-05-16T00:00:03Z,DraftKings,19,0.458716,0.541284"));
}

#[test]
fn therundown_rest_bootstrap_writes_current_market_csv_with_team_time_and_league() {
    let tempdir = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(tempdir.path()).unwrap();
    let payload = fixture("therundown", "events_bootstrap.json");
    let raw = raw(Provider::TheRundown, SourceChannel::RestBootstrap, payload);

    let result = sink.write_raw_message(&raw).unwrap();

    assert_eq!(result.len(), 2);
    assert!(result[0].provider_file.ends_with(
        "therundown/nba/draftkings/2026-05-15T233000Z_los_angeles_lakers_vs_boston_celtics_moneyline.csv"
    ));
    assert_eq!(result[0].provider_file, result[1].provider_file);
    let content = std::fs::read_to_string(&result[0].provider_file).unwrap();
    assert!(content.contains(",2026-05-16T00:00:03Z,DraftKings,19,0.454545,0.545455"));
    assert!(content.contains(",2026-05-16T00:00:03Z,DraftKings,19,0.487805,0.545455"));
    assert!(!result[0]
        .provider_file
        .to_string_lossy()
        .contains("unknown"));
}

#[test]
fn therundown_rest_bootstrap_supports_real_nested_lines_prices_shape() {
    let tempdir = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(tempdir.path()).unwrap();
    let payload = serde_json::json!({
        "meta": {"sport_id": 4, "date": "2026-05-15"},
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
                "name": "moneyline",
                "period_id": 0,
                "participants": [
                    {
                        "id": 7,
                        "type": "TYPE_TEAM",
                        "name": "Cleveland Cavaliers",
                        "lines": [{
                            "id": "home-line",
                            "prices": {
                                "23": {"id": "204506997", "price": -172, "price_delta": 4, "is_main_line": true, "updated_at": "2026-05-15T20:50:00Z"}
                            }
                        }]
                    },
                    {
                        "id": 8,
                        "type": "TYPE_TEAM",
                        "name": "Detroit Pistons",
                        "lines": [{
                            "id": "away-line",
                            "prices": {
                                "23": {"id": "204503682", "price": 145, "price_delta": 5, "is_main_line": true, "updated_at": "2026-05-15T14:31:20Z"}
                            }
                        }]
                    }
                ]
            }]
        }]
    });
    let raw = raw(Provider::TheRundown, SourceChannel::RestBootstrap, payload);

    let result = sink.write_raw_message(&raw).unwrap();

    assert_eq!(result.len(), 2);
    assert!(result[0].provider_file.ends_with(
        "therundown/nba/fanduel/2026-05-15T230000Z_detroit_pistons_vs_cleveland_cavaliers_moneyline.csv"
    ));
    let content = std::fs::read_to_string(&result[0].provider_file).unwrap();
    assert!(
        content.contains("2026-05-15T20:50:00Z,2026-05-16T00:00:03Z,FanDuel,23,0.367647,0.632353")
    );
    assert!(
        content.contains("2026-05-15T14:31:20Z,2026-05-16T00:00:03Z,FanDuel,23,0.408163,0.632353")
    );
}

#[test]
fn therundown_heartbeat_and_unenriched_ws_messages_are_skipped_for_local_csv() {
    let tempdir = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(tempdir.path()).unwrap();
    let heartbeat = raw(
        Provider::TheRundown,
        SourceChannel::WsMarket,
        fixture("therundown", "ws_heartbeat.json"),
    );
    let price_without_metadata = raw(
        Provider::TheRundown,
        SourceChannel::WsMarket,
        fixture("therundown", "ws_market_price.json"),
    );

    assert!(records_from_raw(&heartbeat).unwrap().is_empty());
    assert!(records_from_raw(&price_without_metadata)
        .unwrap()
        .is_empty());
    assert!(sink.write_raw_message(&heartbeat).unwrap().is_empty());
    assert!(sink
        .write_raw_message(&price_without_metadata)
        .unwrap()
        .is_empty());
    assert!(!tempdir.path().join("therundown/unknown_league").exists());
}

#[test]
fn polymarket_ws_without_discovery_metadata_is_skipped_for_local_csv() {
    let tempdir = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(tempdir.path()).unwrap();
    let payload = fixture("polymarket", "market_best_bid_ask.json");
    let raw = raw(Provider::Polymarket, SourceChannel::WsMarket, payload);

    assert!(records_from_raw(&raw).unwrap().is_empty());
    assert!(sink.write_raw_message(&raw).unwrap().is_empty());
    assert!(!tempdir.path().join("polymarket/unknown_sport").exists());
}

#[test]
fn generated_and_fetched_times_remain_distinct_and_missing_provider_time_is_flagged() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let mut payload = fixture("therundown", "ws_market_price.json");
    payload["data"]
        .as_object_mut()
        .unwrap()
        .remove("updated_at");
    payload["meta"].as_object_mut().unwrap().remove("timestamp");
    payload = with_therundown_local_csv_metadata(payload);
    let raw = raw(Provider::TheRundown, SourceChannel::WsMarket, payload);
    let records = records_from_raw(&raw).unwrap();

    assert_eq!(records[0].provider_generated_at, None);
    assert_eq!(records[0].fetched_at, "2026-05-16T00:00:03Z");
    assert!(records[0]
        .quality_flags
        .contains(&"missing_provider_generated_at".to_string()));

    let path = sink
        .write_provider_record(&records[0])
        .unwrap()
        .provider_file;
    let text = std::fs::read_to_string(path).unwrap();
    assert!(!text.contains("missing_provider_generated_at"));
    assert!(text.starts_with(
        "data_generated_at,data_fetched_at,bookmaker,affiliate_id,team_a_polymarket_format,team_b_polymarket_format\n,2026-05-16T00:00:03Z,DraftKings,19,"
    ));
}

#[test]
fn fixture_raw_messages_write_single_provider_csvs() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();

    for (provider, channel, fixture_provider, name) in [
        (
            Provider::TheRundown,
            SourceChannel::WsMarket,
            "therundown",
            "ws_market_price.json",
        ),
        (
            Provider::TheRundown,
            SourceChannel::WsMarket,
            "therundown",
            "off_board_price.json",
        ),
        (
            Provider::Polymarket,
            SourceChannel::WsMarket,
            "polymarket",
            "market_book.json",
        ),
        (
            Provider::Polymarket,
            SourceChannel::WsMarket,
            "polymarket",
            "market_price_change.json",
        ),
        (
            Provider::Polymarket,
            SourceChannel::WsMarket,
            "polymarket",
            "market_best_bid_ask.json",
        ),
    ] {
        let mut payload = fixture(fixture_provider, name);
        if provider == Provider::Polymarket {
            payload = with_polymarket_local_csv_metadata(payload);
        } else if provider == Provider::TheRundown {
            payload = with_therundown_local_csv_metadata(payload);
        }
        let raw = raw(provider, channel, payload);
        for record in records_from_raw(&raw).unwrap() {
            let result = sink.write_provider_record(&record).unwrap();
            assert!(result.provider_file.exists());
        }
    }

    let index = std::fs::read_to_string(temp.path().join("_index/markets_index.csv")).unwrap();
    assert!(index.contains("therundown"));
    assert!(index.contains("polymarket"));
    let mut off_board_csv = String::new();
    append_text_files(&temp.path().join("therundown/nba"), &mut off_board_csv);
    assert!(!off_board_csv.contains("off_board"));
}

#[test]
fn probability_helpers_are_observational_only() {
    assert_eq!(
        american_odds_to_implied_probability("-118"),
        Some(0.5412844036697247)
    );
    assert_eq!(
        american_odds_to_implied_probability("+105"),
        Some(0.4878048780487805)
    );
    assert_eq!(market_decimal_mid(Some("0.47"), Some("0.49")), Some(0.48));
    assert_eq!(american_odds_to_implied_probability("0.0001"), None);
}

#[test]
fn csv_output_redacts_secret_like_values() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let key = MarketFileKey::new(
        "nba",
        "nba",
        "api_key=should_not_write Lakers vs Warriors",
        Some("2026-05-16T23:30:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );
    let mut record = provider_record(CsvProvider::TheRundown, key, "-118");
    record.raw_ref = "raw/Authorization/Bearer secret-value".to_string();
    record.trace_id = "private_key=abc123".to_string();

    let result = sink.write_provider_record(&record).unwrap();
    assert!(!result
        .provider_file
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains("api_key"));
    let mut combined = std::fs::read_to_string(result.provider_file).unwrap();
    combined
        .push_str(&std::fs::read_to_string(temp.path().join("_index/markets_index.csv")).unwrap());
    append_text_files(&temp.path().join("_index/latest/therundown"), &mut combined);

    let lower = combined.to_ascii_lowercase();
    assert!(!lower.contains("api_key"));
    assert!(!lower.contains("secret-value"));
    assert!(!lower.contains("private_key"));
    assert!(!lower.contains("authorization"));
}

#[test]
fn abandoned_process_lock_is_removed_on_next_write() {
    let temp = tempfile::tempdir().unwrap();
    let sink = LocalCsvSink::new(temp.path()).unwrap();
    let lock_path = temp.path().join("_index/.write.lock");
    std::fs::write(
        &lock_path,
        "pid=4000000000 acquired_at=2026-05-16T00:00:00Z\n",
    )
    .unwrap();

    let key = MarketFileKey::new(
        "nba",
        "nba",
        "Lakers vs Warriors",
        Some("2026-05-16T23:30:00Z"),
        "moneyline",
        "full_game",
        MarketLine::NoLine,
    );
    let result = sink
        .write_provider_record(&provider_record(CsvProvider::TheRundown, key, "-118"))
        .unwrap();

    assert!(result.provider_file.exists());
    assert!(!lock_path.exists());
}

fn append_text_files(dir: &std::path::Path, output: &mut String) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            append_text_files(&path, output);
        } else {
            output.push_str(&std::fs::read_to_string(path).unwrap());
        }
    }
}
