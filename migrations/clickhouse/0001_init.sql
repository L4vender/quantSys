CREATE DATABASE IF NOT EXISTS quantsys;

CREATE TABLE IF NOT EXISTS quantsys.normalized_quote
(
    quote_id String,
    provider LowCardinality(String),
    canonical_market_key Nullable(String),
    provider_event_id Nullable(String),
    provider_market_id Nullable(String),
    market_type LowCardinality(String),
    period LowCardinality(String),
    side Nullable(String),
    raw_price Nullable(String),
    normalized_probability Nullable(String),
    best_bid Nullable(String),
    best_ask Nullable(String),
    ingest_ts DateTime64(3, 'UTC'),
    raw_ref String,
    quality_flags String
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(ingest_ts)
ORDER BY (provider, ifNull(canonical_market_key, ''), ingest_ts);

CREATE TABLE IF NOT EXISTS quantsys.latency_sample
(
    sample_id String,
    provider LowCardinality(String),
    canonical_market_key String,
    source_age_ms Int64,
    lead_ms Nullable(Int64),
    method LowCardinality(String),
    observed_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(observed_at)
ORDER BY (canonical_market_key, observed_at);
