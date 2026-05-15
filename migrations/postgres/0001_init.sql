CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;

CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS source;
CREATE SCHEMA IF NOT EXISTS archive;
CREATE SCHEMA IF NOT EXISTS eventbus;
CREATE SCHEMA IF NOT EXISTS ops;

CREATE TABLE IF NOT EXISTS core.data_sources (
    source_name TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    mode TEXT NOT NULL DEFAULT 'mock',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS source.source_states (
    source_name TEXT PRIMARY KEY REFERENCES core.data_sources(source_name),
    status TEXT NOT NULL,
    last_message_at TIMESTAMPTZ,
    last_heartbeat_at TIMESTAMPTZ,
    stale_after_seconds INTEGER NOT NULL DEFAULT 30,
    live_signal_allowed BOOLEAN NOT NULL DEFAULT FALSE,
    live_execution_allowed BOOLEAN NOT NULL DEFAULT FALSE,
    block_reason TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS archive.raw_messages (
    raw_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    source_channel TEXT NOT NULL,
    provider_message_id TEXT,
    provider_event_id TEXT,
    provider_market_id TEXT,
    received_at TIMESTAMPTZ NOT NULL,
    payload_hash TEXT NOT NULL,
    raw_ref TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    trace_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS raw_messages_provider_received_at_idx
    ON archive.raw_messages (provider, received_at DESC);

CREATE TABLE IF NOT EXISTS eventbus.topic_catalog (
    topic_name TEXT PRIMARY KEY,
    topic_key TEXT NOT NULL,
    producer TEXT NOT NULL,
    consumers TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    partitions INTEGER NOT NULL,
    replicas INTEGER NOT NULL,
    retention_days INTEGER NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS ops.worker_heartbeats (
    service_name TEXT NOT NULL,
    instance_id TEXT NOT NULL,
    heartbeat_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    PRIMARY KEY (service_name, instance_id)
);

INSERT INTO core.data_sources (source_name, provider, enabled, mode)
VALUES
    ('therundown', 'therundown', FALSE, 'mock'),
    ('polymarket_market', 'polymarket', FALSE, 'mock'),
    ('polymarket_user', 'polymarket', FALSE, 'mock'),
    ('polymarket_geoblock', 'polymarket', FALSE, 'mock')
ON CONFLICT (source_name) DO NOTHING;
