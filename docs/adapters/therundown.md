# TheRundown Adapter

## Phase 3 Scope

Phase 3 implements TheRundown V2 ingestion only. The adapter performs REST entitlement/probe, REST events bootstrap, REST markets delta, V2 WebSocket parsing, raw wrapping, mock eventbus publishing to `raw.therundown`, DLQ-style structured errors, cursor maintenance, and `SourceState` updates.

It does not implement Polymarket ingestion, event mapping, normalized quote conversion, odds conversion, no-vig, edge, strategy, risk, paper broker, signer, real orders, or frontend trading UI. TheRundown remains a data source only and never becomes an execution venue.

## Architecture

Core implementation lives in `crates/source-sdk/src/therundown/`:

| Module | Responsibility |
|---|---|
| `rest.rs` | URL construction, `X-TheRundown-Key` auth, REST transport, status handling for 401/429/5xx/cursor stale. |
| `headers.rs` | Entitlement, rate limit, datapoint, data delay, websocket access, and `Retry-After` parsing. |
| `parser.rs` | `meta.type` dispatch, required-field validation, off-board marker, deterministic payload hash, `RawMessage` construction. |
| `cursor.rs` | Bootstrap `meta.delta_last_id`, delta `next_last_id`, stale cursor decision. |
| `subscription.rs` | WS query-key URL construction and subscription filter validation. |
| `ws.rs` | Exponential reconnect backoff with bounded jitter. |
| `state.rs` | TheRundown `SourceState` gates and fail-closed status transitions. |
| `publisher.rs` | `RawMessage` publish to `raw.therundown` via mock eventbus and in-memory DLQ sink. |

`services/adapter-therundown` provides the CLI and health endpoints. Phase 3 uses `InMemoryEventProducer` as the mock eventbus producer; Phase 5 can replace it with a Redpanda producer and raw archive sink without changing raw/parser contracts.

## REST Bootstrap

Command:

```bash
cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode bootstrap --date today --sport-id 4
```

Flow:

1. Load config and resolve the API key from `auth_env`.
2. Call `/sports/{sport_id}/events/{date}` with `X-TheRundown-Key`.
3. Parse entitlement headers.
4. Preserve the full REST JSON as a `RawMessage` with `source_channel=rest_bootstrap`.
5. Extract canonical event id from the first event for raw keying.
6. Record `meta.delta_last_id` in `DeltaCursor` when present.
7. Publish to `raw.therundown` through the mock producer.
8. Update `SourceState`.

If `meta.delta_last_id` is absent, raw publish is still allowed, but cursor state is incomplete and later delta recovery must bootstrap again.

## Markets Delta

Command:

```bash
cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode delta --last-id tr_delta_20260515_000001
```

Flow:

1. Call `/markets/delta?last_id=...`.
2. Wrap the delta response as `RawMessage` with `source_channel=rest_delta`.
3. Use `meta.next_last_id` or `meta.last_id` to advance the cursor.
4. Publish to `raw.therundown`.
5. On cursor stale/rejected status, mark `SourceState.cursor_stale` and run bootstrap recovery when the recovery helper is used.

No normalized quote, odds conversion, no-vig, signal, or mapping is produced in Phase 3.

## WebSocket

Command:

```bash
cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode ws
```

The adapter builds `wss://.../markets?key=<secret>&sport_ids=...&market_ids=...&affiliate_ids=...&event_ids=...`. Logs and display paths redact `key`. Production config requires at least one of `sport_ids`, `market_ids`, `affiliate_ids`, or `event_ids` so the adapter does not rely on the provider client buffer under broad subscriptions.

The WS handler parses and enqueues only:

- `meta.type=market_price`: validate required `data.*` fields, wrap as `RawMessage`, publish to `raw.therundown`, update last message time.
- `meta.type=heartbeat`: wrap raw, publish, update last heartbeat/message time.
- unknown `meta.type`: preserve raw, publish with `unknown_schema`, mark schema error state.

Missing required fields create a DLQ record and do not manufacture ids or prices.

## 256-Message Buffer Risk

The config field `subscription_filters_required=true` makes empty filter sets fail fast. Tests may use mock filters, but production mode must subscribe narrowly. The WS loop uses stale detection and backoff to avoid tight reconnect loops.

## Headers And Entitlement

Parsed headers:

- `X-Tier`
- `X-Rate-Limit`
- `X-Data-Delay-Seconds`
- `X-Websocket-Access`
- `X-Datapoints`
- `X-Datapoints-Breakdown`
- `X-Datapoints-Limit`
- `X-Datapoints-Period`
- `X-Datapoints-Remaining`
- `X-Datapoints-Reset`
- `X-Datapoints-Used`
- `Retry-After`

`Retry-After` supports both seconds and HTTP-date values.

## SourceState

TheRundown states currently covered:

- `ok`
- `degraded`
- `stale`
- `rate_limited`
- `auth_failed`
- `data_delay_detected`
- `no_websocket_access`
- `datapoints_exhausted`
- `cursor_stale`
- `schema_error`

Rules:

- `X-Data-Delay-Seconds` missing, unknown, or greater than zero disables live signal.
- `X-Websocket-Access` missing, unknown, or false disables live signal.
- `X-Datapoints-Remaining=0` disables live signal.
- stale source disables live signal.
- TheRundown always sets `live_execution_allowed=false`.

## raw.therundown Schema

The adapter publishes `RawMessage`:

| Field | TheRundown value |
|---|---|
| `provider` | `therundown` |
| `source_channel` | `rest_bootstrap`, `rest_delta`, or `ws_market` |
| `provider_message_id` | `data.id`, heartbeat timestamp, delta id, or bootstrap cursor when available |
| `provider_event_id` | TheRundown canonical `event_id` when present |
| `provider_market_id` | TheRundown `market_id` when present |
| `payload_hash` | deterministic SHA-256 over JSON payload |
| `raw_ref` | deterministic raw/DLQ reference path |
| `payload` | raw JSON payload with no API key or auth headers |

`raw_id` is deterministic from provider, channel, provider ids, and payload hash.

## Error Handling

- `401`: mark `auth_failed`, do not retry in a tight loop.
- `429`: parse `Retry-After`, mark `rate_limited`, pause the affected endpoint.
- `5xx` and network timeout: mark degraded and use exponential backoff with bounded jitter.
- stale WS: mark stale and reconnect with backoff.
- malformed JSON or missing required fields: publish structured DLQ record through `InMemoryDlqSink`.
- unknown fields: tolerated.
- unknown `meta.type`: raw is preserved and state is marked `schema_error`.

The DLQ record includes `error_code`, `error_message`, `provider`, `source_channel`, `payload_hash`, `raw_ref`, `received_at`, `schema_version`, and `trace_id`. It contains no secret. Phase 5 should attach this to raw archive / durable DLQ storage.

## Off-Board Sentinel

The parser marks `price=0.0001` as `quality_flags.off_board` on the parsed raw wrapper. Phase 3 does not convert odds or calculate probability, so the sentinel never enters normalized probability, no-vig, edge, signal, risk, or execution paths.

## Secret Scrub

Secrets are read only from the configured env var. `ApiKey` Debug and Display print `<redacted>`. URL/query and header scrubbers remove `key=...`, `X-TheRundown-Key`, and known secret values. Raw payloads never include auth headers or websocket query strings.

## Mock Tests

Run:

```bash
make therundown-test
make therundown-integration-test
```

Mock coverage includes:

- REST entitlement / bootstrap 200.
- REST markets delta 200.
- REST 401.
- REST 429 with `Retry-After`.
- REST 5xx.
- data delay.
- websocket access false.
- datapoints remaining zero.
- WS heartbeat.
- WS market price.
- WS unknown `meta.type`.
- WS missing required field.
- WS stale/backoff.
- cursor stale bootstrap recovery.
- fixture replay for `ws_market_price.json` and `off_board_price.json`.
- 1k message raw publish smoke path.

## Health And Metrics

The service supports:

```bash
cargo run -p adapter-therundown -- --mode health --health-bind 0.0.0.0:8093
```

Endpoints:

- `/health/live`
- `/health/ready`
- `/metrics`

## Live Probe

Run only outside CI:

```bash
make therundown-live-probe
```

The command reads `.env` or environment variables, requires `THERUNDON_API_KEY` per the current example config, prints only a sanitized entitlement summary, and does not write real payloads to fixtures.

## Phase Boundaries

Phase 4 can build Polymarket ingestion beside this adapter. Phase 5 should replace mock publishing with durable Redpanda/raw archive/DLQ wiring. Phase 6 can consume `raw.therundown` for normalization and mapping. Phase 8 dry-run, risk, paper, and any live execution remain blocked until their own phases pass.
