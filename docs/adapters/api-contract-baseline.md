# Adapter API Contract Baseline

本文档是 Phase 1 外部 API 契约校准后的 adapter 输入输出基线。它不实现 adapter、策略、风控、paper broker、真实下单或前端页面，只固定后续 Phase 3/4 必须遵守的 raw、normalized 与 source health 契约。

## 1. RawMessage

`RawMessage` 是所有外部 payload 入系统后的第一层内部表示。所有外部消息必须先 raw archive，再进入解析流程。

| 字段 | 含义 |
|---|---|
| `raw_id` | 内部稳定 raw id，可由 provider、channel、provider ids、payload hash 派生。 |
| `provider` | `therundown` 或 `polymarket`。 |
| `source_channel` | `rest_bootstrap`、`rest_delta`、`ws_market`、`ws_user`、`rest_geoblock`、`rest_clob`。 |
| `provider_message_id` | 外部消息或价格变化 id，例如 TheRundown `data.id`。 |
| `provider_event_id` | 外部 event/condition id，例如 TheRundown `event_id` 或 Polymarket condition id。 |
| `provider_market_id` | 外部 market id、asset id、token id 或 condition id，按 provider 原样保存。 |
| `received_at` | wall-clock 接收时间。 |
| `received_mono_ns` | 单调时钟接收时间，用于本机延迟和顺序判断。 |
| `payload_hash` | canonical JSON payload hash，用于幂等与 replay。 |
| `raw_ref` | raw archive object key 或 DLQ 引用。 |
| `schema_version` | fixture/adapter schema version。 |
| `trace_id` | 贯穿 raw -> normalized -> mapping -> signal/risk/execution 的 trace id。 |
| `payload` | 原始 JSON payload，日志中不得直接打印。 |

## 2. NormalizedQuote

`NormalizedQuote` 是后续 mapping、latency、dry-run signal 的行情输入。Phase 1 只定义字段，不实现转换逻辑。

| 字段 | 含义 |
|---|---|
| `quote_id` | 内部 quote id。 |
| `provider` | `therundown` 或 `polymarket`。 |
| `canonical_market_key` | canonical 市场 key，Phase 6 映射后填充。 |
| `canonical_event_id` | canonical event id，Phase 6 映射后填充。 |
| `provider_event_id` | provider 原始 event/condition id。 |
| `provider_market_id` | provider 原始 market/asset/condition id。 |
| `provider_participant_id` | provider 原始 participant/outcome id。 |
| `normalized_participant_id` | TheRundown normalized participant id 或 Polymarket outcome token 映射结果。 |
| `sport` | sport code/name。 |
| `market_type` | P0 只允许 `moneyline` 进入 live；spread/total 不进入 P0 live。 |
| `period` | P0 只允许 `full_game`。 |
| `side` | `home`、`away`、`yes`、`no` 或映射后的 canonical side。 |
| `line` | 盘口 line；moneyline 可为 null。 |
| `raw_price` | provider 原始价格，例如 American odds 或 Polymarket decimal probability price。 |
| `normalized_probability` | 标准化概率；off-board sentinel 不得生成该值。 |
| `best_bid` | Polymarket bid 或可推导 best bid。 |
| `best_ask` | Polymarket ask 或可推导 best ask。 |
| `size` | 顶层价格对应 size/depth。 |
| `provider_ts` | provider payload timestamp。 |
| `ingest_ts` | ingestion wall-clock timestamp。 |
| `ingest_mono_ns` | ingestion monotonic timestamp。 |
| `raw_ref` | 指回 `RawMessage.raw_ref`。 |
| `quality_flags` | `off_board`、`delayed_source`、`stale`、`unknown_schema`、`missing_required_field` 等。 |

## 3. SourceState

`SourceState` 是后续 source health、risk gate 和 operator console 的输入。Phase 1 只定义字段与降级口径。

| 字段 | 含义 |
|---|---|
| `source` | `therundown`、`polymarket_market`、`polymarket_user`、`polymarket_geoblock`。 |
| `mode` | `rest_bootstrap`、`rest_delta`、`live_ws`、`mock`、`paper_only`。 |
| `tier` | TheRundown tier 或 provider plan。 |
| `data_delay_seconds` | TheRundown `X-Data-Delay-Seconds`。unknown 按 delayed 处理。 |
| `websocket_access` | TheRundown `X-Websocket-Access` 或 Polymarket WS auth availability。 |
| `status` | `ok`、`degraded`、`stale`、`rate_limited`、`blocked`、`unknown`。 |
| `last_message_at` | 最近任意消息时间。 |
| `last_heartbeat_at` | 最近 heartbeat/PONG 时间。 |
| `stale_after_seconds` | source stale 阈值。TheRundown/Polymarket baseline 为 30 秒。 |
| `rate_limited` | 当前 endpoint 是否被 429 或预算耗尽限制。 |
| `geoblocked` | Polymarket geoblock 是否阻断。 |
| `error` | 结构化错误 code/message，不能包含 secret。 |
| `live_signal_allowed` | 是否允许作为 live 主信号输入。 |
| `live_execution_allowed` | 是否允许 live execution。 |
| `block_reason` | `delayed_source`、`no_websocket_access`、`stale_source`、`geoblocked`、`rate_limited`、`unknown_schema` 等。 |

## 4. TheRundown Mapping

| Provider 字段/信号 | RawMessage | NormalizedQuote | SourceState |
|---|---|---|---|
| REST `X-TheRundown-Key` | 不入 payload/log；只作为 secret ref | 不适用 | auth success/failure 进入 `status`/`error` |
| WS query `key` | 不入 payload/log；只作为 secret ref | 不适用 | auth success/failure 进入 `status`/`error` |
| `meta.type` | dispatch key | unknown schema flag when unsupported | unknown schema alert |
| `meta.type=market_price` | `source_channel=ws_market` | quote candidate | `last_message_at` |
| `data.id` | `provider_message_id` | quote source id | 不适用 |
| `data.event_id` | `provider_event_id` | `provider_event_id` | 不适用 |
| `data.market_id` | `provider_market_id` | `provider_market_id` | 不适用 |
| `data.market_participant_id` | payload | `provider_participant_id` | 不适用 |
| `data.normalized_market_participant_id` | payload | `normalized_participant_id` | 不适用 |
| `data.line` | payload | `line` | 不适用 |
| `data.price` | payload | `raw_price` -> normalized probability unless sentinel | 不适用 |
| `data.price=0.0001` | payload | `quality_flags.off_board=true` and no probability | `live_signal_allowed=false` for that quote |
| `data.previous_price` | payload | previous raw context | 不适用 |
| `data.is_main_line` | payload | quality/market eligibility | 不适用 |
| `data.sport_id` | payload | `sport` after mapping | 不适用 |
| `data.updated_at` | provider payload | `provider_ts` | source freshness input |
| `heartbeat.data.now` | heartbeat payload | no quote | `last_heartbeat_at` |
| REST events `meta.delta_last_id` | bootstrap cursor | event discovery baseline | recovery cursor |
| Markets delta `last_id` | delta cursor | price change ordering | stale cursor triggers bootstrap |
| `X-Tier` | response metadata | 不适用 | `tier` |
| `X-Rate-Limit` | response metadata | 不适用 | endpoint rate budget |
| `X-Data-Delay-Seconds` | response metadata | quality flag when delayed | `data_delay_seconds` and live gate |
| `X-Websocket-Access` | response metadata | 不适用 | `websocket_access` and live gate |
| `X-Datapoints-*` | response metadata | 不适用 | datapoint budget |
| `429` / `Retry-After` | response metadata | no quote | `rate_limited=true`; backoff until retry time |

## 5. Polymarket Mapping

| Provider 字段/信号 | RawMessage | NormalizedQuote | SourceState |
|---|---|---|---|
| Market WS endpoint | `source_channel=ws_market` | market quote source | `polymarket_market` status |
| User WS endpoint | `source_channel=ws_user` | no market quote | `polymarket_user` status |
| Market subscription `assets_ids` | payload | token/outcome allowlist | parser contract |
| User subscription `markets` | payload | condition id allowlist | user channel health |
| `custom_feature_enabled=true` | payload | enables extended event parsing | parser capability |
| `event_type=book` | provider payload | `best_bid`/`best_ask`/depth | `last_message_at` |
| `event_type=price_change` | provider payload | quote update | `last_message_at` |
| `event_type=last_trade_price` | provider payload | trade reference only unless mapped | `last_message_at` |
| `event_type=tick_size_change` | provider payload | quality/market metadata update | schema alert on mismatch |
| `event_type=best_bid_ask` | provider payload | top-of-book update | requires custom feature |
| `event_type=new_market` | provider payload | discovery metadata | requires custom feature |
| `event_type=market_resolved` | provider payload | market closed flag | live block for resolved market |
| Client `PING` every 10s | no raw quote | no quote | heartbeat probe |
| `PONG` missing | health event | no quote | stale/reconnect |
| Endpoint rate budget | response metadata | quality flag if degraded | per-endpoint limiter, not global |
| Geoblock `blocked=true` | `source_channel=rest_geoblock` | no quote | `geoblocked=true`, `live_execution_allowed=false` |
| User order update | `source_channel=ws_user` | no quote | reconcile/order state input |
| create order response | `source_channel=rest_clob` | no quote | execution receipt mock contract only |
| L1/L2 headers | never logged | no quote | auth readiness only |
| deposit wallet / `POLY_1271` | signer config | no quote | live execution readiness |

## 6. Degradation Rules

| Rule | Required behavior |
|---|---|
| delayed source | `X-Data-Delay-Seconds` missing/unknown/>0 sets `live_signal_allowed=false`; dry-run/replay may continue with `quality_flags.delayed_source`. |
| no websocket access | TheRundown `X-Websocket-Access=false` or unknown disables live primary signal. REST may only support bootstrap/delta for dry-run/paper/replay until confirmed. |
| stale source | If no TheRundown heartbeat or market message arrives for 30 seconds, or Polymarket PONG/message age breaches threshold, mark stale source and disallow signal / execution. |
| geoblocked | Polymarket `blocked=true` or geoblock probe failure in live mode sets `live_execution_allowed=false` and fail closed. |
| 429 / Retry-After | Respect `Retry-After`, pause only the affected endpoint budget, and avoid reconnect storms. |
| data point exhausted | TheRundown `X-Datapoints-Remaining=0` disables live primary signal until reset or manual confirmation. |
| unknown schema | Preserve payload in raw archive, emit schema alert, and do not directly discard the message. |
| missing required field | Route to DLQ with schema error and raw_ref; do not manufacture ids or prices. |
| unknown field | Parser must tolerate additive unknown fields and continue when required fields are present. |
| off-board sentinel | TheRundown `price=0.0001` must set `quality_flags.off_board=true` and must not participate in probability or signal calculations. |
| execution unknown status | Polymarket partial fill, cancelled remainder, timeout, or unknown status requires reconcile before any follow-up action. |

## 7. Non-Goals

Phase 1 does not implement strategy, risk engine, paper broker, real signing, real order submission, frontend pages, TheRundown execution, second execution venue, or spread/total live trading.
