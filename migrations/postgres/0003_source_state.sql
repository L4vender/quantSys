CREATE SCHEMA IF NOT EXISTS source;

ALTER TABLE source.source_states
    ADD COLUMN IF NOT EXISTS provider TEXT,
    ADD COLUMN IF NOT EXISTS mode TEXT,
    ADD COLUMN IF NOT EXISTS tier TEXT,
    ADD COLUMN IF NOT EXISTS data_delay_seconds BIGINT,
    ADD COLUMN IF NOT EXISTS websocket_access BOOLEAN,
    ADD COLUMN IF NOT EXISTS geoblocked BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS rate_limited BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS error_code TEXT,
    ADD COLUMN IF NOT EXISTS error_message TEXT;

CREATE TABLE IF NOT EXISTS source.source_state_history (
    id BIGSERIAL PRIMARY KEY,
    source_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    mode TEXT NOT NULL,
    status TEXT NOT NULL,
    tier TEXT,
    data_delay_seconds BIGINT,
    websocket_access BOOLEAN,
    geoblocked BOOLEAN NOT NULL DEFAULT FALSE,
    rate_limited BOOLEAN NOT NULL DEFAULT FALSE,
    last_message_at TIMESTAMPTZ,
    last_heartbeat_at TIMESTAMPTZ,
    stale_after_seconds BIGINT NOT NULL DEFAULT 30,
    block_reason TEXT,
    error_code TEXT,
    error_message TEXT,
    live_signal_allowed BOOLEAN NOT NULL DEFAULT FALSE,
    live_execution_allowed BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS source_state_history_source_updated_idx
    ON source.source_state_history (source_name, updated_at DESC);

INSERT INTO core.data_sources (source_name, provider, enabled, mode)
VALUES
    ('polymarket_time', 'polymarket', FALSE, 'mock'),
    ('raw_archive', 'internal', TRUE, 'mock')
ON CONFLICT (source_name) DO NOTHING;
