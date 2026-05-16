use chrono::{TimeZone, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel};
use quantsys_eventbus::EventEnvelope;
use quantsys_storage::{
    InMemoryDlqStore, InMemoryObjectArchive, InMemoryRawArchiveIndex, InMemorySourceStateStore,
};
use raw_archive::{RawArchiveProcessor, RawArchiveProcessorConfig};
use serde_json::json;

fn processor() -> RawArchiveProcessor {
    RawArchiveProcessor::new(
        RawArchiveProcessorConfig::default(),
        InMemoryObjectArchive::default(),
        InMemoryRawArchiveIndex::default(),
        InMemoryDlqStore::default(),
        InMemorySourceStateStore::default(),
    )
}

fn raw(provider: Provider, source_channel: SourceChannel, topic: &str) -> EventEnvelope {
    let raw = RawMessage::new(
        provider,
        source_channel,
        Some("msg-1".to_string()),
        Some("event-1".to_string()),
        Some("market-1".to_string()),
        Utc.with_ymd_and_hms(2026, 5, 16, 10, 0, 0).unwrap(),
        123,
        String::new(),
        "phase5.mock.v1".to_string(),
        json!({"fixture": true, "event_id": "event-1"}),
    );
    EventEnvelope {
        topic: topic.to_string(),
        key: raw.raw_id.as_bytes().to_vec(),
        payload: serde_json::to_vec(&raw).unwrap(),
        offset: 1,
        partition: 0,
    }
}

#[tokio::test]
async fn therundown_raw_topic_archives_and_reads_back_by_ref() {
    let processor = processor();
    let result = processor
        .process_envelope(raw(
            Provider::TheRundown,
            SourceChannel::WsMarket,
            "raw.therundown",
        ))
        .await
        .unwrap();

    let read = processor.read_by_ref(&result.raw_ref).unwrap();
    assert_eq!(read.payload["fixture"], true);
    assert!(processor.index().get(&result.raw_id).is_some());
}

#[tokio::test]
async fn polymarket_market_and_user_topics_archive() {
    let processor = processor();

    let market = processor
        .process_envelope(raw(
            Provider::Polymarket,
            SourceChannel::WsMarket,
            "raw.polymarket.market",
        ))
        .await
        .unwrap();
    let user = processor
        .process_envelope(raw(
            Provider::Polymarket,
            SourceChannel::WsUser,
            "raw.polymarket.user",
        ))
        .await
        .unwrap();

    assert!(market.raw_ref.contains("raw/polymarket/ws_market"));
    assert!(user.raw_ref.contains("raw/polymarket/ws_user"));
}

#[tokio::test]
async fn duplicate_raw_event_is_idempotent_and_bad_payload_does_not_block_good_payload() {
    let processor = processor();
    let good = raw(
        Provider::TheRundown,
        SourceChannel::WsMarket,
        "raw.therundown",
    );
    let bad = EventEnvelope {
        topic: "raw.therundown".to_string(),
        key: b"bad".to_vec(),
        payload: b"{not json".to_vec(),
        offset: 2,
        partition: 0,
    };

    let first = processor.process_envelope(good.clone()).await.unwrap();
    let duplicate = processor.process_envelope(good).await.unwrap();
    let bad_result = processor.process_envelope(bad).await;
    let after_bad = processor
        .process_envelope(raw(
            Provider::TheRundown,
            SourceChannel::WsMarket,
            "raw.therundown",
        ))
        .await
        .unwrap();

    assert_eq!(first.raw_id, duplicate.raw_id);
    assert!(duplicate.duplicate);
    assert!(bad_result.is_err());
    assert_eq!(processor.dlq().list().len(), 1);
    assert!(processor.index().get(&after_bad.raw_id).is_some());
}

#[tokio::test]
async fn mock_archive_sustains_one_thousand_messages_per_second() {
    let processor = processor();
    let started = std::time::Instant::now();

    for idx in 0..1_000 {
        let mut event = raw(
            Provider::Polymarket,
            SourceChannel::WsMarket,
            "raw.polymarket.market",
        );
        let mut raw: RawMessage = serde_json::from_slice(&event.payload).unwrap();
        raw.provider_message_id = Some(format!("msg-{idx}"));
        raw.payload = json!({"fixture": true, "idx": idx});
        raw.payload_hash = quantsys_domain::compute_payload_hash(&raw.payload);
        raw.raw_id = quantsys_domain::compute_raw_id(
            &raw.provider,
            &raw.source_channel,
            raw.provider_message_id.as_deref(),
            raw.provider_event_id.as_deref(),
            raw.provider_market_id.as_deref(),
            &raw.payload_hash,
        );
        event.payload = serde_json::to_vec(&raw).unwrap();
        processor.process_envelope(event).await.unwrap();
    }

    let elapsed = started.elapsed();
    assert_eq!(processor.index().len(), 1_000);
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "in-memory raw archive smoke was slower than 1k msg/s: {elapsed:?}"
    );
}
