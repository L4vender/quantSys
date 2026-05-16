# Topic Catalog

Topic catalog 的源文件是 `scripts/topic-init/topics.toml`，由 `crates/eventbus` 解析，并由 `scripts/topic-init/topic_init.py` 幂等创建到 Redpanda。Phase 2 只定义 topic metadata，不实现 topic producer/consumer 的业务逻辑。

| Topic | Key | Producer | Consumers | Retention | Partitions |
|---|---|---|---|---:|---:|
| `raw.therundown` | `provider_event_id` | `adapter-therundown` | `normalizer`, `raw-archive`, `replay` | 14d | 3 |
| `raw.polymarket.market` | `provider_market_id` | `adapter-polymarket-market` | `normalizer`, `raw-archive`, `replay` | 14d | 3 |
| `raw.polymarket.user` | `venue_order_id` | `adapter-polymarket-user` | `raw-archive`, `execution-sync` | 90d | 3 |
| `norm.quote` | `canonical_market_key` | `normalizer` | `mapper`, `latency`, `ch-sink` | 14d | 3 |
| `mapping.decision` | `canonical_event_id` | `canonical-mapper` | `api`, `review` | 30d | 3 |
| `latency.sample` | `canonical_market_key` | `latency-engine` | `alert`, `api` | 30d | 3 |
| `signal.event` | `canonical_market_key` | `signal-engine` | `api`, `ch-sink` | 30d | 3 |
| `order.intent` | `intent_id` | `signal-engine` | `risk` | 90d | 3 |
| `risk.decision` | `intent_id` | `risk-engine` | `paper`, `execution`, `api` | 90d | 3 |
| `execution.request` | `venue_account_id` | `risk-manual` | `execution-gateway-pm` | 90d | 3 |
| `execution.receipt` | `venue_order_id` | `execution-user-adapter` | `ledger`, `audit`, `reconcile` | 365d | 3 |
| `paper.fill` | `paper_order_id` | `paper-broker` | `replay`, `api`, `analytics` | 180d | 3 |
| `dlq.raw` | `message_hash` | `any-service` | `operator`, `replay` | 30d | 3 |

## `raw.therundown` Schema

Phase 3 `adapter-therundown` publishes `RawMessage` only. `provider=therundown`; `source_channel` is one of `rest_bootstrap`, `rest_delta`, or `ws_market`; provider ids are copied from TheRundown payload when present; `payload_hash` and `raw_id` are deterministic; `payload` is the raw external JSON and never includes `X-TheRundown-Key`, websocket query `key`, or other auth material. WebSocket `heartbeat` and unknown `meta.type` messages are preserved as raw payloads; missing required fields are sent to the adapter DLQ sink instead of fabricating ids.

## `raw.polymarket.market` Schema

Phase 4 `adapter-polymarket-market` publishes `RawMessage` only. `provider=polymarket`; `source_channel` is one of `rest_discovery`, `ws_market`, `rest_geoblock`, or `rest_time`. Discovery raw wraps the Gamma events response and uses the first discovered condition/token for stable ids. Market WS raw preserves `book`, `price_change`, `best_bid_ask`, `last_trade_price`, `tick_size_change`, `new_market`, `market_resolved`, and unknown `event_type` payloads. `payload_hash` and `raw_id` are deterministic; `provider_event_id` is the condition id when present; `provider_market_id` is the asset/token id when present. Missing required market fields go to the adapter DLQ sink.

## `raw.polymarket.user` Schema

Phase 4 `adapter-polymarket-user` publishes `RawMessage` only. `provider=polymarket`; `source_channel=ws_user`; payloads include raw user `order`, `order_update`, `trade`/`fill`, and unknown user channel events. The adapter treats this as read-only state for future reconciliation. `apiKey`, `secret`, `passphrase`, `signature`, transaction hashes, and private-key-like fields are redacted before entering `RawMessage`, logs, errors, fixtures, or DLQ records. Missing real user credentials sets `SourceState.status=auth_missing` and does not fail market ingestion or CI.

## Phase 2 约束

- `order.intent`、`risk.decision`、`execution.*`、`paper.fill` 在 Phase 2 只是未来链路的 topic metadata。
- Phase 2 不发布 order intent，不评估 risk，不生成 paper fill，不提交 Polymarket order。
- 所有 producer/consumer 需要在后续阶段以幂等 key 和 offset checkpoint 方式实现。
