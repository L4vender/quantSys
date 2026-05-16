CREATE SCHEMA IF NOT EXISTS archive;

CREATE TABLE IF NOT EXISTS archive.raw_archive_index (
    raw_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    topic TEXT NOT NULL,
    source_channel TEXT NOT NULL,
    provider_message_id TEXT,
    provider_event_id TEXT,
    provider_market_id TEXT,
    payload_hash TEXT NOT NULL,
    raw_ref TEXT NOT NULL UNIQUE,
    schema_version TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    archive_status TEXT NOT NULL,
    payload_size_bytes BIGINT NOT NULL DEFAULT 0,
    quality_flags JSONB NOT NULL DEFAULT '{}'::JSONB,
    duplicate_count BIGINT NOT NULL DEFAULT 0,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS raw_archive_index_payload_hash_idx
    ON archive.raw_archive_index (payload_hash);

CREATE INDEX IF NOT EXISTS raw_archive_index_provider_received_at_idx
    ON archive.raw_archive_index (provider, received_at DESC);

CREATE INDEX IF NOT EXISTS raw_archive_index_provider_event_id_idx
    ON archive.raw_archive_index (provider_event_id);

CREATE INDEX IF NOT EXISTS raw_archive_index_provider_market_id_idx
    ON archive.raw_archive_index (provider_market_id);

CREATE INDEX IF NOT EXISTS raw_archive_index_trace_id_idx
    ON archive.raw_archive_index (trace_id);

CREATE INDEX IF NOT EXISTS raw_archive_index_topic_channel_received_at_idx
    ON archive.raw_archive_index (topic, source_channel, received_at DESC);
