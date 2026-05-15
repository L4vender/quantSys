# 外部 API 契约校准 Spike

本文档定义适配器开发前必须完成的外部 API 契约校准。目标是先用官方文档、真实探测和脱敏 fixture 固化 TheRundown 与 Polymarket 的字段、权限、限流和降级规则，再进入业务服务实现。

## 1. 目标

1. 确认 TheRundown API key 的实时权限、WebSocket 权限、数据点配额、突发限流和数据延迟。
2. 固化 TheRundown V2 REST、delta、WebSocket `market_price`、`heartbeat` 的 payload schema。
3. 固化 Polymarket market/user WebSocket 订阅字段、心跳、事件类型、geoblock 和 CLOB 订单契约。
4. 生成后续 adapter 必跑的 fixture、schema、contract test 清单和源配置样例。
5. 任何不确定项标记为“待确认”，并在代码开发前给出保守降级策略。

## 2. 官方来源

| 平台 | 契约点 | 官方文档 |
|---|---|---|
| TheRundown | V2 WebSocket、`meta.type`、`market_price`、heartbeat、256-message buffer | [WebSocket](https://docs.therundown.io/api-reference/v2/websocket) |
| TheRundown | 数据点、RPS、套餐延迟、`X-Websocket-Access` | [Rate Limits](https://docs.therundown.io/rate-limits) |
| TheRundown | events bootstrap、canonical `event_id`、market/participant shape | [Events](https://docs.therundown.io/api-reference/v2/events) |
| TheRundown | market delta、`last_id`、30 分钟 stale cursor | [Markets Delta](https://docs.therundown.io/api-reference/generated/v2-markets/get-market-price-changes-since-a-given-id) |
| Polymarket | market channel、`assets_ids`、`custom_feature_enabled`、PING/PONG | [Market Channel](https://docs.polymarket.com/api-reference/wss/market) |
| Polymarket | user channel、`markets` condition IDs、API key/secret/passphrase | [User Channel](https://docs.polymarket.com/api-reference/wss/user) |
| Polymarket | endpoint 级限流 | [Rate Limits](https://docs.polymarket.com/quickstart/introduction/rate-limits) |
| Polymarket | geoblock endpoint 与 blocked response | [Geographic Restrictions](https://docs.polymarket.com/api-reference/geoblock) |
| Polymarket | FAK/FOK/GTC/GTD 与 marketable limit order | [Order Types](https://docs.polymarket.com/developers/CLOB/orders/onchain-order-info) |
| Polymarket | L1/L2 headers、`POLY_1271`、deposit wallet | [Authentication](https://docs.polymarket.com/developers/CLOB/authentication) |

## 3. 必须产出的工程工件

| 工件 | 路径 | 内容 |
|---|---|---|
| 契约基线报告 | `docs/reports/external-api-contract-spike-YYYY-MM-DD.md` | API key 权限、套餐、延迟、WS access、geoblock、限流观测、字段差异、待确认项。 |
| TheRundown fixture | `tests/fixtures/external/therundown/` | `events_bootstrap.json`、`markets_delta.json`、`ws_market_price.json`、`ws_heartbeat.json`、`rate_limit_headers.json`、`off_board_price.json`。 |
| Polymarket fixture | `tests/fixtures/external/polymarket/` | `market_subscribe.json`、`market_book.json`、`market_price_change.json`、`market_best_bid_ask.json`、`user_order_update.json`、`geoblock_blocked.json`、`create_order_response.json`。 |
| Contract manifest | `tests/contract/external_api_contract_manifest.yaml` | 每个 fixture 的来源 URL、采集时间、脱敏字段、schema version、阻塞级别。 |
| Source config 样例 | `configs/sources/*.example.toml` | TheRundown filter、rate budget、data-point budget、Polymarket token shard、WS heartbeat、reconnect/backoff。 |
| Adapter 契约说明 | `docs/adapters/api-contract-baseline.md` | 外部字段到内部 `RawMessage`、`NormalizedQuote`、`SourceState` 的映射表。 |

## 4. TheRundown 校准清单

| 检查项 | 验收口径 | 阻塞范围 |
|---|---|---|
| Auth | REST 使用 `X-TheRundown-Key`，WS 使用 query `key`，secret 不进入日志和 fixture。 | 所有 adapter |
| Entitlement headers | 记录 `X-Tier`、`X-Rate-Limit`、`X-Data-Delay-Seconds`、`X-Websocket-Access`、`X-Datapoints-*`。 | live signal |
| Real-time gate | `X-Data-Delay-Seconds > 0` 或 `X-Websocket-Access=false` 时，live 主信号禁用。 | live signal |
| WS message shape | 以 `meta.type` 分发；`market_price` 解析 `data.id`、`event_id`、`affiliate_id`、`market_id`、`market_participant_id`、`normalized_market_participant_id`、`line`、`price`、`previous_price`、`is_main_line`、`sport_id`、`updated_at`。 | adapter |
| Heartbeat | 15 秒 heartbeat；30 秒内无任意消息视为 stale 并重连。 | source health |
| Buffer pressure | 记录 256-message buffer 风险；订阅必须使用 `sport_ids`、`market_ids`、`affiliate_ids` 或 `event_ids` 过滤。 | production |
| REST bootstrap | `/api/v2/sports/{sportID}/events/{date}` 使用 canonical `event_id`，保存 `meta.delta_last_id`。 | discovery |
| Delta cursor | `/api/v2/markets/delta` 使用 `last_id`，cursor stale 后必须重新 bootstrap。 | recovery |
| Off-board sentinel | price `0.0001` 进入 `quality_flags.off_board`，不得参与概率和信号计算。 | normalizer |

## 5. Polymarket 校准清单

| 检查项 | 验收口径 | 阻塞范围 |
|---|---|---|
| Market WS subscription | 字段固定为 `assets_ids`；需要 `best_bid_ask`、`new_market`、`market_resolved` 时设置 `custom_feature_enabled: true`。 | market adapter |
| Market WS heartbeat | market/user channel 客户端每 10 秒发送 `PING`，未收到 `PONG` 或消息超阈值则重连。 | source health |
| Market events | 至少固化 `book`、`price_change`、`last_trade_price`、`tick_size_change`、`best_bid_ask` fixture；未知事件进入 raw archive 和 schema alert。 | parser |
| User WS subscription | 使用 `auth.apiKey`、`auth.secret`、`auth.passphrase` 与 `markets` condition IDs；secret 字段必须脱敏。 | user adapter |
| Rate limits | 按官方 Rate Limits 页配置域和端点级 token bucket，不使用单一全局限制。 | production |
| Geoblock | 下单前调用 `GET https://polymarket.com/api/geoblock`；`blocked=true` 时 live execution fail closed。 | live execution |
| Order type | P0 live 使用 marketable limit + `FAK`；必须处理 partial fill、cancelled remainder、unknown status 和 reconcile。 | live execution |
| Auth/signing | deposit wallet / `POLY_1271` 为默认路径；L1/L2 headers、API key derivation 和 signing fixture 必须先 mock 通过。 | live execution |

## 6. Contract Test 验收

| 测试 | 最低断言 |
|---|---|
| Fixture schema validation | 所有 fixture 能解析为 `RawMessage`，保留 raw payload hash、provider、provider event id、received timestamp。 |
| Unknown field tolerance | 新增字段不失败，缺失关键字段进入 DLQ 并记录 schema error。 |
| Secret scrubber | API key、secret、passphrase、private key、signature 不出现在日志、fixture、报告。 |
| Source state gate | delayed、no_ws、stale、rate_limited、geoblocked 均能生成明确 `SourceState` 与 risk block reason。 |
| Replay determinism | 同一 fixture 重放生成相同 raw id/hash 和 parser output。 |
| Rate budget simulation | mock 429、Retry-After、data-point remaining=0、WS stale 时不会重连风暴。 |

## 7. 完成门槛

1. `docs/reports/external-api-contract-spike-YYYY-MM-DD.md` 已记录官方核验、真实探测或无法探测原因。
2. `tests/fixtures/external/` 与 `tests/contract/external_api_contract_manifest.yaml` 已存在，且没有真实 secret。
3. [interface-document](interface-document.md) 已同步官方字段差异，尤其是 TheRundown `meta.type` 和 Polymarket `assets_ids`。
4. [3_development_phases](3_development_phases.md) 的数据采集阶段引用本 spike 的产物，不再写“官方 fixture 待确认”。
5. 若无法获得实时 TheRundown 权限或 Polymarket 合规交易环境，后续阶段只能进入 dry-run/paper/replay，不允许 live execution。

## 8. Phase 1 产物

本仓库当前 Phase 1 产物：

- 契约报告：[`docs/reports/external-api-contract-spike-2026-05-15.md`](reports/external-api-contract-spike-2026-05-15.md)
- Adapter 契约基线：[`docs/adapters/api-contract-baseline.md`](adapters/api-contract-baseline.md)
- Contract manifest：[`tests/contract/external_api_contract_manifest.yaml`](../tests/contract/external_api_contract_manifest.yaml)
- TheRundown fixtures：[`tests/fixtures/external/therundown/`](../tests/fixtures/external/therundown/)
- Polymarket fixtures：[`tests/fixtures/external/polymarket/`](../tests/fixtures/external/polymarket/)
- Source config examples：[`configs/sources/`](../configs/sources/)
- 验收命令：`make contract-test`

## 9. 非目标

本 spike 不实现策略、风控、paper broker、真实下单或前端页面；它只负责把外部 API 的真实契约变成可测试、可追溯、可回归的输入。
