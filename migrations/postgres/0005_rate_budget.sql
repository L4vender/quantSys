CREATE SCHEMA IF NOT EXISTS source;

CREATE TABLE IF NOT EXISTS source.rate_budget_snapshots (
    provider TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    limit_value BIGINT,
    remaining BIGINT,
    reset_at TIMESTAMPTZ,
    retry_after_seconds BIGINT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    status TEXT NOT NULL,
    PRIMARY KEY (provider, endpoint)
);

CREATE TABLE IF NOT EXISTS source.rate_budget_audit (
    id BIGSERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    limit_value BIGINT,
    remaining BIGINT,
    reset_at TIMESTAMPTZ,
    retry_after_seconds BIGINT,
    updated_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS rate_budget_audit_provider_endpoint_created_idx
    ON source.rate_budget_audit (provider, endpoint, created_at DESC);
