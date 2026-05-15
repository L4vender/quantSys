use chrono::{TimeZone, Utc};
use quantsys_storage::{ObjectKeyBuilder, RedisKeyBuilder, StorageConfig};

#[test]
fn redis_key_builder_uses_stable_namespaced_keys() {
    let keys = RedisKeyBuilder::new("quantsys");

    assert_eq!(
        keys.latest_quote("polymarket", "nba:lal-bos:full_game:moneyline:home"),
        "quantsys:latest:quote:polymarket:nba:lal-bos:full_game:moneyline:home"
    );
    assert_eq!(
        keys.worker_heartbeat("api-gateway", "local-1"),
        "quantsys:worker:heartbeat:api-gateway:local-1"
    );
}

#[test]
fn object_key_builder_partitions_raw_payloads_by_date_provider_and_channel() {
    let builder = ObjectKeyBuilder::new("raw");
    let ts = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();

    let key = builder.raw_payload("therundown", "ws_market", "event-1", "raw-1", ts);

    assert_eq!(
        key,
        "raw/2026/05/15/therundown/ws_market/event-1/raw-1.json"
    );
}

#[test]
fn storage_config_has_local_compose_defaults() {
    let config = StorageConfig::local_compose();

    assert!(config.postgres.url.contains("postgres"));
    assert!(config.clickhouse.url.contains("8123"));
    assert!(config.redis.url.contains("redis://"));
    assert_eq!(config.object_storage.bucket, "quantsys-raw");
}
