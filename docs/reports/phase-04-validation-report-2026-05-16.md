# Phase 4 Polymarket Ingestion Validation Report

## Verification Time

- Time: 2026-05-16 03:08:44 CST
- Commit: `6f220a01d21e39d46d99558d6237b2180d006084`
- Worktree: dirty; Phase 4 implementation files are present but not committed in this workspace.
- Verdict: **PARTIAL**
- Phase 5 entry: **not approved yet**

The required build and test commands pass, and the main mock/fixture ingestion path proves that Polymarket market/user raw payloads can be wrapped and published to `raw.polymarket.*`. Validation still found Phase 4 hard-gate gaps around malformed geoblock/time probe state handling and weaker-than-specified smoke coverage.

## Validation Scope

Reviewed:

- `README.md`
- `docs/3_development_phases.md`
- `docs/development-phases/phase-04-polymarket-ingestion.md`
- `docs/1_external_api_contract_spike.md`
- `docs/reports/external-api-contract-spike-2026-05-15.md`
- `docs/adapters/api-contract-baseline.md`
- `docs/adapters/polymarket.md`
- `docs/adapters/therundown.md`
- `docs/interface-document.md`
- `docs/2_architecture_target.md`
- `docs/schema/topic-catalog.md`
- `configs/sources/polymarket.example.toml`
- `services/adapter-polymarket-market/`
- `services/adapter-polymarket-user/`
- `crates/source-sdk/`
- `crates/domain/`
- `crates/eventbus/`
- `crates/config/`
- `crates/telemetry/`
- `crates/test-support/`
- `tests/fixtures/external/polymarket/`
- `tests/contract/`
- `Makefile`
- `.github/workflows/ci.yml`

## Static Audit

| Item | Result | Notes |
| --- | --- | --- |
| `services/adapter-polymarket-market/` | passed | Market adapter crate exists. |
| `services/adapter-polymarket-user/` | passed | User adapter crate exists. |
| `crates/source-sdk/src/polymarket/` | passed | Module directory exists with discovery, parser, subscription, token cache, state, probes, publisher, errors. |
| `crates/source-sdk/src/polymarket.rs` | n/a | Directory module is used instead. |
| `docs/adapters/polymarket.md` | passed | Present and mostly complete. |
| `configs/sources/polymarket.example.toml` | passed | Contains defaults, env names, budgets, `execution_enabled=false`, `geoblock_required=true`. |
| Required Polymarket fixtures | passed | Required fixtures exist, including `market_subscribe.json`, `market_book.json`, `market_price_change.json`, `market_best_bid_ask.json`, `user_order_update.json`, `geoblock_blocked.json`, and `create_order_response.json`. |
| Polymarket tests | passed | Present as `crates/source-sdk/tests/polymarket_unit.rs` and `crates/source-sdk/tests/polymarket_integration.rs`. |
| Top-level `tests/integration/` | weak | No top-level directory; equivalent Rust integration tests live under `crates/source-sdk/tests/`. |
| Makefile targets | passed | All requested targets exist. |
| CI workflow | passed | Runs fmt, clippy, workspace tests, contract test, TheRundown test, Polymarket test. |

Polymarket SDK module structure:

- `discovery.rs`
- `error.rs`
- `geoblock.rs`
- `market_ws.rs`
- `mod.rs`
- `parser.rs`
- `publisher.rs`
- `state.rs`
- `subscription.rs`
- `time_probe.rs`
- `token_cache.rs`

Polymarket adapter service files:

- `services/adapter-polymarket-market/src/app.rs`
- `services/adapter-polymarket-market/src/config.rs`
- `services/adapter-polymarket-market/src/lib.rs`
- `services/adapter-polymarket-market/src/main.rs`
- `services/adapter-polymarket-user/src/app.rs`
- `services/adapter-polymarket-user/src/config.rs`
- `services/adapter-polymarket-user/src/lib.rs`
- `services/adapter-polymarket-user/src/main.rs`

## Command Results

| Command | Result | Evidence |
| --- | --- | --- |
| `make contract-test` | passed | External API contract smoke checks passed. |
| `make fmt` | passed | `cargo fmt --all --check` passed. |
| `make clippy` | passed | `cargo clippy --workspace --all-targets -- -D warnings` passed. |
| `make test` | passed | Workspace tests passed, including Polymarket unit/integration tests. |
| `make therundown-test` | passed | 15 unit, 14 integration, and adapter health test passed. |
| `make polymarket-test` | passed | 10 Polymarket unit, 10 Polymarket integration, and both adapter crates passed. |
| `make polymarket-integration-test` | passed | 10 Polymarket integration tests passed. |
| `make adapter-polymarket-market` | passed | Market adapter crate built successfully. |
| `make adapter-polymarket-user` | passed | User adapter crate built successfully. |
| `make check` | passed | fmt, clippy, workspace tests, contract, TheRundown, and Polymarket tests passed. |
| `make polymarket-contract-test` | passed | Contract plus Polymarket test target passed. |
| `make polymarket-mock` | passed | Runs one market WS mock smoke test; narrower than the full integration suite. |
| `make polymarket-public-probe` | passed | `active_sports_markets=262`, `filtered_closed=546`, `filtered_non_sports=73`, `token_cache_tokens=524`, topic `raw.polymarket.market`. |
| `make polymarket-geoblock-probe` | passed | `blocked=false`, country `HK`, IP printed as `<redacted-ip>`, `live_execution_allowed=false`. |
| `cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode time-probe` | passed | `offset_ms=1000`, `large_offset_warning=false`. |
| `cargo run -p adapter-polymarket-user -- --config configs/sources/polymarket.example.toml --mode auth-check` | passed | Missing user credentials reported as `auth_missing`; no panic and no env values printed. |

No required command failed.

## Unit Test Coverage Matrix

| Requirement | Result | Evidence |
| --- | --- | --- |
| Market subscription payload contract | passed | `market_subscription_payload_uses_assets_ids_and_custom_feature_contract` |
| `assets_ids` field correct | passed | Same test asserts `assets_ids` present. |
| `asset_ids` field forbidden | passed | Same test rejects payload containing `asset_ids`. |
| `custom_feature_enabled` field correct | passed | Same test asserts `true`. |
| User subscription payload contract | passed | `user_subscription_payload_uses_markets_condition_ids_and_redacts_auth` |
| User subscription uses `markets` condition IDs | passed | Same test asserts `markets[0]`. |
| User auth secret redaction | passed | Same test checks redacted JSON and `Debug`/`Display`. |
| Discovery parser | passed | `discovery_parser_filters_active_open_sports_markets_and_builds_token_cache` |
| Discovery active/open filters | passed | Same test covers active event/market and closed filtering. |
| Sports market filter | passed | Same test covers sports and non-sports counters. |
| Token cache insert/lookup/TTL | passed | Same test covers upsert, condition/token/slug/outcome, and TTL expiry. |
| `condition_id -> token_ids` | passed | Same test. |
| `token_id -> condition_id` | passed | Same test. |
| `book` parser | passed | `market_ws_parser_dispatches_supported_market_event_types` |
| `price_change` parser | passed | Same test. |
| `best_bid_ask` parser | passed | Same test. |
| `last_trade_price` parser | passed | Same test. |
| `tick_size_change` parser | passed | Same test. |
| `new_market` parser | passed | Same test. |
| `market_resolved` parser | passed | Same test. |
| Unknown market event | passed | `market_ws_parser_preserves_unknown_and_rejects_missing_required_fields` |
| Missing market required field | passed | Same test returns parser error; integration publishes DLQ. |
| User order parser | passed | `user_ws_parser_parses_order_fill_and_redacts_secrets_from_raw` |
| User fill parser | passed | Same test. |
| User order_update parser | passed | Same test covers inline `order_update`. |
| Geoblock blocked parser | passed | `geoblock_parser_redacts_ip_and_state_machine_fails_closed_when_blocked` |
| Geoblock allowed parser | passed | Same test. |
| Malformed geoblock parser | passed | Same test asserts parse error for missing `blocked`. |
| Time parser | passed | `time_probe_parser_calculates_offsets_and_large_offset_warning` |
| Missing server time handling | passed | Same test asserts parser error. |
| Malformed server time handling | passed | Same test asserts parser error. |
| Negative offset | passed | Same test. |
| Large offset warning | passed | Same test. |
| SourceState geoblock blocked gate | passed | Geoblock state test. |
| SourceState geoblock unknown fail closed | weak | State machine implements `geoblock_unknown`, but no direct unit test; adapter malformed path does not set it. |
| SourceState market stale gate | passed | `source_state_covers_market_stale_user_auth_missing_and_market_resolved` |
| SourceState user auth_missing | passed | Same test. |
| SourceState user stale | passed | Integration stale test. |
| SourceState user auth_failed | weak | State function exists; no direct test or mock auth failure scenario. |
| SourceState user disabled | weak | Initial user state is disabled; no direct assertion. |
| SourceState market_resolved | passed | Unit and integration tests. |
| Payload hash deterministic | passed | `polymarket_payload_hash_and_raw_message_construction_are_deterministic` |
| RawMessage construction | passed | Same test and parser tests. |
| Provider = polymarket | passed | Discovery and RawMessage tests. |
| Topic routing `raw.polymarket.market` | passed | Integration discovery/market tests. |
| Topic routing `raw.polymarket.user` | passed | Integration user test. |
| Secret not in error display | passed | Auth `Debug`/`Display` tests and secret audit. |
| No-order-call guard | passed | Static audit found no order endpoint mutation call in Phase 4 adapter code. |

## Integration and Mock Scenario Matrix

| Scenario | Result | Evidence |
| --- | --- | --- |
| Discovery active markets 200 | passed | `discovery_builds_token_cache_and_publishes_raw_polymarket_market` |
| Discovery closed markets filtered out | passed | Unit discovery filter test. |
| Discovery missing token ids | passed | `discovery_missing_token_ids_goes_to_dlq_without_publish` |
| Discovery sports filter | passed | Unit discovery filter test. |
| Discovery to token cache | passed | Integration discovery test. |
| Discovery to `raw.polymarket.market` publish | passed | Integration discovery test asserts topic. |
| Token cache to market WS subscribe | passed | `token_cache_constructs_market_ws_subscription_with_assets_ids` |
| Market WS subscribe uses `assets_ids` | passed | Unit and integration subscription tests. |
| Market WS rejects `asset_ids` | passed | Unit subscription validator test. |
| Market WS `book` to RawMessage to market topic | passed | `market_ws_events_publish_raw_and_market_resolved_updates_source_state` |
| Market WS `price_change` to RawMessage to market topic | passed | Same integration test. |
| Market WS `best_bid_ask` to RawMessage to market topic | passed | Same integration test. |
| Market WS `last_trade_price` to RawMessage to market topic | passed | Same integration test. |
| Market WS `tick_size_change` to RawMessage to market topic | passed | Same integration test. |
| Market WS `new_market` to RawMessage to market topic | passed | Same integration test. |
| Market WS `market_resolved` to RawMessage + SourceState | passed | Same integration test. |
| Market WS unknown event type | passed | `unknown_or_missing_market_ws_schema_goes_to_raw_or_dlq` |
| Market WS missing required field | passed | Same integration test publishes DLQ. |
| Market WS PING/PONG | partial | Service loop sends `PING` and handles `PONG`; tests cover `mark_pong`, stale, and backoff, but not a real mock WS PING/PONG handshake. |
| Market WS stale timeout | passed | `ping_pong_stale_detection_reconnect_backoff_and_rate_limit_state` |
| Market WS reconnect/backoff | passed | Same integration test covers backoff state. |
| User WS auth missing | passed | `user_auth_missing_is_disabled_without_failing_market_adapter` and CLI `auth-check`. |
| User WS auth success | partial | Auth payload construction with credentials is tested; no mock user WS handshake accepting auth. |
| User WS order update to user topic | passed | `user_ws_order_and_fill_publish_raw_polymarket_user_without_credentials_in_payload` |
| User WS fill update to user topic | passed | Same integration test. |
| User WS unknown event raw | weak | Parser supports unknown user events; no explicit integration assertion. |
| User WS secret redaction | passed | Unit and integration tests. |
| Geoblock blocked=true to SourceState blocked | passed | Integration geoblock probe test. |
| Geoblock blocked=false to SourceState ok | passed | Integration geoblock probe test. |
| Geoblock malformed to SourceState unknown/fail closed | failed | Parser error is covered, but `probe_geoblock` returns before setting `SourceState=polymarket_geoblock unknown`. |
| Geoblock network error to unknown/fail closed | partial | Code sets `geoblock_unknown` on transport/HTTP error; no explicit integration test. |
| Time probe ok | passed | Integration time probe test and public time-probe CLI. |
| Time probe malformed to degraded | failed | Parser error is covered, but `probe_time_at` returns before setting `SourceState=polymarket_time degraded`. |
| Endpoint rate limited | passed | Integration marks endpoint budget rate-limited and state `RateLimited`; no HTTP 429 transport test for Polymarket. |
| Token cache TTL expired | passed | Unit token cache TTL assertions. |
| Fixture replay `market_book.json` | passed | Unit and integration parser/publish tests. |
| Fixture replay `market_price_change.json` | passed | Unit and integration parser/publish tests. |
| Fixture replay `market_best_bid_ask.json` | passed | Unit and integration parser/publish tests. |
| Fixture replay `user_order_update.json` | passed | Unit and integration parser/publish tests. |
| Fixture replay `geoblock_blocked.json` | passed | Unit and integration geoblock tests. |

## `raw.polymarket.market` Output Validation

| Validation Point | Result | Notes |
| --- | --- | --- |
| Discovery outputs RawMessage | passed | Discovery parser builds `RawMessage` with `SourceChannel::RestDiscovery`; integration publishes to market topic. |
| WS `book` outputs RawMessage | passed | Parser and integration tests. |
| WS `price_change` outputs RawMessage | passed | Parser and integration tests. |
| WS `best_bid_ask` outputs RawMessage | passed | Parser and integration tests. |
| WS `last_trade_price` outputs RawMessage | passed | Parser and integration tests. |
| WS `tick_size_change` outputs RawMessage | passed | Parser and integration tests. |
| WS `new_market` outputs RawMessage | passed | Parser and integration tests. |
| WS `market_resolved` outputs RawMessage | passed | Parser and integration tests. |
| `RawMessage.provider=polymarket` | passed | Unit assertions. |
| `source_channel=rest_discovery/ws_market/rest_geoblock/rest_time` | passed | Code and tests. |
| `provider_event_id` traceable to condition id/event slug | passed | Market parser uses `market`; discovery uses first discovered condition id. |
| `provider_market_id` traceable to token/asset/condition | passed | Market parser uses `asset_id` or condition id; discovery uses first token. |
| `payload_hash` deterministic | passed | Unit hash test. |
| `raw_id` deterministic | passed | Unit RawMessage test. |
| EventEnvelope topic = `raw.polymarket.market` | passed | Publisher routes non-user Polymarket channels to market topic. |
| No `norm.quote` output | passed | Polymarket publisher only emits raw topics. |
| No mapping/signal/order/risk/execution output | passed | No Phase 4 Polymarket adapter output to future topics. Future topics remain Phase 2 eventbus metadata only. |

## `raw.polymarket.user` Output Validation

| Validation Point | Result | Notes |
| --- | --- | --- |
| User order update outputs RawMessage | passed | Parser and integration tests. |
| User fill outputs RawMessage | passed | Parser and integration tests. |
| User unknown event raw or structured error | weak | Parser supports unknown raw; explicit integration coverage missing. |
| `RawMessage.provider=polymarket` | passed | Parser construction. |
| `source_channel=ws_user` | passed | Unit and integration tests. |
| EventEnvelope topic = `raw.polymarket.user` | passed | Publisher maps `WsUser` to user raw topic. |
| Auth secret not in RawMessage | passed | Payload redaction tests. |
| Auth secret not in logs/errors | passed | Auth missing prints env names only; credentials `Debug`/`Display` redacted. |
| No order endpoint call | passed | Static audit found no order mutation call. |
| No signed order generation | passed | Static audit found no signer/signed order implementation. |
| No private key read | passed | Config uses L2 auth env names only; no private key field. |
| No reconcile logic | passed | User adapter parses and publishes raw only. |
| Missing credentials do not panic | passed | CLI `auth-check` exits successfully with `auth_missing`. |

## SourceState Validation

| SourceState | Result | Evidence |
| --- | --- | --- |
| `polymarket_market ok` | passed | Discovery and market WS handlers call `market_ok`; tests observe ok paths. |
| `polymarket_market stale` | passed | Integration stale test. |
| `polymarket_market rate_limited` | passed | Endpoint budget integration test. |
| `polymarket_market schema_error` | passed | Unknown/missing schema paths. |
| `polymarket_market market_resolved` | passed | Unit and integration tests. |
| `polymarket_user ok` | passed | User order/fill integration test. |
| `polymarket_user disabled` | partial | Initial state is disabled; no direct test assertion. |
| `polymarket_user auth_missing` | passed | Unit, integration, and CLI auth-check. |
| `polymarket_user auth_failed` | weak | State function exists; no mock auth failure test. |
| `polymarket_user stale` | passed | Integration stale test. |
| `polymarket_geoblock ok` | passed | Integration allowed geoblock test. |
| `polymarket_geoblock blocked` | passed | Integration blocked geoblock test. |
| `polymarket_geoblock unknown` | failed for malformed response | State function exists and transport errors set unknown, but malformed successful geoblock responses do not update SourceState. |
| `polymarket_time ok` | passed | Integration time probe test and public CLI. |
| `polymarket_time degraded` | failed for malformed response | Large offset can produce degraded state, but malformed successful time responses do not update SourceState. |

Rules:

- Market WS stale disables live signal input: passed.
- User WS stale leaves live execution reconciliation not ready: passed.
- Geoblock blocked sets `live_execution_allowed=false` and `block_reason=geoblocked`: passed.
- Geoblock unknown in live mode must fail closed: partial; state function does this, but malformed probe response does not reach it.
- Phase 4 does not enable live execution: passed.
- `execution_enabled` defaults false: passed.
- SourceState errors contain no secrets: passed.

## Geoblock and Time Probe Validation

Geoblock:

| Case | Result | Notes |
| --- | --- | --- |
| blocked=true | passed | Fixture and integration. |
| blocked=false | passed | Fixture, integration, and public geoblock probe. |
| malformed response | failed at adapter state level | Parser rejects malformed payload, but adapter does not set `polymarket_geoblock unknown` before returning. |
| network error | partial | Code sets `geoblock_unknown` on transport/HTTP error; no explicit integration test. |
| IP redaction | passed | Parser replaces IP with `<redacted-ip>`; public probe prints redacted IP. |
| Probe result not live approval | passed | Public output keeps `live_execution_allowed=false`; docs state no live approval. |

Time:

| Case | Result | Notes |
| --- | --- | --- |
| normal offset | passed | Fixture integration and public time-probe CLI. |
| missing server time | parser passed, adapter failed | Parser rejects missing time; adapter does not set degraded on parser error. |
| malformed time | parser passed, adapter failed | Parser rejects malformed time; adapter does not set degraded on parser error. |
| negative offset | passed | Unit parser test. |
| large offset warning | passed | Unit parser/state path. |
| Not used for trading logic | passed | No trading logic exists in Phase 4. |
| Docs mention future latency-engine use | passed | Documented in `docs/adapters/polymarket.md`. |

## Secret Safety Audit

Result: **passed with minor documentation placeholders only**.

Findings:

- `.gitignore` ignores `.env` and `.env.*`, while allowing `.env.example`.
- No real Polymarket secret, passphrase, API key, private key, or signature was found in fixtures or docs.
- Fixtures use placeholders such as `<redacted-secret>`, `<redacted-passphrase>`, `<redacted-signature>`, `<redacted-transaction-hash>`, and `<redacted-ip>`.
- `L2Credentials` redacts `Debug` and `Display`.
- User raw parser redacts `apiKey`, `secret`, `passphrase`, `signature`, private-key-like fields, and transaction hashes before RawMessage construction.
- CLI auth-check prints env var names only, not values.
- Public geoblock probe prints `ip=<redacted-ip>`.
- The only `create_order_response.json` fixture is a contract baseline fixture; no Phase 4 adapter code calls an order endpoint.

## Public Probe Results

| Probe | Result | Output |
| --- | --- | --- |
| `make polymarket-public-probe` | passed | `active_sports_markets=262`, `filtered_closed=546`, `filtered_non_sports=73`, `token_cache_tokens=524`, topic `raw.polymarket.market`. |
| `make polymarket-geoblock-probe` | passed | `blocked=false`, country `HK`, region empty, `ip=<redacted-ip>`, `live_execution_allowed=false`. |
| Public market WS smoke | not covered | `market-ws` mode exists but is a long-running loop. There is no finite Makefile smoke target that connects, subscribes, observes one public market WS message, and exits. |

No public probe used credentials, private keys, signed orders, or order endpoints.

## Performance Smoke

| Requirement | Result | Notes |
| --- | --- | --- |
| 1k market updates raw publish path | passed | `raw_publish_path_handles_1k_market_messages_with_mock_p95_under_50ms` publishes 1,000 mock updates. |
| P95 publish < 50ms | partial | Test name says P95, but implementation checks average per-message elapsed time, not actual p95 latency distribution. |
| 1k token IDs memory stable | weak | Config max is 1,000 and cache path is exercised, but no explicit memory measurement exists. |
| Reconnect/backoff not breaking rate budget | partial | Backoff and endpoint budget states are tested; no full loop budget exhaustion test. |

No performance test uses real Polymarket for load.

## Documentation Consistency

`docs/adapters/polymarket.md` covers:

- Phase 4 scope.
- Adapter architecture.
- Public discovery.
- Condition/token cache.
- Market WS and `assets_ids` contract.
- `custom_feature_enabled`.
- Market event types: `book`, `price_change`, `best_bid_ask`, `last_trade_price`, `tick_size_change`, `new_market`, `market_resolved`.
- User WS read-only flow.
- Auth redaction.
- Geoblock and time probes.
- SourceState.
- `raw.polymarket.market` and `raw.polymarket.user`.
- Endpoint budgets.
- PING/PONG and stale/reconnect.
- Token cache TTL.
- Mock/fixture testing.
- Adapter/test commands.
- Phase 5/6/8/13 boundaries.
- Explicit non-goals: real orders, signer, execution, strategy, risk, mapping, normalized quote.

Inconsistencies:

- Docs state malformed or unknown geoblock is a fail-closed condition, but `probe_geoblock` does not set `SourceState=polymarket_geoblock unknown` on malformed successful responses.
- Docs list geoblock malformed as covered, but integration tests do not cover adapter-level malformed geoblock state.
- Time probe docs imply degraded state for missing/malformed time; adapter parser-error path returns without setting `polymarket_time degraded`.

## CI Check

`.github/workflows/ci.yml` runs:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `make contract-test`
- `make therundown-test`
- `make polymarket-test`
- topic init dry-run

CI does not run live user WS probes, does not require Polymarket user credentials, does not call order endpoints, does not depend on private keys, and does not start a long WS soak.

## Phase Boundary Check

Result: **passed**.

Static search did not find Phase 4 adapter implementation of:

- signer
- signed order
- create order
- cancel order
- execution gateway
- paper broker
- risk engine
- strategy engine
- edge calculation
- lead-lag signal
- no-vig as formal normalized quote
- mapping decision
- order intent
- frontend trading UI
- live execution

Notes:

- `crates/eventbus` and `docs/schema/topic-catalog.md` contain future topic metadata (`norm.quote`, `mapping.decision`, `order.intent`, `risk.decision`, `execution.request`) from Phase 2; Polymarket Phase 4 publisher does not emit them.
- `tests/fixtures/external/polymarket/create_order_response.json` is a contract fixture only.
- `configs/sources/polymarket.example.toml` includes future `clob_orders` rate budget and execution-related config labels, but `execution_enabled=false` and no Phase 4 order path uses them.
- Parser references `/order/id` only to extract user-channel raw order payload identifiers.

## Failed, Missing, and Weak Coverage

Failed:

1. `probe_geoblock` does not set `SourceState=polymarket_geoblock unknown` on malformed 2xx geoblock payloads. This violates the geoblock unknown/malformed fail-closed requirement.
2. `probe_time_at` does not set `SourceState=polymarket_time degraded` on malformed or missing 2xx time payloads. This violates the time-probe degraded-state requirement.

Weak or missing coverage:

1. No finite public market WS smoke command exists. `market-ws` is a long-running mode.
2. Mock PING/PONG is covered through state methods and service code inspection, not a real mock WS server handshake.
3. User WS auth success is covered by payload construction, not a mock WS auth-accepted connection.
4. User unknown event raw publish lacks explicit integration assertion.
5. `user_auth_failed`, `user_disabled`, `geoblock_unknown`, and adapter-level `time degraded` need direct tests.
6. Endpoint rate limit is tested by state mutation, not an HTTP 429 Polymarket transport scenario.
7. Performance smoke checks average per-message time, not true p95.
8. 1k token cache memory stability has no explicit measurement.
9. Top-level `tests/integration/` is absent, though crate-level Rust integration tests exist.
10. `make polymarket-mock` is a narrow single-scenario smoke; full mock coverage is in `make polymarket-integration-test`.

## Phase 4 Done Assessment

Passed:

- All required make/build/test commands pass.
- Public discovery mock and live public discovery probe pass.
- Discovery builds condition/token cache.
- Token cache supports condition-to-token and token-to-condition lookup.
- Market subscription uses `assets_ids`, rejects `asset_ids`, and includes `custom_feature_enabled=true`.
- Market WS parser covers all required Phase 4 event types.
- Market raw publishes to `raw.polymarket.market`.
- User order/fill raw publishes to `raw.polymarket.user`.
- Missing user credentials are `auth_missing` and do not break market ingestion.
- Geoblock blocked/allowed happy paths update SourceState.
- Time normal offset works.
- Secrets are redacted.
- No order endpoint, private key, signer, strategy, risk, mapping, normalized quote, paper broker, execution gateway, or frontend trading page was implemented.
- CI avoids real Polymarket user credentials and live user WS.

Not done:

- Geoblock malformed/unknown adapter state handling is incomplete.
- Time malformed/missing adapter degraded handling is incomplete.
- Some smoke tests are state-level rather than transport/WS-level.
- Performance smoke is useful but not a true p95 measurement.

## Phase 5 Gate

Phase 4 should be treated as **PARTIAL**, not PASS. Do not enter Phase 5 until at least the failed items are fixed and tested:

1. On malformed 2xx geoblock response, set `SourceState=polymarket_geoblock unknown`, keep `live_execution_allowed=false`, and publish a sanitized structured error/DLQ record.
2. On malformed or missing 2xx time response, set `SourceState=polymarket_time degraded` and publish a sanitized structured error/DLQ record or equivalent probe error state.
3. Add direct tests for geoblock malformed adapter state and time malformed adapter state.

Recommended before Phase 5:

1. Add a finite `polymarket-market-ws-smoke` or equivalent public WS smoke command that subscribes with `assets_ids`, observes one message or clean timeout, redacts output, and exits.
2. Add mock WS handshake tests for PING/PONG and user auth success.
3. Replace the average publish smoke with a true p95 measurement, or rename the test/report to avoid claiming p95.
4. Add a 1k token cache memory smoke or document the measured upper bound.

## Live Execution Blockers

Live execution remains blocked by design and must continue to be blocked until future phases provide:

- Durable raw archive and replay readiness from Phase 5.
- Canonical mapping from Phase 6.
- Dry-run/signal-only validation from Phase 8.
- Paper broker, risk, audit, signer isolation, execution gateway, and live execution gates from later phases.
- Hard geoblock, stale, secret, and no-order-call guarantees verified at adapter and integration levels.
