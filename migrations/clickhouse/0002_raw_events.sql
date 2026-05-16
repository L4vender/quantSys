CREATE TABLE IF NOT EXISTS quantsys.raw_events
(
    raw_id String,
    provider LowCardinality(String),
    topic LowCardinality(String),
    source_channel LowCardinality(String),
    provider_event_id Nullable(String),
    provider_market_id Nullable(String),
    payload_hash String,
    raw_ref String,
    schema_version LowCardinality(String),
    received_at DateTime64(3, 'UTC'),
    archived_at DateTime64(3, 'UTC'),
    archive_status LowCardinality(String),
    payload_size_bytes UInt64
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(received_at)
ORDER BY (provider, topic, source_channel, received_at, raw_id);
