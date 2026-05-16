# Phase 4 Polymarket Ingestion Validation Report

## Verification Metadata

- Verification date: `2026-05-16`
- Local time zone: `Asia/Shanghai`
- Current commit: `24a187a3f93537022cf9bf82acc32247c37ffcbf`
- Worktree status: dirty; Phase 3/4 CSV, watchlist, and Polymarket adapter files are still local workspace changes.
- Overall verdict: **PARTIAL**
- Phase 5 entry: **not fully approved as a hard gate**
- Operational capture status: **usable for matched TheRundown/Polymarket raw observation with mock-tested flows**

This report replaces the earlier Phase 4 validation note. It reflects the current implementation after adding local CSV observation, TheRundown to Polymarket watchlist generation, and default watchlist-based websocket capture.

The important distinction is:

- Phase 4 raw ingestion paths for Polymarket market/user data are implemented and tested with fixtures/mocks.
- Current operator workflow can generate `output/live-mapping/ws_watchlist.json`, then run TheRundown and Polymarket WS capture against only matched markets.
- Phase 4 still does not implement strategy, risk, paper broker, signer, order generation, execution gateway, canonical Phase 6 mapping persistence, or live execution.
- The remaining hard-gate weaknesses are malformed probe state handling, finite public WS smoke coverage, and performance measurement depth.

## Current Scope

Validated areas:

- Polymarket public market discovery.
- Condition/token cache.
- Market WS subscription using `assets_ids`.
- Market WS raw event parsing and publish to `raw.polymarket.market`.
- User WS read-only parser and publish to `raw.polymarket.user`.
- User auth missing behavior.
- Geoblock and server time probe happy paths.
- Local CSV observation path.
- TheRundown to Polymarket watchlist generation for matched event/market capture.
- Default watchlist-based WS startup for market adapters.
- Secret redaction and no-order-call boundary.

Out of scope and still not implemented:

- Real order placement.
- Polymarket order endpoint calls.
- Signer or signed orders.
- Private key loading.
- Execution gateway.
- Strategy, edge, lead-lag, no-vig, risk, or paper broker.
- Frontend trading UI.
- Formal Phase 6 canonical mapping storage.

## Static Audit

| Item | Result | Notes |
| --- | --- | --- |
| `services/adapter-polymarket-market/` | passed | Market adapter exists with `discovery`, `market-ws`, `geoblock`, `time-probe`, and `health` modes. |
| `services/adapter-polymarket-user/` | passed | User adapter exists with `user-ws`, `auth-check`, and `health` modes. |
| `crates/source-sdk/src/polymarket/` | passed | Contains discovery, subscription, parser, token cache, state, geoblock, time probe, publisher, errors, and backoff helpers. |
| `configs/sources/polymarket.example.toml` | passed | `execution_enabled=false`, `geoblock_required=true`, `custom_feature_enabled=true`, user auth env names, rate budgets, and local CSV config are present. |
| `docs/adapters/polymarket.md` | passed | Documents Phase 4 scope, discovery, WS, user WS, geoblock/time, SourceState, raw topics, and non-goals. |
| Required Polymarket fixtures | passed | Fixtures cover discovery, market WS events, user order/fill, geoblock, time, and contract baseline order response. |
| Polymarket tests | passed | Unit/integration tests live under `crates/source-sdk/tests/`; adapter tests exist under service crates. |
| Watchlist support | passed | `crates/domain/src/ws_watchlist.rs`, adapter watchlist helpers, and mapping output exist. |
| Makefile targets | passed | Includes Polymarket, local CSV, mapping, and watchlist targets. |
| CI workflow | passed by inspection | CI avoids credentialed user WS and live probes. |

## Current Operator Flow

The current default capture flow is:

```bash
make live-watchlist
make therundown-csv-run
HTTPS_PROXY=http://127.0.0.1:6244 HTTP_PROXY=http://127.0.0.1:6244 ALL_PROXY=http://127.0.0.1:6244 make polymarket-csv-run
```

`make live-watchlist` writes:

```text
output/live-mapping/ws_watchlist.json
```

Both market WS adapters now use this file by default:

| Adapter | Default behavior |
| --- | --- |
| `adapter-therundown --mode ws` | Loads `output/live-mapping/ws_watchlist.json`, subscribes to matched `event_ids` and `market_ids`, preserves configured sportsbook affiliate filters, and drops non-selected lines locally. |
| `adapter-polymarket-market --mode market-ws` | Loads `output/live-mapping/ws_watchlist.json` and builds the market subscription from selected Polymarket `asset_ids` using the required `assets_ids` field. |

The old broad subscription behavior is still available only for explicit debugging:

```bash
--disable-watchlist
```

Normal CSV capture should not use `--disable-watchlist`.

## Watchlist Selection Policy

The watchlist is an operator capture filter, not a trading signal and not final Phase 6 canonical mapping.

Selection rule:

- Match by sport, date, team/player names, market type, period, and line for spread/total.
- Home/away order is audit metadata only and is not required.
- Every matched event keeps at most one market per type:
  - one moneyline,
  - one spread line,
  - one total line.
- For spread/total, if multiple lines match:
  - choose the line with the largest matched market count,
  - if counts tie, choose the median available line.
- For the final selected event/market type, subscribe to one Polymarket condition/token pair after stable confidence/order sorting.

Test coverage:

- `test_ws_watchlist_selects_one_market_per_event_type_by_count_then_median_line`
- `test_therundown_parser_expands_multiple_distinct_spread_lines`
- `test_spread_and_total_candidates_require_same_line`
- `watchlist_extracts_ws_subscription_ids`
- `watchlist_overrides_therundown_event_and_market_filters_but_keeps_affiliates`
- `watchlist_filters_therundown_ws_payload_by_event_market_and_line`
- `watchlist_provides_polymarket_assets_for_market_subscription`

## Fresh Command Results

Commands run for this rewrite:

| Command | Result | Evidence |
| --- | --- | --- |
| `make contract-test` | passed | External API contract smoke checks passed. |
| `make polymarket-test` | passed | 11 Polymarket unit tests, 10 Polymarket integration tests, market adapter tests, user adapter tests, and user markets-file tests passed. |
| `make local-csv-test` | passed | 19 local CSV tests passed. |
| `make mapping-test` | passed | 18 live matching/watchlist tests passed. |
| `cargo test -p quantsys-domain --test ws_watchlist` | passed | 1 watchlist DTO test passed. |
| `cargo test -p adapter-therundown --test watchlist` | passed | 2 TheRundown watchlist tests passed. |
| `cargo test -p adapter-polymarket-market --test watchlist` | passed | 1 Polymarket watchlist test passed. |
| `cargo fmt --all --check` | passed | Rust formatting clean. |
| `cargo clippy -p quantsys-domain -p adapter-therundown -p adapter-polymarket-market --all-targets -- -D warnings` | passed | Targeted clippy clean. |
| `cargo build -p adapter-polymarket-market -p adapter-polymarket-user` | passed | Polymarket adapter binaries build. |

Commands not re-run during this rewrite:

| Command | Status | Reason |
| --- | --- | --- |
| `make check` | not rerun | Large aggregate target; targeted Phase 4 and watchlist checks were run fresh. |
| `make test` | not rerun | `make polymarket-test`, domain watchlist, mapping, and local CSV suites were run fresh. |
| `make therundown-test` | not rerun | This report focuses on Phase 4 plus watchlist interaction; TheRundown watchlist tests were run fresh. |
| `make polymarket-public-probe` | not rerun | Public network probe is optional and not a substitute for mock tests. |
| `make polymarket-geoblock-probe` | not rerun | Public network probe is optional; fixture/mock geoblock tests were run. |

## Polymarket Unit Coverage Matrix

| Requirement | Result | Evidence |
| --- | --- | --- |
| Market subscription payload contract | passed | `market_subscription_payload_uses_assets_ids_and_custom_feature_contract` |
| `assets_ids` present | passed | Same test. |
| `asset_ids` forbidden | passed | Same test rejects invalid key. |
| `custom_feature_enabled=true` | passed | Same test. |
| User subscription payload contract | passed | `user_subscription_payload_uses_markets_condition_ids_and_redacts_auth` |
| User subscription uses condition IDs in `markets` | passed | Same test. |
| Auth redaction | passed | Same test checks redacted JSON and credential `Debug`/`Display`. |
| Discovery active/open/sports filter | passed | `discovery_parser_filters_active_open_sports_markets_and_builds_token_cache` |
| Token cache condition/token lookups | passed | Same test. |
| Token cache TTL | passed | Same test. |
| Market `book` parser | passed | `market_ws_parser_dispatches_supported_market_event_types` |
| Market `price_change` parser | passed | Same test. |
| Market `best_bid_ask` parser | passed | Same test. |
| Market `last_trade_price` parser | passed | Same test. |
| Market `tick_size_change` parser | passed | Same test. |
| Market `new_market` parser | passed | Same test. |
| Market `market_resolved` parser | passed | Same test. |
| Unknown market event | passed | `market_ws_parser_preserves_unknown_and_rejects_missing_required_fields` |
| Missing market required field | passed | Same test and integration DLQ path. |
| User order/fill/order_update raw parser | passed | `user_ws_parser_parses_order_fill_and_redacts_secrets_from_raw` |
| Geoblock parser and blocked gate | passed | `geoblock_parser_redacts_ip_and_state_machine_fails_closed_when_blocked` |
| Time parser normal/negative/large offset | passed | `time_probe_parser_calculates_offsets_and_large_offset_warning` |
| Payload hash and RawMessage deterministic construction | passed | `polymarket_payload_hash_and_raw_message_construction_are_deterministic` |
| SourceState stale/auth_missing/market_resolved | passed | `source_state_covers_market_stale_user_auth_missing_and_market_resolved` |

## Integration and Mock Matrix

| Scenario | Result | Evidence |
| --- | --- | --- |
| Discovery builds token cache | passed | `discovery_builds_token_cache_and_publishes_raw_polymarket_market` |
| Discovery publishes `raw.polymarket.market` | passed | Same test asserts topic. |
| Discovery missing token ids to DLQ | passed | `discovery_missing_token_ids_goes_to_dlq_without_publish` |
| Token cache to market subscription | passed | `token_cache_constructs_market_ws_subscription_with_assets_ids` |
| Market WS supported events publish raw | passed | `market_ws_events_publish_raw_and_market_resolved_updates_source_state` |
| Market resolved updates SourceState | passed | Same test. |
| Unknown market event kept raw | passed | `unknown_or_missing_market_ws_schema_goes_to_raw_or_dlq` |
| Missing required field to DLQ | passed | Same test. |
| User order/fill to `raw.polymarket.user` | passed | `user_ws_order_and_fill_publish_raw_polymarket_user_without_credentials_in_payload` |
| User secrets redacted from raw | passed | Same test. |
| Geoblock blocked/allowed happy path | passed | `geoblock_and_time_probes_publish_raw_and_update_source_state` |
| Time happy path | passed | Same test. |
| PING/PONG/stale/backoff state | passed at state level | `ping_pong_stale_detection_reconnect_backoff_and_rate_limit_state` |
| User auth missing does not break market adapter | passed | `user_auth_missing_is_disabled_without_failing_market_adapter` |
| 1k mock publish smoke | passed | `raw_publish_path_handles_1k_market_messages_with_mock_p95_under_50ms` |

## Raw Topic Validation

### `raw.polymarket.market`

Passed:

- Discovery raw is wrapped as `RawMessage`.
- Market WS raw events are wrapped as `RawMessage`.
- Supported event types include `book`, `price_change`, `best_bid_ask`, `last_trade_price`, `tick_size_change`, `new_market`, and `market_resolved`.
- Publisher routes non-user Polymarket raw events to `raw.polymarket.market`.
- Provider is `polymarket`.
- Source channels include `rest_discovery`, `ws_market`, `rest_geoblock`, and `rest_time`.
- `payload_hash` and `raw_id` are deterministic.

Not implemented, by design:

- `norm.quote`
- `mapping.decision`
- `signal.event`
- `order.intent`
- `risk.decision`
- `execution.request`

### `raw.polymarket.user`

Passed:

- User order/order_update raw is wrapped as `RawMessage`.
- User fill raw is wrapped as `RawMessage`.
- Publisher routes user-channel events to `raw.polymarket.user`.
- Missing user credentials produce `auth_missing` and do not fail market ingestion.
- Auth payload secrets are not copied into raw output.

Weak coverage:

- Unknown user event raw publish exists in parser behavior but lacks a dedicated integration assertion.
- Mock user WS auth success handshake is not covered; subscription/auth payload construction is covered.

## SourceState Validation

| State | Result | Notes |
| --- | --- | --- |
| `polymarket_market ok` | passed | Market handlers set ok on supported messages. |
| `polymarket_market stale` | passed | Stale detection test. |
| `polymarket_market rate_limited` | passed | Endpoint budget state test. |
| `polymarket_market schema_error` | passed | Unknown/missing schema tests. |
| `polymarket_market market_resolved` | passed | Unit and integration tests. |
| `polymarket_user ok` | passed | User order/fill integration. |
| `polymarket_user disabled` | partial | Initial state is disabled; no dedicated assertion in current report run. |
| `polymarket_user auth_missing` | passed | Unit/integration and user CLI behavior. |
| `polymarket_user auth_failed` | weak | State function exists; no mock auth-failure test. |
| `polymarket_user stale` | passed | Stale detection test. |
| `polymarket_geoblock ok` | passed | Allowed fixture path. |
| `polymarket_geoblock blocked` | passed | Blocked fixture path. |
| `polymarket_geoblock unknown` | partial | Transport errors set unknown; malformed successful response still needs direct adapter-state coverage. |
| `polymarket_time ok` | passed | Time fixture path. |
| `polymarket_time degraded` | partial | Large offset path exists; malformed successful response needs direct adapter-state coverage. |

Rules still held:

- `execution_enabled` defaults to `false`.
- Geoblock blocked sets `live_execution_allowed=false`.
- Phase 4 does not enable live execution.
- SourceState errors must not contain secrets.

## Geoblock and Time Probe

| Case | Result | Notes |
| --- | --- | --- |
| Geoblock `blocked=true` | passed | Fixture/unit/integration path. |
| Geoblock `blocked=false` | passed | Fixture/unit/integration path. |
| Geoblock IP redaction | passed | Parser emits `<redacted-ip>`. |
| Geoblock malformed response | partial | Parser rejects malformed payload; adapter-level unknown state needs stronger test/handling. |
| Geoblock network error | partial | Code sets unknown on transport error; direct test should be added. |
| Time normal offset | passed | Fixture/integration path. |
| Time negative offset | passed | Unit parser path. |
| Time large offset warning | passed | Unit parser/state path. |
| Time missing/malformed response | partial | Parser rejects bad time; adapter-level degraded state needs stronger test/handling. |

## Local CSV and Watchlist Validation

Local CSV is not a production archive and not a trading signal. It is an observation aid for comparing TheRundown and Polymarket rows.

Passed:

- Provider folders are separated.
- TheRundown rows can be separated by sportsbook display name.
- Polymarket rows use discovery metadata to avoid `unknown_sport`, `unknown_league`, `unknown_time`, and missing team names when discovery data exists.
- CSV rows distinguish source-generated time and local fetch time.
- Header is written once.
- Append mode does not overwrite.
- `output/local-csv` is ignored.
- Cross-process CSV/index writes use a local lock and atomic JSON writes.
- Empty or abandoned index/lock recovery is tested.
- Secret-like strings are redacted from CSV output.

Watchlist defaults:

- `adapter-therundown --mode ws` now requires the default watchlist unless `--disable-watchlist` is explicitly passed.
- `adapter-polymarket-market --mode market-ws` now requires the default watchlist unless `--disable-watchlist` is explicitly passed.
- Normal `make therundown-csv-run` and `make polymarket-csv-run` therefore capture selected matched markets, not broad full market sets.

## Secret Safety Audit

Result: passed for reviewed Phase 4 paths.

- `.env` and `.env.*` are ignored.
- Config stores auth env var names, not secret values.
- User credentials redact `Debug` and `Display`.
- Auth JSON redaction covers `apiKey`, `secret`, and `passphrase`.
- User raw redaction covers auth, signature-like fields, private-key-like fields, and transaction hashes.
- Geoblock IP is redacted.
- Public probe output does not include secrets.
- No test output from the fresh commands printed secrets.

## No-Order-Call and Phase Boundary Check

Result: passed by static audit and tests.

No Phase 4 adapter code implements or calls:

- create order,
- cancel order,
- signer,
- signed order,
- private key read,
- execution gateway,
- paper broker,
- strategy engine,
- risk engine,
- order intent,
- live execution.

The presence of `tests/fixtures/external/polymarket/create_order_response.json` remains a contract fixture only. It is not used by adapter runtime code to place orders.

## CI Status

By inspection, `.github/workflows/ci.yml` runs:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `make contract-test`
- `make therundown-test`
- `make polymarket-test`

CI does not:

- require real Polymarket user credentials,
- run live user WS,
- call order endpoints,
- require private keys,
- run a long public WS soak.

## Weak or Missing Coverage

These items keep the overall Phase 4 verdict at **PARTIAL** rather than a clean hard-gate PASS:

1. Malformed 2xx geoblock response should set adapter `SourceState=polymarket_geoblock unknown` and remain fail closed. Parser rejection exists, but adapter-state coverage is not strong enough.
2. Missing/malformed 2xx time response should set adapter `SourceState=polymarket_time degraded`. Parser rejection exists, but adapter-state coverage is not strong enough.
3. There is no finite public market WS smoke target that subscribes with `assets_ids`, observes one public message or clean timeout, redacts output, and exits.
4. PING/PONG is covered by service code and state-level tests, but not by a real mock WS handshake.
5. User WS auth success is covered by payload construction, but not by a mock auth-accepted WS handshake.
6. User unknown event raw publish lacks a dedicated integration assertion.
7. Performance smoke checks mock average per-message parser/publish time; it is not a true p95 latency distribution measurement.
8. 1k token cache memory stability is bounded by config/tests but lacks explicit memory measurement.

## Phase 4 Done Assessment

Implemented and verified:

- Polymarket market discovery mock flow.
- Active/open/sports filtering.
- Condition/token cache.
- Market subscription with `assets_ids`.
- Market WS parser for required event types.
- Market raw publish to `raw.polymarket.market`.
- User order/fill raw publish to `raw.polymarket.user`.
- User credential missing path as `auth_missing`.
- Geoblock blocked/allowed happy paths.
- Time normal offset path.
- Local CSV observation path.
- Default matched watchlist capture path.
- Secret redaction.
- No order endpoint, signer, private key, strategy, risk, or execution.

Not fully done for strict hard-gate PASS:

- Adapter-level malformed geoblock/time state handling.
- Finite public market WS smoke command.
- Stronger mock WS handshake coverage.
- Stronger p95/memory smoke measurement.

## Phase 5 Gate

Recommendation: **do not treat Phase 4 as a clean PASS yet**.

It is reasonable to continue local data observation and matched-market capture with the current implementation. It is not yet a strict Phase 5 hard-gate PASS until the weak items above are addressed.

Required before a strict Phase 5 gate:

1. Add tests and behavior for malformed geoblock response to set `polymarket_geoblock unknown` and fail closed.
2. Add tests and behavior for malformed/missing time response to set `polymarket_time degraded`.
3. Add a finite public market WS smoke command or mock-equivalent transport smoke.
4. Add direct user unknown-event and user auth-success mock tests.
5. Clarify or improve the performance smoke so it either measures true p95 or stops claiming p95.

## Live Execution Blockers

Live execution remains blocked by design.

Still required in later phases:

- Phase 5 durable raw archive and replay.
- Phase 6 canonical event/market mapping.
- Phase 8 dry-run validation.
- Paper broker.
- Risk engine.
- Execution audit.
- Signer isolation.
- Execution gateway.
- Live geoblock/stale/auth hard gates.

The current watchlist and CSV outputs are observation and capture controls only. They must not be interpreted as trading signals or execution approval.
