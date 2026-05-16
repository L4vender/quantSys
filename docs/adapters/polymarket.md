# Polymarket Adapter

## Phase 4 Scope

Phase 4 implements Polymarket data ingestion only. The business outputs are `raw.polymarket.market`, `raw.polymarket.user`, and Polymarket `SourceState`.

It does not implement strategy, risk, paper broker, event mapping, normalized quote conversion, signer, signed orders, execution gateway, real order submission, or a frontend trading page. TheRundown remains an external odds/events source; Polymarket CLOB remains the only future execution venue.

## Architecture

Core implementation lives in `crates/source-sdk/src/polymarket/`:

| Module | Responsibility |
|---|---|
| `discovery.rs` | Gamma events URL construction, mock/reqwest REST transport, active/closed/sports discovery parsing. |
| `subscription.rs` | Market and user subscription payload construction. Market subscriptions use `assets_ids`; `asset_ids` is rejected. |
| `token_cache.rs` | `condition_id -> token_ids`, `token_id -> condition_id`, `token_id -> outcome`, `slug -> condition_id`, cache version and TTL. |
| `parser.rs` | Market/user WS event dispatch, required-field validation, deterministic raw wrapping. |
| `geoblock.rs` | Geoblock response parsing and IP redaction. |
| `time_probe.rs` | Server time parsing and local offset calculation. |
| `state.rs` | Polymarket market/user/geoblock/time `SourceState` transitions. |
| `publisher.rs` | In-memory raw publisher and DLQ-style structured error sink. |
| `market_ws.rs` | Reconnect backoff helpers used by market and user services. |
| `error.rs` | Polymarket errors, L2 credential redaction, secret JSON scrubber. |

Service binaries:

| Service | Modes |
|---|---|
| `services/adapter-polymarket-market` | `discovery`, `market-ws`, `geoblock`, `time-probe`, `health` |
| `services/adapter-polymarket-user` | `user-ws`, `auth-check`, `health` |

Both services expose `/health/live`, `/health/ready`, and `/metrics`. Phase 4 uses `InMemoryEventProducer`; Phase 5 can replace this with durable Redpanda/raw archive wiring.

## Market Discovery

Discovery uses the public Gamma events flow. For the local observation/mapping workflow the default config targets the Polymarket Games tag so the adapter starts from event-level sports markets instead of broad futures/awards markets:

```text
GET {gamma_api_base_url}/events?active=true&closed=false&limit=100&offset=0&tag_id=100639
```

The parser accepts an array payload, `data[]`, or `events[]`. It filters `active=true`, `closed=false`, sports-related events, and the configured P0 market types `moneyline`, `spread`, and `total`. The default sports allowlist covers NBA, NFL, MLB, NHL, ATP, WTA, tennis, and soccer. It extracts Polymarket event id, event title, market title, slug, condition id, CLOB token ids, outcomes, start time when present, line, market type, and market status.

The event id is discovery metadata. The market WS is not subscribed with the event id. Discovery builds the token cache, then the market WS subscribes with the discovered token ids in `assets_ids`.

Discovery publishes the original external payload as `RawMessage` with:

| Field | Value |
|---|---|
| `provider` | `polymarket` |
| `source_channel` | `rest_discovery` |
| `topic` | `raw.polymarket.market` |

Missing critical fields such as `conditionId` or `clobTokenIds` produce structured schema errors rather than fabricated ids.

## Condition/Token Cache

`TokenCache` stores:

| Lookup | Purpose |
|---|---|
| `condition_id -> token_ids` | Build market WS `assets_ids` subscriptions. |
| `token_id -> condition_id` | Attribute market updates to a condition. |
| `token_id -> outcome name` | Preserve raw outcome identity for later Phase 6 mapping. |
| `market_slug -> condition_id` | Stable discovery lookup. |
| `event_id -> condition_ids` | Keep event-level grouping for local observation and future Phase 6 mapping. |

The cache records a version, source label, update timestamp, and TTL. The example config sets `token_cache_ttl_seconds = 300` and `max_token_subscriptions = 1000`.

## Market WebSocket

Market WS endpoint:

```text
wss://ws-subscriptions-clob.polymarket.com/ws/market
```

Subscription contract:

```json
{
  "assets_ids": ["<token_id_1>", "<token_id_2>"],
  "type": "market",
  "custom_feature_enabled": true
}
```

`assets_ids` is mandatory. `asset_ids` is rejected by the SDK validator. `custom_feature_enabled=true` is enabled by default so the adapter can receive `best_bid_ask`, `new_market`, and `market_resolved`.

Supported market events:

| `event_type` | Handling |
|---|---|
| `book` | Validate `market`, `asset_id`, `timestamp`, `bids`, `asks`; publish raw. |
| `price_change` | Validate `market`, `timestamp`, and `changes` or `price_changes`; publish raw. |
| `best_bid_ask` | Validate top-of-book fields; publish raw. |
| `last_trade_price` | Validate trade price fields; publish raw. |
| `tick_size_change` | Validate tick-size metadata; publish raw. |
| `new_market` | Publish raw lifecycle event. |
| `market_resolved` | Publish raw and set `SourceState.status=market_resolved`. |
| unknown | Preserve raw, mark `unknown_schema`, and keep a schema-health signal. |

The service sends WebSocket `PING` every configured heartbeat interval. `PONG`, `PING`, market messages, and stale timeout are handled in the read loop. Stale market WS sets `SourceState.status=stale`, disables live signal input, and reconnects with endpoint-scoped backoff.

## Local CSV Output

The market adapter can append Polymarket market observations to the shared local CSV sink after `raw.polymarket.market` publish succeeds:

```bash
cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode market-ws --csv-output output/local-csv
```

The config section `[local_csv]` defaults to `enabled=false`; `--csv-output` is a manual local override. CSV files are written under `output/local-csv/polymarket/{league}/` with filenames like `2026-05-16T233000Z_lakers_vs_warriors_moneyline.csv`. Each row contains only source generated time, local fetched time, team A in Polymarket-style probability format, and team B in Polymarket-style probability format.

Polymarket WS messages usually contain condition/token/price fields but not full sport, league, team, and start-time metadata. The market adapter therefore enriches local CSV rows from the discovery token cache before writing. If a WS message cannot be matched to discovery metadata, it is still published as raw data but is skipped for local CSV so the sink does not create `unknown_sport/unknown_time` files. These values are for observation only. They are not edge, signal, risk, order intent, or execution approval.

See [Local CSV Output](../storage/local-csv-output.md) for fields, file naming, and cleanup.

## User WebSocket

User WS endpoint:

```text
wss://ws-subscriptions-clob.polymarket.com/ws/user
```

Subscription contract:

```json
{
  "auth": {
    "apiKey": "<redacted>",
    "secret": "<redacted>",
    "passphrase": "<redacted>"
  },
  "markets": ["<condition_id>"],
  "type": "user"
}
```

The user channel subscribes by condition ids in `markets`, not token ids. The adapter parses `order`, `order_update`, and `trade`/`fill` raw payloads, then publishes them to `raw.polymarket.user`. It performs no reconciliation, no signing, and no order mutation.

If `POLYMARKET_API_KEY`, `POLYMARKET_SECRET`, or `POLYMARKET_PASSPHRASE` is missing, the user adapter sets `SourceState.status=auth_missing`, prints a redacted operator message, and exits without failing market ingestion or CI. No private key is read from config.

## Geoblock Probe

Geoblock endpoint:

```text
GET https://polymarket.com/api/geoblock
```

The parser reads `blocked`, `country`, and `region`; IP is always emitted as `<redacted-ip>`. `blocked=true` sets:

| SourceState field | Value |
|---|---|
| `source` | `polymarket_geoblock` |
| `status` | `blocked` |
| `geoblocked` | `true` |
| `live_execution_allowed` | `false` |
| `block_reason` | `geoblocked` |

Malformed or unknown geoblock status in live mode is a fail-closed condition for future execution. Phase 4 does not enable execution.

## Server Time Probe

Server time endpoint:

```text
GET https://clob.polymarket.com/time
```

The probe parses Unix seconds from a JSON number or `server_time`-style field, compares it to local wall time, and records `offset_ms`. Large offsets mark the time source degraded. Phase 4 does not use the offset for trading logic; later latency/clock work can consume it.

## SourceState

Covered states:

| Source | States |
|---|---|
| `polymarket_market` | `ok`, `stale`, `rate_limited`, `schema_error`, `market_resolved` |
| `polymarket_user` | `ok`, `disabled`, `auth_missing`, `auth_failed`, `stale` |
| `polymarket_geoblock` | `ok`, `blocked`, `unknown` |
| `polymarket_time` | `ok`, `degraded` |

Rules:

- Market WS stale means live signal input must not use Polymarket market data.
- User WS stale means future live execution reconciliation is not ready.
- Geoblock blocked or unknown means future live execution must fail closed.
- `execution_enabled` defaults to `false` and Phase 4 never opens an execution path.

## RawMessage Schema

`raw.polymarket.market`:

| Field | Value |
|---|---|
| `provider` | `polymarket` |
| `source_channel` | `rest_discovery`, `ws_market`, `rest_geoblock`, `rest_time` |
| `provider_event_id` | condition id when present |
| `provider_market_id` | token/asset id when present |
| `payload_hash` | deterministic SHA-256 over JSON payload |
| `raw_ref` | deterministic raw reference |
| `payload` | raw external JSON with sensitive fields redacted when relevant |

`raw.polymarket.user`:

| Field | Value |
|---|---|
| `provider` | `polymarket` |
| `source_channel` | `ws_user` |
| `provider_message_id` | order/fill id when present |
| `provider_event_id` | condition id when present |
| `provider_market_id` | asset id when present |
| `payload` | raw user JSON with auth, signatures, and transaction hashes redacted |

## Errors And DLQ

Structured DLQ records include `error_code`, `error_message`, `provider`, `source_channel`, `payload_hash`, `raw_ref`, `received_at`, `schema_version`, and `trace_id`.

Covered errors include unknown event type, missing required field, malformed JSON, auth missing, auth failed, rate limited, stale connection, geoblock malformed, token cache missing, and subscription rejected. Secrets are scrubbed before errors or user raw payloads are emitted.

## Rate Budget

Config keeps rate budgets endpoint-scoped under `rate_budgets_by_endpoint` for market WS, user WS, discovery, geoblock, time, and future CLOB order paths. Phase 4 marks the affected endpoint rate-limited and applies reconnect/backoff locally; it does not hard-code a single global limiter.

## Mock And Fixture Tests

Run:

```bash
make polymarket-test
make polymarket-integration-test
```

Mock coverage includes discovery active markets, closed-market filtering, sports filtering, token cache TTL, market subscription `assets_ids`, rejection of `asset_ids`, market `book`, `price_change`, `best_bid_ask`, `last_trade_price`, `tick_size_change`, `new_market`, `market_resolved`, unknown event, missing field DLQ, user auth missing, user order, user fill, secret redaction, geoblock blocked/allowed, time probe, endpoint rate limited, stale timeout, reconnect/backoff, and 1k message publish smoke.

## Running Adapters

Market adapter:

```bash
cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode discovery
cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode market-ws
cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode geoblock
cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode time-probe
```

User adapter:

```bash
cargo run -p adapter-polymarket-user -- --config configs/sources/polymarket.example.toml --mode auth-check
cargo run -p adapter-polymarket-user -- --config configs/sources/polymarket.example.toml --mode user-ws
```

Public probes:

```bash
make polymarket-public-probe
make polymarket-geoblock-probe
```

These probes are manual and not part of CI. They do not call order endpoints, read private keys, generate signed orders, or write real payload fixtures.

## Phase Boundaries

Phase 5 can attach durable raw archive and source health storage. Phase 6 can map raw Polymarket conditions/tokens to canonical events. Phase 8 can consume mapped normalized quotes for dry-run signal only after Phase 6/7. Phase 13 may introduce execution contracts and signer isolation. Live execution remains blocked until paper, risk, geoblock, heartbeat, audit, and mock execution gates pass.
