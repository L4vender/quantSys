# Phase 3 Validation Report - TheRundown Ingestion

## Summary

| Item | Result |
|---|---|
| Validation time | 2026-05-16 02:38:39 CST |
| Commit | `6f220a0` |
| Scope | Phase 3 TheRundown ingestion only |
| Overall conclusion | PASS |
| Phase 4 entry | Allowed |

Phase 3 was validated as a TheRundown-only ingestion phase. The implementation produces `raw.therundown` `RawMessage` events through a mock producer, updates TheRundown `SourceState`, handles REST and WS parser failure modes, and does not implement Polymarket ingestion, mapping, strategy, risk, paper broker, signer, or execution.

Non-blocking weak spots are listed at the end of this report.

## Validation Scope

Reviewed:

- `README.md`
- `docs/3_development_phases.md`
- `docs/development-phases/phase-03-therundown-ingestion.md`
- `docs/1_external_api_contract_spike.md`
- `docs/reports/external-api-contract-spike-2026-05-15.md`
- `docs/adapters/api-contract-baseline.md`
- `docs/adapters/therundown.md`
- `docs/schema/topic-catalog.md`
- `configs/sources/therundown.example.toml`
- `services/adapter-therundown/`
- `crates/source-sdk/`
- `crates/domain/`
- `crates/eventbus/`
- `crates/config/`
- `crates/telemetry/`
- `crates/test-support/`
- `tests/fixtures/external/therundown/`
- `tests/contract/`
- `Makefile`
- `.github/workflows/ci.yml`

## Static Audit

Required paths were present:

| Path | Result |
|---|---|
| `services/adapter-therundown/` | present |
| `crates/source-sdk/src/therundown/` | present |
| `docs/adapters/therundown.md` | present |
| `tests/fixtures/external/therundown/events_bootstrap.json` | present |
| `tests/fixtures/external/therundown/markets_delta.json` | present |
| `tests/fixtures/external/therundown/ws_market_price.json` | present |
| `tests/fixtures/external/therundown/ws_heartbeat.json` | present |
| `tests/fixtures/external/therundown/rate_limit_headers.json` | present |
| `tests/fixtures/external/therundown/off_board_price.json` | present |
| `configs/sources/therundown.example.toml` | present |
| TheRundown tests | `crates/source-sdk/tests/therundown_unit.rs`, `crates/source-sdk/tests/therundown_integration.rs` |

Required Makefile targets were present and non-empty:

| Target | Result |
|---|---|
| `contract-test` | present, runs contract Python smoke |
| `fmt` | present, runs `cargo fmt --all --check` |
| `clippy` | present, runs workspace clippy with `-D warnings` |
| `test` | present, runs `cargo test --workspace` |
| `check` | present, runs fmt, clippy, test, contract, TheRundown tests |
| `therundown-test` | present, runs unit, integration, adapter tests |
| `therundown-contract-test` | present, runs contract + TheRundown tests |
| `therundown-integration-test` | present, runs TheRundown integration test |
| `therundown-mock` | present, runs mock bootstrap integration test |
| `adapter-therundown` | present, builds adapter binary |
| `therundown-live-probe` | present, requires `THERUNDON_API_KEY` and runs adapter probe |

## Command Results

| Command | Result | Evidence |
|---|---|---|
| `make contract-test` | passed | contract smoke checks passed |
| `make fmt` | passed | `cargo fmt --all --check` exit 0 |
| `make clippy` | passed | workspace clippy finished with `-D warnings` |
| `make test` | passed | workspace tests passed, including TheRundown unit/integration tests |
| `make therundown-test` | passed | 15 unit + 14 integration + adapter health test passed |
| `make therundown-integration-test` | passed | 14/14 TheRundown integration tests passed |
| `make adapter-therundown` | passed | adapter binary built |
| `make check` | passed | fmt, clippy, workspace test, contract, TheRundown tests passed |
| `make therundown-mock` | passed | mock bootstrap publish test passed |
| `make therundown-live-probe` | passed with weak entitlement evidence | command printed sanitized output and no key; fields were `None` |

Live probe output:

```text
therundown probe ok tier=None delay=None websocket_access=None rate_limit=None datapoints_remaining=None live_signal_allowed=false live_execution_allowed=false
```

This proves the command is runnable and secret-safe in the current environment. It does not prove live entitlement headers were returned by the selected probe endpoint.

## Unit Test Coverage Matrix

| Requirement | Result | Evidence |
|---|---|---|
| REST URL construction | passed | `therundown_rest_url_construction_uses_v2_contract_paths` |
| Header auth construction | passed | `therundown_auth_header_and_debug_never_expose_api_key` |
| API key not in Debug / Display | passed | `therundown_auth_header_and_debug_never_expose_api_key` |
| entitlement headers parser | passed | `therundown_header_parser_extracts_entitlement_rate_limit_and_datapoints` |
| `X-Tier` parser | passed | same header parser test |
| `X-Rate-Limit` parser | passed | same header parser test |
| `X-Data-Delay-Seconds` parser | passed | same header parser test |
| `X-Websocket-Access` parser | passed | same header parser test |
| `X-Datapoints-*` parser | passed | same header parser test |
| `Retry-After` parser | passed | `therundown_retry_after_parser_accepts_seconds_and_http_dates` |
| rate budget exhausted | passed | `therundown_rate_and_datapoint_budget_exhaustion_are_detected` |
| reconnect backoff + jitter | passed | `therundown_backoff_uses_exponential_delay_with_bounded_jitter` |
| heartbeat stale detector | passed | `therundown_heartbeat_stale_detector_marks_stale_after_threshold` |
| delta cursor update | passed | `therundown_delta_cursor_updates_and_decides_stale_recovery` |
| cursor stale recovery decision | passed | same cursor test |
| payload hash deterministic | passed | `therundown_payload_hash_and_raw_message_construction_are_deterministic` |
| `RawMessage` construction | passed | same raw message test |
| `meta.type=market_price` dispatch | passed | `therundown_parser_dispatches_market_price_heartbeat_and_unknown_types` |
| `meta.type=heartbeat` dispatch | passed | same parser dispatch test |
| unknown `meta.type` handling | passed | same parser dispatch test |
| missing required field error | passed | `therundown_missing_required_market_price_field_returns_schema_error` |
| off-board sentinel marker | passed | `therundown_off_board_sentinel_sets_raw_marker_only` |
| SourceState delayed gate | passed | `therundown_source_state_gates_delayed_no_ws_stale_and_datapoints` |
| SourceState no websocket access gate | passed | same SourceState gate test |
| SourceState stale gate | passed | same SourceState gate test |
| SourceState datapoints exhausted gate | passed | same SourceState gate test |
| TheRundown `live_execution_allowed=false` | passed | SourceState base implementation and tests assert false in key states |
| secret scrubber for TheRundown key patterns | passed | `therundown_secret_scrubber_removes_keys_and_query_params` |

## Integration / Mock Coverage Matrix

| Scenario | Result | Evidence |
|---|---|---|
| REST 200 entitlement | passed via bootstrap headers; weak direct probe coverage | `mock_rest_bootstrap_publishes_raw_therundown_and_updates_cursor` uses 200 with entitlement headers; no direct mock `probe()` test |
| REST events bootstrap 200 | passed | `mock_rest_bootstrap_publishes_raw_therundown_and_updates_cursor` |
| REST markets delta 200 | passed | `mock_market_delta_publishes_raw_therundown_and_advances_cursor` |
| REST 401 auth failed | passed | `mock_401_sets_auth_failed_and_does_not_retry` |
| REST 429 Retry-After | passed | `mock_429_applies_retry_after_and_sets_rate_limited_without_storm` |
| REST 5xx | passed | `mock_5xx_uses_backoff_state_and_keeps_secret_scrubbed` |
| datapoints remaining = 0 | passed | `datapoints_delay_and_no_ws_headers_update_source_state` |
| data delay seconds > 0 | passed | same datapoints/delay/no-ws test |
| websocket access false | passed | same datapoints/delay/no-ws test |
| WS heartbeat | passed | `mock_ws_heartbeat_updates_source_state` |
| WS market_price | passed | `mock_ws_market_price_publishes_raw_therundown` |
| WS unknown `meta.type` | passed | `mock_unknown_ws_type_is_preserved_as_raw_unknown_schema` |
| WS missing required field | passed | `mock_missing_required_ws_field_goes_to_dlq_without_publish` |
| WS stale / timeout | passed at detector/backoff level | `mock_stale_marks_source_and_computes_reconnect_backoff`; no network WS mock server |
| cursor stale triggers bootstrap | passed | `mock_cursor_stale_triggers_bootstrap_recovery` |
| REST bootstrap -> RawMessage -> raw producer | passed | `mock_rest_bootstrap_publishes_raw_therundown_and_updates_cursor` |
| market delta -> RawMessage -> raw producer | passed | `mock_market_delta_publishes_raw_therundown_and_advances_cursor` |
| WS heartbeat updates SourceState | passed | `mock_ws_heartbeat_updates_source_state` |
| WS market_price publishes RawMessage | passed | `mock_ws_market_price_publishes_raw_therundown` |
| 429 applies Retry-After and no reconnect storm | passed at state/no-retry level | stores `retry_after=2s` and publishes nothing; no long-running reconnect storm test |
| 401 sets auth_failed | passed | `mock_401_sets_auth_failed_and_does_not_retry` |
| stale triggers reconnect/backoff | passed | `mock_stale_marks_source_and_computes_reconnect_backoff` |
| fixture replay `ws_market_price.json` | passed | `fixture_replay_ws_market_and_off_board_publish_raw_only` |
| fixture replay `off_board_price.json` | passed | same fixture replay test |

## `raw.therundown` Output Validation

Validated behavior:

- REST bootstrap output is a `RawMessage`.
- REST delta output is a `RawMessage`.
- WS `market_price` output is a `RawMessage`.
- `RawMessage.provider = therundown`.
- `RawMessage.source_channel` is `rest_bootstrap`, `rest_delta`, or `ws_market`.
- `provider_event_id` is copied from TheRundown `event_id` when present.
- `provider_market_id` is copied from TheRundown `market_id` when present.
- `payload_hash` and `raw_id` are deterministic in unit tests.
- `EventEnvelope.topic = raw.therundown` in mock producer tests.

No Phase 3 TheRundown code publishes these downstream topics:

- `norm.quote`
- `mapping.decision`
- `signal.event`
- `order.intent`
- `risk.decision`
- `execution.request`

Those topic names exist only in Phase 2 topic metadata / docs, not as TheRundown adapter outputs.

## SourceState Validation

Covered states:

| State | Result | Evidence |
|---|---|---|
| `ok` | passed | bootstrap, WS heartbeat, WS market tests |
| `degraded` | passed | 5xx test |
| `stale` | passed | stale detector/backoff test |
| `rate_limited` | passed | 429 test |
| `auth_failed` | passed | 401 test |
| `data_delay_detected` | passed | delayed header test |
| `no_websocket_access` | passed | no websocket access unit/integration coverage |
| `datapoints_exhausted` | passed | datapoints remaining zero test |
| `cursor_stale` | passed | cursor stale recovery path |
| `schema_error` | passed | unknown meta type and missing required field tests |

Gate rules:

- `X-Data-Delay-Seconds > 0` sets `live_signal_allowed=false`.
- `X-Websocket-Access=false` sets `live_signal_allowed=false`.
- missing/unknown headers conservatively set `live_signal_allowed=false`; live probe observed this with all entitlement fields `None`.
- stale source sets `live_signal_allowed=false`.
- datapoints exhausted sets `live_signal_allowed=false`.
- TheRundown state base always sets `live_execution_allowed=false`.
- `block_reason` values are structured strings such as `delayed_source`, `no_websocket_access`, `datapoints_exhausted`, `stale_source`, `rate_limited`, `cursor_stale`, `unknown_schema`, and `missing_required_field`.

Weak point: `mark_ok_message` currently updates `last_heartbeat_at` for both heartbeat and market messages. This still satisfies stale detection because the Phase 3 stale rule allows heartbeat or market messages, but the field name is semantically loose.

## Secret Safety Audit

Findings:

- `ApiKey` Debug / Display redacts the key.
- REST transport scrubs transport and JSON parse errors.
- WS URL is printed through `redact_ws_url`.
- `RawMessage` payload is built from provider JSON, not auth headers or URL query strings.
- DLQ structured error does not include the key in tested paths.
- `.env` and `.env.*` are ignored by `.gitignore`; `.env.example` is allowed.
- `output/` exists and is ignored. A scan found only sanitized live-mapping entitlement metadata and key env names, not raw keys.
- No `logs/` directory exists.

Known compatibility issue:

- `.env.example` uses `THERUNDOWN_API_KEY`.
- `configs/sources/therundown.example.toml`, `Makefile`, and Phase 1 report currently use `THERUNDON_API_KEY`.
- This is a naming mismatch for live probe usability. It is not a secret leak and does not affect CI/mock tests.

## Live Probe

`make therundown-live-probe` ran in the current environment and exited 0. It printed no API key and wrote no fixture. The output was sanitized but did not include concrete entitlement values:

```text
tier=None delay=None websocket_access=None rate_limit=None datapoints_remaining=None
```

This is weak live entitlement evidence. Mock tests remain the Phase 3 acceptance basis.

## Performance Smoke

| Requirement | Result | Evidence |
|---|---|---|
| mock 1k msg/s raw publish path | passed | `raw_publish_path_handles_1k_messages_with_local_p95_under_20ms` publishes 1,000 messages |
| WS parser + enqueue under 20ms | weak pass | test asserts average per-message time under 20ms, not true P95 |
| reconnect storm does not exceed budget | weak pass | 429 and stale tests verify no tight immediate retry and backoff state; no long-running storm simulation |

## Documentation Consistency

`docs/adapters/therundown.md` contains:

- Phase 3 scope.
- Adapter architecture.
- REST bootstrap flow.
- markets delta flow.
- WebSocket flow.
- subscription filter rules.
- 256-message buffer risk handling.
- headers / entitlement parsing.
- SourceState state machine.
- `raw.therundown` schema.
- cursor recovery strategy.
- 429 / Retry-After handling.
- 401 / 5xx handling.
- stale / reconnect handling.
- off-board sentinel handling.
- secret scrub rules.
- mock testing instructions.
- adapter and test commands.
- Phase 4 / 5 / 6 boundaries.
- explicit non-goals: Polymarket, normalized quote, mapping, strategy, risk, execution.

No material code/document mismatch was found. Minor mismatch: documentation says live probe prints entitlement summary; current live run printed the fields but values were `None`.

## CI Validation

`.github/workflows/ci.yml` runs:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `make contract-test`
- `make therundown-test`
- topic init dry-run

CI does not run:

- live TheRundown API.
- `make therundown-live-probe`.
- long-running WS soak.
- any command that requires a real API key.

## Phase Boundary Check

The Phase 3 implementation does not add real business capability for:

- Polymarket adapter.
- Polymarket user channel.
- Polymarket order endpoint.
- signer.
- execution gateway.
- paper broker.
- risk engine.
- strategy engine.
- edge calculation.
- no-vig / odds conversion as normalized quote.
- mapping decision.
- order intent.
- frontend trading UI.
- live execution.

References to later-phase topics and services remain in Phase 2 topic metadata and architecture docs only.

## Failed / Missing / Weak Coverage

Blocking failures: none.

Weak or follow-up coverage:

1. Direct mock `probe()` / REST 200 entitlement test is not present; entitlement headers are covered through bootstrap responses and parser unit tests.
2. `make therundown-live-probe` ran safely, but the current probe endpoint returned `None` for entitlement values.
3. `THERUNDON_API_KEY` vs `THERUNDOWN_API_KEY` naming mismatch should be fixed or aliased before operator-facing live probes.
4. Performance smoke measures average per-message latency, not true P95.
5. WS network client is statically present and CLI-wired, but integration tests call `handle_ws_json` directly rather than using a mock WebSocket server.
6. `last_heartbeat_at` is updated for both heartbeat and market messages; stale behavior is correct for Phase 3, but the field is semantically broad.

## Phase Decision

Phase 3 status: PASS.

Allowed to enter Phase 4: yes.

Rationale: all mandatory mock ingestion flows pass, `raw.therundown` publication is verified, SourceState gates are verified, REST/WS parser error modes are covered, CI is offline-safe, secrets are not exposed, and no later-phase trading capability was implemented.

Before Phase 5 raw archive / durable DLQ, fix or revisit:

- durable Redpanda producer and archive sink.
- durable DLQ topic wiring.
- stricter endpoint pause semantics for `Retry-After`.
- true P95 parser/enqueue measurement.
- direct mock probe test.

Live execution remains blocked by future phases: raw archive health, normalization/mapping, dry-run signals, risk, paper broker, execution contract mock, geoblock, audit, and live operations gates.
