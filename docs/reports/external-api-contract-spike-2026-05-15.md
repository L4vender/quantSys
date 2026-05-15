# External API Contract Spike - 2026-05-15

## 1. Phase 1 Scope

Phase 1 only calibrates external API contracts before writing production adapters. It fixes mock/doc-derived fixtures, a fixture manifest, source config examples, a minimal contract smoke test, and adapter input/output baselines for TheRundown V2 and Polymarket CLOB.

This phase does not implement strategy, risk, paper broker, real signing, real order submission, frontend pages, TheRundown execution, a second execution venue, or spread/total live trading.

Authoritative project documents reviewed:

- `README.md`
- `docs/1_external_api_contract_spike.md`
- `docs/development-phases/phase-01-external-api-contract.md`
- `docs/interface-document.md`
- `docs/2_architecture_target.md`
- `docs/4_risk_and_validation_plan.md`
- `docs/5_deployment_requirements.md`

Official external documentation checked on 2026-05-15:

- TheRundown WebSocket: https://docs.therundown.io/api-reference/v2/websocket
- TheRundown Rate Limits: https://docs.therundown.io/rate-limits
- TheRundown Events: https://docs.therundown.io/api-reference/v2/events
- TheRundown Markets Delta: https://docs.therundown.io/api-reference/generated/v2-markets/get-market-price-changes-since-a-given-id
- Polymarket Market Channel: https://docs.polymarket.com/api-reference/wss/market
- Polymarket User Channel: https://docs.polymarket.com/api-reference/wss/user
- Polymarket Rate Limits: https://docs.polymarket.com/quickstart/introduction/rate-limits
- Polymarket Geoblock: https://docs.polymarket.com/api-reference/geoblock
- Polymarket Order Types: https://docs.polymarket.com/developers/CLOB/orders/onchain-order-info
- Polymarket Authentication: https://docs.polymarket.com/developers/CLOB/authentication

## 2. Repository Audit

Before this phase, the repository was documentation-only. These required paths were missing and have now been created as the minimum Phase 1 support structure:

- `docs/reports/`
- `docs/adapters/`
- `tests/fixtures/external/therundown/`
- `tests/fixtures/external/polymarket/`
- `tests/contract/`
- `configs/sources/`
- `crates/`
- `services/`
- `scripts/`

No Rust workspace, Python package, Node package, services, strategy modules, execution modules, or frontend app existed before Phase 1. This phase intentionally did not create a broad Phase 2 engineering skeleton.

Document consistency review:

- No conflict found on the main architecture direction: TheRundown V2 is a data source only; Polymarket CLOB is the only real execution venue.
- No conflict found on live gating: paper, risk, geoblock, heartbeat, and audit remain hard prerequisites before live execution.
- No conflict found on Phase 1 non-goals: strategy, risk, paper broker, real orders, and frontend pages remain out of scope.
- 待确认项 / blocking question: `docs/interface-document.md` contains a strategy parameter example with `allowed_market_types: ["moneyline", "spread", "total"]`, while `README.md` and `docs/2_architecture_target.md` define P0 live scope as full-game moneyline only and explicitly exclude spread/total live. Treat the interface example as non-live/future-facing until confirmed.
- 待确认项 / operational question: local `.env` uses `THERUNDON_API_KEY`. The example config follows that current name, but a future cleanup may want to standardize on `THERUNDOWN_API_KEY` with backward-compatible aliasing.

## 3. TheRundown Contract Baseline

Role: external sportsbook odds and sports events source only. It must never be modeled as an execution venue.

Authentication:

- REST uses `X-TheRundown-Key`.
- WebSocket uses query `key`.
- No API key may be logged, committed, archived in fixtures, or written to ordinary database fields.

Endpoint and subscription:

- V2 WebSocket endpoint baseline: `wss://therundown.io/api/v2/ws/markets?key=...`.
- Production subscriptions must filter by at least one of `sport_ids`, `market_ids`, `affiliate_ids`, or `event_ids`.
- The 256-message client buffer risk is production blocking: handlers must parse and enqueue quickly, subscriptions must be narrow, and reconnect recovery must bootstrap from REST/delta.

WebSocket dispatch:

- Dispatch by `meta.type`.
- Supported baseline types are `market_price` and `heartbeat`.
- Unknown `meta.type` goes to raw archive plus schema alert and must not be silently dropped.

`market_price` required payload fields:

- `data.id`
- `data.event_id`
- `data.affiliate_id`
- `data.market_id`
- `data.market_participant_id`
- `data.normalized_market_participant_id`
- `data.line`
- `data.price`
- `data.previous_price`
- `data.is_main_line`
- `data.sport_id`
- `data.updated_at`

Heartbeat and freshness:

- Heartbeat baseline is 15 seconds.
- If no heartbeat or market message arrives within 30 seconds, mark the source stale.
- A stale source must not produce signal or execution inputs until it recovers and bootstraps as needed.

Rate, entitlement, and budget headers:

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

Rate limiting behavior:

- HTTP `429` must respect `Retry-After`.
- Backoff applies to the affected endpoint/rate budget and must avoid reconnect storms.
- `X-Datapoints-Remaining=0` disables live primary signal until budget reset or manual confirmation.

Data delay and live signal gate:

- `X-Data-Delay-Seconds` missing, unknown, or greater than 0 disables live primary signal.
- `X-Websocket-Access=false` or unknown disables live primary signal.
- Non-real-time subscription plans or no WebSocket permission must not enter live primary signal mode.

REST bootstrap and market delta:

- REST event bootstrap records canonical TheRundown `event_id`.
- REST event bootstrap stores `meta.delta_last_id` as the starting delta cursor.
- Market delta uses `last_id`.
- If a cursor is stale, expired, or rejected, re-run REST bootstrap and resume from the new `meta.delta_last_id`.

Off-board sentinel:

- `price=0.0001` is the off-board sentinel.
- It sets `quality_flags.off_board=true`.
- It must not participate in probability conversion, no-vig calculation, lead-lag detection, edge calculation, signal generation, or live execution decisions.

Real probe summary from current local environment:

- REST entitlement probe with local `.env` key returned HTTP 200.
- Sanitized headers observed: `x-tier=ultra`, `x-data-delay-seconds=0`, `x-websocket-access=true`, `x-rate-limit=10`, and `x-datapoints-*`.
- Filtered WebSocket probe opened successfully and received a `heartbeat`.
- No real payload or secret was written to fixture files. Probe details are summarized only in this report.

## 4. Polymarket Contract Baseline

Role: CLOB market data source and the only real execution venue. Phase 1 does not submit real orders.

Endpoints and authentication:

- Market WebSocket endpoint: `wss://ws-subscriptions-clob.polymarket.com/ws/market`.
- User WebSocket endpoint: `wss://ws-subscriptions-clob.polymarket.com/ws/user`.
- Market channel is unauthenticated.
- User channel requires `apiKey`, `secret`, and `passphrase`.
- CLOB L1/L2 headers, API key derivation, and signing fixtures are not live-tested in Phase 1; they must pass mock contract tests before any live order path.

Subscriptions:

- Market channel subscription field must be `assets_ids`.
- Do not use `asset_ids`.
- User channel subscription field is `markets`, containing condition IDs.
- `custom_feature_enabled=true` enables extended market events such as `best_bid_ask`, `new_market`, and `market_resolved`.

Market event baseline:

- `book`
- `price_change`
- `last_trade_price`
- `tick_size_change`
- `best_bid_ask`
- `new_market`
- `market_resolved`

Heartbeat and stale behavior:

- Client sends `PING` every 10 seconds.
- Missing `PONG`, missing market/user messages beyond configured `stale_after_seconds`, or connection close marks the source stale.
- Stale Polymarket market data disables signal/execution inputs.
- Stale Polymarket user channel disables live execution reconciliation readiness.
- Reconnect must use configured backoff and endpoint-level rate budget.

Rate limits:

- Rate budget is endpoint-scoped.
- Do not hard-code one global rate limiter for all Polymarket CLOB and WS endpoints.

Geoblock:

- Geoblock endpoint baseline: `GET https://polymarket.com/api/geoblock`.
- Geoblock is a live execution hard gate.
- `blocked=true` requires live execution fail closed.
- Unknown geoblock status in live mode must also fail closed.

Order and signing contract baseline:

- P0 live order type is marketable limit + FAK.
- Production must handle partial fill, cancelled remainder, unknown status, and reconcile.
- P0 signing mode is fixed to deposit wallet / `POLY_1271`.
- Mock create-order response fixture exists only as a parser contract and does not permit real order submission.

Real probe summary from current local environment:

- Geoblock endpoint returned HTTP 200.
- Sanitized response summary: `blocked=false`, country `HK`, IP redacted.
- This probe is not a durable live execution approval. Every live deployment and every pretrade path must enforce geoblock checks.

## 5. Secret / Compliance Baseline

Rules:

- API key, secret, passphrase, private key, and signature values must never enter logs, fixtures, reports, test snapshots, frontend bundles, raw archive object keys, or ordinary database fields.
- Fixtures must be mock, official-doc-derived, or live-sanitized.
- Any live-sanitized fixture must scrub all secret-like fields before it is committed.
- Manifest entries must record every sanitized field.
- Geoblock is a live execution hard gate.
- If TheRundown real-time permission cannot be confirmed, live primary signal must be disabled.
- If TheRundown WebSocket permission cannot be confirmed, live primary signal must be disabled.
- If Polymarket geoblock cannot be confirmed, live execution must be disabled.
- If Polymarket user channel auth cannot be confirmed, only mock/paper/replay paths are allowed.
- If order signing cannot be confirmed, real order submission is prohibited.

Repository safety action:

- `.gitignore` now excludes `.env` and `.env.*`, while allowing `.env.example`.
- The local `.env` was read only for probe execution and was not modified.

## 6. Probed / Unprobed / Unknown

Probed:

| Item | Result | Consequence |
|---|---|---|
| TheRundown REST entitlement | HTTP 200; sanitized headers included `ultra`, zero delay, WS access true, rate/data-point headers | Current key appears suitable for adapter contract development, but live signal still requires runtime gating. |
| TheRundown filtered WS open | Connected and received `heartbeat` | WS availability confirmed for current key/network at probe time. |
| Polymarket geoblock | HTTP 200; sanitized `blocked=false`, IP redacted | Current network probe is not enough for live approval; runtime geoblock gate remains required. |

Unprobed / Unknown:

| Item | Conservative degradation |
|---|---|
| TheRundown sustained WS market update delivery under production filters | Treat sustained freshness as unknown until Phase 3 mock/live soak; stale source disables signal/execution. |
| TheRundown 256-message buffer behavior under burst | Require narrow filters, parse+enqueue handler, and reconnect bootstrap before production. |
| TheRundown 429 behavior for this account | Use mock 429 / `Retry-After`; obey endpoint backoff and disable live signal on budget exhaustion. |
| Polymarket user channel auth | Only mock/paper/replay; no live execution reconciliation until L2 auth is confirmed. |
| Polymarket endpoint-specific rate budgets for final production volume | Configure by endpoint; do not hard-code one global limiter. |
| Polymarket order signing and L1/L2 headers | Mock only; real order submission prohibited until signing contract tests and controlled live-readiness gates pass. |
| Polymarket create order / FAK execution | Mock contract only; no real orders in Phase 1. |

Required fail-closed defaults:

- TheRundown WS access unconfirmed -> live primary signal disabled.
- TheRundown `X-Data-Delay-Seconds` unconfirmed or greater than 0 -> live primary signal disabled.
- Polymarket geoblock unconfirmed -> live execution disabled.
- Polymarket user channel auth unconfirmed -> mock / paper / replay only.
- Order signing unconfirmed -> real order submission prohibited.

## 7. Contract Artifacts

Generated artifacts:

- `tests/fixtures/external/therundown/events_bootstrap.json`
- `tests/fixtures/external/therundown/markets_delta.json`
- `tests/fixtures/external/therundown/ws_market_price.json`
- `tests/fixtures/external/therundown/ws_heartbeat.json`
- `tests/fixtures/external/therundown/rate_limit_headers.json`
- `tests/fixtures/external/therundown/off_board_price.json`
- `tests/fixtures/external/polymarket/market_subscribe.json`
- `tests/fixtures/external/polymarket/market_book.json`
- `tests/fixtures/external/polymarket/market_price_change.json`
- `tests/fixtures/external/polymarket/market_best_bid_ask.json`
- `tests/fixtures/external/polymarket/user_order_update.json`
- `tests/fixtures/external/polymarket/geoblock_blocked.json`
- `tests/fixtures/external/polymarket/create_order_response.json`
- `tests/contract/external_api_contract_manifest.yaml`
- `configs/sources/therundown.example.toml`
- `configs/sources/polymarket.example.toml`
- `docs/adapters/api-contract-baseline.md`
- `scripts/contract/check_external_api_contract.py`
- `Makefile`

## 8. Phase 2 Readiness

Status: **Phase 2 Ready**.

Phase 2 may start after the local contract smoke test passes, because the required contract baseline, fixtures, manifest, config examples, and adapter baseline documentation are present.

Live execution is not ready. The following remain blocking for live execution, not for Phase 2 foundation work:

- Polymarket user channel auth confirmation.
- L1/L2 header and API key derivation mock tests.
- `POLY_1271` signing fixture and signer isolation.
- Runtime geoblock gate in execution path.
- Risk engine, paper broker, replay reports, audit, and live operations gates from later phases.
