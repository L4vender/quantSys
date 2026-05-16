use chrono::{Duration, TimeZone, Utc};
use quantsys_domain::{
    Provider, RawArchiveStatus, RawMessage, SourceChannel, SourceMode, SourceStatus,
};
use quantsys_storage::{
    ArchiveReadRequest, ArchiveWriteRequest, ConsumerLagSnapshot, InMemoryConsumerLagStore,
    InMemoryDlqStore, InMemoryObjectArchive, InMemoryRateBudgetStore, InMemoryRawArchiveIndex,
    InMemorySourceStateStore, ObjectArchive, ObjectKeyBuilder, RateBudgetSnapshot,
    RateBudgetStatus, RawArchiveIndexRecord, RawArchiveSearchQuery, RedisKeyBuilder,
};
use serde_json::json;
use tempfile::tempdir;

fn raw_message(raw_id_suffix: &str) -> RawMessage {
    RawMessage::new(
        Provider::TheRundown,
        SourceChannel::WsMarket,
        Some(format!("message-{raw_id_suffix}")),
        Some("event-1".to_string()),
        Some("market-1".to_string()),
        Utc.with_ymd_and_hms(2026, 5, 16, 9, 3, 0).unwrap(),
        100,
        String::new(),
        "therundown.v2.ws_market_price.mock.v1".to_string(),
        json!({"meta": {"type": "market_price"}, "data": {"id": raw_id_suffix}}),
    )
}

#[test]
fn object_key_builder_uses_phase5_raw_and_dlq_layout() {
    let builder = ObjectKeyBuilder::new("");
    let ts = Utc.with_ymd_and_hms(2026, 5, 16, 9, 3, 0).unwrap();

    assert_eq!(
        builder.raw_archive_key("therundown", "ws_market", "raw-1", ts),
        "raw/therundown/ws_market/2026/05/16/09/raw-1.json"
    );
    assert_eq!(
        builder.dlq_archive_key("polymarket", "ws_user", "raw-2", ts),
        "dlq/polymarket/ws_user/2026/05/16/09/raw-2.json"
    );
    let sanitized = builder.raw_archive_key("poly/secret", "ws_user", "raw-2", ts);
    assert!(sanitized.contains("poly_secret"));
    assert!(!sanitized.contains("poly/secret"));
}

#[test]
fn object_archive_memory_and_local_backends_roundtrip_by_raw_ref() {
    let memory = InMemoryObjectArchive::default();
    let key = "raw/therundown/ws_market/2026/05/16/09/raw-1.json";
    let body = br#"{"ok":true}"#.to_vec();

    let write = memory
        .write(ArchiveWriteRequest::json(key, body.clone(), "raw-1"))
        .unwrap();
    let read = memory
        .read(ArchiveReadRequest::by_ref(write.raw_ref.clone()))
        .unwrap();
    assert_eq!(read.bytes, body);

    let temp = tempdir().unwrap();
    let local = quantsys_storage::LocalFilesystemObjectArchive::new(temp.path()).unwrap();
    let local_write = local
        .write(ArchiveWriteRequest::json(key, body.clone(), "raw-1"))
        .unwrap();
    let local_read = local
        .read(ArchiveReadRequest::by_ref(local_write.raw_ref))
        .unwrap();
    assert_eq!(local_read.bytes, body);
}

#[test]
fn object_archive_is_idempotent_and_reports_batch_partial_failures() {
    let archive = InMemoryObjectArchive::default();
    let key = "raw/therundown/ws_market/2026/05/16/09/raw-1.json";

    let first = archive
        .write(ArchiveWriteRequest::json(
            key,
            br#"{"ok":true}"#.to_vec(),
            "raw-1",
        ))
        .unwrap();
    let second = archive
        .write(ArchiveWriteRequest::json(
            key,
            br#"{"ok":true}"#.to_vec(),
            "raw-1",
        ))
        .unwrap();
    assert!(!first.duplicate);
    assert!(second.duplicate);

    let results = archive.write_batch(vec![
        ArchiveWriteRequest::json(
            "raw/therundown/ws_market/2026/05/16/09/raw-2.json",
            br#"{"ok":true}"#.to_vec(),
            "raw-2",
        ),
        ArchiveWriteRequest::json(
            "raw/therundown/ws_market/2026/05/16/09/secret-key.json",
            br#"{"ok":true}"#.to_vec(),
            "raw-3",
        ),
    ]);

    assert_eq!(results.len(), 2);
    assert!(results[0].as_ref().unwrap().raw_ref.ends_with("raw-2.json"));
    assert!(results[1].is_err());
}

#[test]
fn raw_archive_index_upserts_duplicates_and_searches_metadata() {
    let index = InMemoryRawArchiveIndex::default();
    let mut raw = raw_message("1");
    raw.raw_ref = "raw/therundown/ws_market/2026/05/16/09/raw-1.json".to_string();

    let inserted = index.upsert(RawArchiveIndexRecord::from_raw(
        &raw,
        RawArchiveStatus::Archived,
    ));
    let duplicate = index.upsert(RawArchiveIndexRecord::from_raw(
        &raw,
        RawArchiveStatus::Duplicate,
    ));
    assert!(!inserted.duplicate);
    assert!(duplicate.duplicate);

    let by_ref = index.get_by_raw_ref(&raw.raw_ref).unwrap();
    assert_eq!(by_ref.raw_id, raw.raw_id);
    assert_eq!(by_ref.duplicate_count, 1);

    let search = index.search(RawArchiveSearchQuery {
        provider: Some(Provider::TheRundown),
        topic: Some("raw.therundown".to_string()),
        from: Some(raw.received_at - Duration::minutes(1)),
        to: Some(raw.received_at + Duration::minutes(1)),
        ..RawArchiveSearchQuery::default()
    });
    assert_eq!(search.len(), 1);
}

#[test]
fn source_state_rate_budget_and_consumer_lag_stores_keep_latest_snapshots() {
    let source_store = InMemorySourceStateStore::default();
    let mut state = quantsys_domain::SourceState {
        source: "therundown".to_string(),
        mode: SourceMode::LiveWs,
        tier: Some("ultra".to_string()),
        data_delay_seconds: Some(0),
        websocket_access: Some(true),
        status: SourceStatus::Ok,
        last_message_at: Some(Utc.with_ymd_and_hms(2026, 5, 16, 9, 3, 0).unwrap()),
        last_heartbeat_at: None,
        stale_after_seconds: 30,
        rate_limited: false,
        geoblocked: false,
        error: None,
        live_signal_allowed: true,
        live_execution_allowed: false,
        block_reason: None,
    };
    source_store.update("therundown", Provider::TheRundown, state.clone());
    assert_eq!(
        source_store.latest("therundown").unwrap().state.status,
        SourceStatus::Ok
    );
    state.status = SourceStatus::Stale;
    source_store.update("therundown", Provider::TheRundown, state);
    assert_eq!(source_store.history("therundown").len(), 2);
    assert_eq!(
        source_store
            .redis_latest_health(&RedisKeyBuilder::new("quantsys"), "therundown")
            .unwrap()
            .0,
        "quantsys:source_health:therundown"
    );

    let budget_store = InMemoryRateBudgetStore::default();
    let budget = RateBudgetSnapshot::new(
        Provider::Polymarket,
        "market_ws_reconnect",
        Some(10),
        Some(0),
        None,
        Some(60),
        Utc.with_ymd_and_hms(2026, 5, 16, 9, 4, 0).unwrap(),
    );
    budget_store.update(budget.clone());
    assert_eq!(budget.status, RateBudgetStatus::Exhausted);
    assert!(budget.retry_after_until().is_some());
    assert_eq!(
        budget_store
            .redis_latest(&RedisKeyBuilder::new("quantsys"))
            .len(),
        1
    );

    let lag_store = InMemoryConsumerLagStore::default();
    lag_store.update(ConsumerLagSnapshot::new(
        "raw.therundown",
        "raw-archive",
        0,
        90,
        100,
        Utc.with_ymd_and_hms(2026, 5, 16, 9, 5, 0).unwrap(),
    ));
    assert!(lag_store.latest_for_topic("raw.therundown")[0].is_lagging(5));
    assert_eq!(
        lag_store
            .redis_latest(&RedisKeyBuilder::new("quantsys"))
            .len(),
        1
    );
}

#[test]
fn dlq_store_and_redis_key_builder_cover_phase5_keys() {
    let keys = RedisKeyBuilder::new("quantsys");
    assert_eq!(
        keys.source_health("therundown"),
        "quantsys:source_health:therundown"
    );
    assert_eq!(
        keys.rate_budget("polymarket", "time_probe"),
        "quantsys:rate_budget:polymarket:time_probe"
    );
    assert_eq!(
        keys.consumer_lag("raw.therundown", "raw-archive"),
        "quantsys:consumer_lag:raw.therundown:raw-archive"
    );

    let dlq = InMemoryDlqStore::default();
    let event = quantsys_domain::DlqEvent::new(
        None,
        Provider::TheRundown,
        "raw.therundown".to_string(),
        SourceChannel::WsMarket,
        quantsys_domain::DlqErrorCode::MalformedJson,
        quantsys_domain::DlqErrorCategory::Validation,
        "malformed json",
        None,
        None,
        "dlq/therundown/ws_market/2026/05/16/09/unknown.json".to_string(),
        None,
        Utc.with_ymd_and_hms(2026, 5, 16, 9, 6, 0).unwrap(),
        false,
    );
    dlq.insert(event.clone());
    assert_eq!(dlq.list().len(), 1);
    assert_eq!(dlq.list()[0].dlq_id, event.dlq_id);
}
