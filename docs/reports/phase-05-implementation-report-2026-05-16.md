# Phase 5 Implementation Report - 2026-05-16

## Summary

Phase 5 adds raw archive, DLQ, source health, rate budget, and consumer lag foundations. The implementation remains offline-safe and does not add normalized quotes, mapping, strategy, risk, paper broker, signer, execution gateway, order endpoints, or frontend trading pages.

## Added Or Modified Files

- `crates/domain/src/raw.rs`, `source.rs`, `dlq.rs`
- `crates/storage/src/object_archive.rs`, `raw_index.rs`, `dlq_store.rs`, `source_state_store.rs`, `rate_budget_store.rs`, `consumer_lag_store.rs`
- `services/raw-archive/`
- `services/source-health/`
- `migrations/postgres/0002_raw_archive_index.sql` through `0006_consumer_lag.sql`
- `migrations/clickhouse/0002_raw_events.sql`
- `tests/fixtures/raw/`
- `docs/schema/raw-event.md`
- `docs/runbooks/source-health.md`
- `Makefile`, `.github/workflows/ci.yml`, topic catalog metadata

## Flows

Raw archive flow: eventbus envelope -> RawMessage parse -> topic/provider/channel validation -> deterministic payload hash and raw id check -> secret scan -> object archive write -> archive index upsert -> source state latest update.

DLQ flow: malformed or invalid message -> classify error -> write DLQ object key -> store DLQ metadata. DLQ errors are isolated from later good messages.

Source health flow: read source state snapshots, rate budgets, consumer lag, DLQ list, and raw index metadata through read-only HTTP endpoints.

Rate budget flow: endpoint-scoped snapshots keep limit, remaining, reset, Retry-After, update time, and status.

Consumer lag flow: topic/group/partition snapshots compute `high_watermark - last_consumed_offset` and expose lagging checks.

## Test Results

Executed:

```text
cargo test -p quantsys-domain --test raw_phase5
cargo test -p quantsys-storage --test raw_archive_phase5
cargo test -p raw-archive --test raw_archive_flow
cargo test -p source-health --test source_health_api
cargo test -p raw-archive
cargo test -p source-health
cargo test --workspace
make contract-test
make fmt
make clippy
make test
make therundown-test
make polymarket-test
make phase5-test
make check
make compose-up
make migrate-local
make topic-init
make raw-archive-integration-test
make source-health-integration-test
make compose-down
make raw-archive-bench
```

All passed locally. The raw archive smoke archived 1,000 in-memory messages in about 0.11s in the observed run, exceeding 1k msg/s. Archive write P95 is not separately exported yet; the smoke records total elapsed time only.

## Backend Support

ObjectArchive supports in-memory and local filesystem read/write with idempotency and batch partial failure reporting. S3-compatible/MinIO has config and interface coverage; full client wiring remains a TODO. Docker compose, migrations, and topic init were verified locally; raw-archive/source-health integration tests still use in-memory stores by design.

## Not Complete

- Real PostgreSQL, Redis, Redpanda, and MinIO clients are not wired into the services yet.
- Docker integration targets are present as Makefile hooks but this implementation primarily validates in-memory/local backends.
- Source-health API returns archive metadata by `raw_ref`; full payload read still belongs to raw-archive/object archive.

## Phase 6 Decision

The offline Phase 5 acceptance path is satisfied for raw archive, deterministic ids, idempotent duplicates, DLQ isolation, source state snapshots, rate budget snapshots, consumer lag snapshots, local object archive, migrations, docs, and make targets.

Phase 6 mapping remains blocked from live trading by design. Phase 8 dry-run and live execution remain blocked until their own phases implement normalization, mapping review, strategy, risk, paper broker, signer, and execution gateway.
