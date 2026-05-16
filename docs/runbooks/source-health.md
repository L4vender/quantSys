# Source Health Runbook

Phase 5 source health is read-only. It reports adapter health, raw archive lag, rate budget status, and DLQ pressure. It does not gate orders or expose strategy/execution APIs.

## Status Definitions

| Status | Meaning |
|---|---|
| `ok` | Source is current and usable for its phase. |
| `degraded` | Source is reachable but has a warning. |
| `stale` | No message within `stale_after_seconds`. |
| `delayed` / `data_delay_detected` | Provider data is delayed. |
| `no_ws` / `no_websocket_access` | WebSocket entitlement is missing. |
| `geoblocked` / `blocked` | Polymarket geoblock probe blocks live execution. |
| `rate_limited` | Endpoint budget is exhausted or Retry-After is active. |
| `auth_failed` / `auth_missing` | Credentials are rejected or unavailable. |
| `datapoints_exhausted` | TheRundown entitlement counters are exhausted. |
| `schema_error` | Adapter or archive rejected schema. |
| `dlq_spike` | DLQ volume is above alert threshold. |
| `lagging` | Raw archive consumer lag exceeds threshold. |
| `unknown` | No reliable state yet. |

## Source Fields

TheRundown fields include tier, delay seconds, websocket access, datapoint/rate state, last message, heartbeat, stale threshold, and live gates. `live_execution_allowed` is always false.

Polymarket market fields include WS status, discovery status, rate-limited endpoint, stale status, and market resolution markers.

Polymarket user fields include auth missing/failed, user WS state, and redacted credential handling.

Polymarket geoblock and time probes report `geoblocked`, server time warnings, and fail closed for live execution.

## Rate Budgets

Budgets are endpoint-scoped. Required endpoints include TheRundown REST entitlement, events bootstrap, markets delta, WS reconnect, Polymarket discovery, market WS reconnect, user WS reconnect, geoblock, and time probe.

Redis latest key format:

```text
rate_budget:{provider}:{endpoint}
```

Status is `ok`, `exhausted`, `rate_limited`, or `unknown`.

## Consumer Lag

Lag records include topic, consumer group, partition, last consumed offset, high watermark, lag, and updated time.

Redis latest key format:

```text
consumer_lag:{topic}:{consumer_group}
```

If lag exceeds threshold, mark source health `lagging` and check raw-archive logs, object archive write latency, DLQ spikes, and Redpanda health.

## DLQ Spike Handling

1. Query `/api/v1/dlq`.
2. Group by `error_code`.
3. For `secret_scan_failed`, confirm adapters are redacting auth fields.
4. For schema errors, compare payload fixture against adapter contract.
5. For archive/index failures, check object storage and PostgreSQL.
6. Replay only after confirming payloads are sanitized.

## Common Commands

```bash
make raw-archive-test
make source-health-test
make phase5-test
cargo run -p source-health -- --port 8085
curl http://localhost:8085/api/v1/source-health
curl 'http://localhost:8085/api/v1/raw/by-ref?raw_ref=raw/therundown/ws_market/2026/05/16/10/example.json'
curl http://localhost:8085/api/v1/rate-budgets
curl http://localhost:8085/api/v1/consumer-lag
```

## Alerts

Alert on sustained `stale`, `delayed`, `no_ws`, `geoblocked`, `rate_limited`, `dlq_spike`, or `lagging`. For TheRundown delayed/no WS, keep live signal disabled. For Polymarket geoblock, keep live execution disabled.

## Phase 6 Entry Check

Before normalization and mapping, verify raw events archive for TheRundown, Polymarket market, and Polymarket user; `raw_ref` lookup works; duplicates are idempotent; DLQ does not block good messages; source state, rate budget, and consumer lag snapshots are readable.
