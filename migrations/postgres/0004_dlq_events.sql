CREATE SCHEMA IF NOT EXISTS archive;

CREATE TABLE IF NOT EXISTS archive.dlq_events (
    dlq_id TEXT PRIMARY KEY,
    raw_id TEXT,
    provider TEXT NOT NULL,
    topic TEXT NOT NULL,
    source_channel TEXT NOT NULL,
    error_code TEXT NOT NULL,
    error_message TEXT NOT NULL,
    error_category TEXT NOT NULL,
    payload_hash TEXT,
    raw_ref TEXT,
    dlq_ref TEXT NOT NULL,
    trace_id TEXT,
    received_at TIMESTAMPTZ NOT NULL,
    failed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    retryable BOOLEAN NOT NULL DEFAULT FALSE,
    replay_metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS dlq_events_provider_failed_at_idx
    ON archive.dlq_events (provider, failed_at DESC);

CREATE INDEX IF NOT EXISTS dlq_events_error_code_idx
    ON archive.dlq_events (error_code);

CREATE INDEX IF NOT EXISTS dlq_events_payload_hash_idx
    ON archive.dlq_events (payload_hash);

CREATE INDEX IF NOT EXISTS dlq_events_retryable_idx
    ON archive.dlq_events (retryable, failed_at DESC);
