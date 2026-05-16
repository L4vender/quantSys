CREATE SCHEMA IF NOT EXISTS eventbus;

CREATE TABLE IF NOT EXISTS eventbus.consumer_lag (
    topic TEXT NOT NULL,
    consumer_group TEXT NOT NULL,
    partition INTEGER NOT NULL,
    last_consumed_offset BIGINT NOT NULL,
    high_watermark BIGINT NOT NULL,
    lag BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (topic, consumer_group, partition)
);

CREATE INDEX IF NOT EXISTS consumer_lag_topic_group_idx
    ON eventbus.consumer_lag (topic, consumer_group);
