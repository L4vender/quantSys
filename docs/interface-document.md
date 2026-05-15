# Polymarket / TheRundown 延迟信号系统接口文档

核验日期：2026-05-15
来源文档：[1_external_api_contract_spike](1_external_api_contract_spike.md)

## 0. 接口定版

| 领域 | 定版 |
|---|---|
| 控制面 API | Rust Axum 暴露 HTTP REST + WebSocket |
| 数据面事件 | Redpanda topic，使用 Kafka protocol schema |
| 内部 RPC | gRPC 仅用于 RiskService、ExecutionService、ClockService |
| TheRundown REST 认证 | 固定 `X-TheRundown-Key` header |
| TheRundown WS 认证 | 固定 query `key` |
| Polymarket 签名 | P0 固定 deposit wallet / `POLY_1271` |
| Live order | P0 固定 marketable limit + FAK |
| 危险操作鉴权 | JWT + TOTP/WebAuthn 二次确认 |

## 1. 接口分层

系统接口分为四层：

| 层 | 协议 | 用途 | 设计原则 |
|---|---|---|---|
| 外部源接口 | REST / WebSocket | 接入 TheRundown、Polymarket | 适配器封装，外部字段不泄漏到策略核心 |
| 内部数据面接口 | Redpanda event、gRPC | 高频行情、信号、风险、执行 | 可回放、幂等、低开销 |
| 内部控制面接口 | HTTP REST / WebSocket | 前端、配置、回放、审计 | 易调试、统一错误、带 trace |
| 管理与运维接口 | HTTP、Prometheus、OpenTelemetry | 健康、指标、告警 | 不暴露 secret，不参与交易路径 |

## 2. 外部接口基线

### 2.1 TheRundown

| 项 | 设计口径 |
|---|---|
| 角色 | 体育赛事和赔率数据源 |
| 认证 | REST 生产默认 `X-TheRundown-Key` header；WS 使用 query `key` |
| V2 WS | `wss://therundown.io/api/v2/ws/markets?key=...` |
| WS 消息 | 生产解析以 `meta.type` 为准：`market_price`、`heartbeat`；未知消息保留 raw 并进入 schema alert |
| 关键限制 | 非实时套餐或 `X-Websocket-Access=false` 时禁止 live 主信号；WS 消息也按 data points 计量 |
| 风险字段 | price `0.0001` 表示 off-board sentinel，不可当作真实概率 |
| 限流 | 429 时遵守 `Retry-After`，并读取 `X-Datapoints-*`、`X-Rate-Limit`、`X-Data-Delay-Seconds`、`X-Websocket-Access` |

TheRundown V2 WS `market_price` 内部映射示例：

```json
{
  "meta": {
    "type": "market_price",
    "version": "v2",
    "timestamp": 1772495104
  },
  "data": {
    "id": 193600383,
    "event_id": "9b9d0cf6007fdaeb15c3a1888dcfd5df",
    "affiliate_id": 26,
    "market_participant_id": 19402291,
    "market_id": 3,
    "line": "1.5",
    "price": "-117",
    "previous_price": "-122.0000",
    "price_delta": 5,
    "is_main_line": true,
    "normalized_market_participant_id": 10,
    "normalized_market_participant_type": 3,
    "sport_id": 7,
    "updated_at": "2026-03-02T23:44:44Z"
  }
}
```

适配规则：

1. `meta.type=market_price` 只代表一个 sportsbook/market/participant 的价格点。
2. `data.id` 作为 provider price/change id；`event_id` 是 TheRundown V2 canonical event id。
3. `normalized_market_participant_id` 优先用于 canonical participant 映射；缺失时降级到 `market_participant_id` 并降低 confidence。
4. `heartbeat.data.now` 用于连接健康与 feed clock 估计；30 秒内无任何消息时 source stale。
5. `X-Data-Delay-Seconds > 0` 时，source 自动标记为 delayed，禁止 live 主信号。
6. `X-Websocket-Access=false` 时，系统不尝试建立 live WS 主链路。
7. TheRundown WS client 有 256-message buffer 风险，订阅必须按 sport、market、affiliate 或 event 过滤。

### 2.2 Polymarket

| 项 | 设计口径 |
|---|---|
| 角色 | CLOB 执行 venue 与行情源 |
| 认证 | CLOB 交易使用 L2 API key/secret/passphrase；P0 固定 deposit wallet / `POLY_1271` 签名模式 |
| Market WS | `wss://ws-subscriptions-clob.polymarket.com/ws/market`，market channel 不鉴权 |
| User WS | `wss://ws-subscriptions-clob.polymarket.com/ws/user`，user channel 需要认证 |
| 订阅字段 | market channel 使用 `assets_ids`；user channel 使用 `markets` 即 condition IDs |
| Ping/Pong | 客户端每 10 秒 ping，服务端 pong |
| 速率限制 | 按官方 Rate Limits 页做域和端点级 token bucket，不在代码中硬编码单一全局值 |
| Geoblock | `GET https://polymarket.com/api/geoblock` 是 live trading 硬闸门 |

Polymarket market WS 订阅示例：

```json
{
  "assets_ids": ["<token_id_1>", "<token_id_2>"],
  "type": "market",
  "custom_feature_enabled": true
}
```

`custom_feature_enabled=true` 用于接收 `best_bid_ask`、`new_market`、`market_resolved` 等扩展事件；如果只需要基础 `book`、`price_change`、`last_trade_price`、`tick_size_change`，可以关闭该选项以降低解析面。

Polymarket user WS 订阅示例：

```json
{
  "auth": {
    "apiKey": "<api-key>",
    "secret": "<secret>",
    "passphrase": "<passphrase>"
  },
  "markets": ["<condition_id>"],
  "type": "user"
}
```

## 3. 控制面 REST API

所有控制面接口以 `/api/v1` 开头，响应结构统一：

```json
{
  "trace_id": "9a1c7b19-6f0b-4f7d-bb31-0eb0d9f82e1d",
  "data": {},
  "error": null,
  "ts": "2026-05-14T03:15:22.183Z"
}
```

错误结构：

```json
{
  "trace_id": "9a1c7b19-6f0b-4f7d-bb31-0eb0d9f82e1d",
  "data": null,
  "error": {
    "code": "SOURCE_STALE",
    "message": "TheRundown feed is stale for market nba:...",
    "retryable": true,
    "details": {}
  },
  "ts": "2026-05-14T03:15:22.183Z"
}
```

### 3.1 System

| Method | Path | 说明 | 鉴权 |
|---|---|---|---|
| GET | `/api/v1/system/health` | 系统总健康状态 | JWT |
| GET | `/api/v1/system/topology` | 服务拓扑、版本、连接状态 | JWT |
| GET | `/api/v1/system/mode` | 当前系统模式 | JWT |
| PATCH | `/api/v1/system/mode` | 切换 `RESEARCH_ONLY` / `PAPER_ONLY` / `LIVE_READY` | JWT + TOTP |

`GET /api/v1/system/health` 响应：

```json
{
  "mode": "PAPER_ONLY",
  "services": [
    {"name": "adapter-therundown", "status": "ok", "last_heartbeat_at": "2026-05-14T03:15:20Z"},
    {"name": "execution-gateway-pm", "status": "disabled", "reason": "live trading not enabled"}
  ],
  "queues": [
    {"topic": "norm.quote", "consumer_group": "signal-engine", "lag": 123}
  ],
  "time": {
    "host_offset_ms": 2.1,
    "polymarket_offset_ms": -8.4,
    "therundown_offset_ms": 15.7
  }
}
```

### 3.2 Sources

| Method | Path | 说明 |
|---|---|---|
| GET | `/api/v1/sources` | 数据源列表与状态 |
| GET | `/api/v1/sources/{source}` | 单数据源详情 |
| PATCH | `/api/v1/sources/{source}` | 启停、过滤器、模式配置 |
| POST | `/api/v1/sources/{source}/probe` | 立即执行 health/tier/geoblock probe |

`SourceState`：

```json
{
  "source": "therundown",
  "mode": "live_ws",
  "tier": "ultra",
  "data_delay_seconds": 0,
  "websocket_access": true,
  "status": "ok",
  "last_message_at": "2026-05-14T03:15:21Z",
  "last_heartbeat_at": "2026-05-14T03:15:20Z",
  "error": null
}
```

### 3.3 Markets

| Method | Path | 说明 |
|---|---|---|
| GET | `/api/v1/markets` | canonical 市场列表 |
| GET | `/api/v1/markets/{canonical_market_key}` | 市场详情 |
| GET | `/api/v1/markets/{canonical_market_key}/quotes` | 最新归一化 quote |
| GET | `/api/v1/markets/{canonical_market_key}/latency` | lag 统计 |
| PATCH | `/api/v1/markets/{canonical_market_key}/mapping` | 人工修正 mapping |

查询参数：

| 参数 | 说明 |
|---|---|
| `sport` | `nba`、`nfl` 等 |
| `status` | `active`、`closed`、`unmapped` |
| `min_mapping_confidence` | 最低映射置信度 |
| `source` | `therundown`、`polymarket` |

### 3.4 Strategies

| Method | Path | 说明 | 鉴权 |
|---|---|---|---|
| GET | `/api/v1/strategies` | 策略列表 | JWT |
| POST | `/api/v1/strategies` | 新建策略 | JWT + TOTP |
| GET | `/api/v1/strategies/{strategy_id}` | 策略详情 | JWT |
| PATCH | `/api/v1/strategies/{strategy_id}` | 修改策略参数 | JWT + TOTP |
| POST | `/api/v1/strategies/{strategy_id}/enable` | 启用策略 | JWT + TOTP |
| POST | `/api/v1/strategies/{strategy_id}/disable` | 停用策略 | JWT |

策略参数示例：

```json
{
  "name": "therundown_to_pm_moneyline_v1",
  "enabled": false,
  "params": {
    "allowed_sports": ["nba", "nfl"],
    "allowed_market_types": ["moneyline"],
    "future_facing_market_types": ["spread", "total"],
    "min_mapping_confidence": 0.95,
    "max_source_age_ms": 750,
    "min_lead_ms": 100,
    "min_edge_bps": 80,
    "min_depth_usdc": 25
  },
  "risk_limits": {
    "max_order_size_usdc": 10,
    "max_market_exposure_usdc": 100,
    "max_daily_loss_usdc": 50,
    "max_orders_per_minute": 30
  }
}
```

P0 live 范围仅允许 full-game moneyline。`spread` 和 `total` 只保留为 future-facing interface 示例，不属于 P0 live trading 范围，也不能在 Phase 1/2/early ingestion 阶段被当作 live order 或 signal 输入。

### 3.5 Signals

| Method | Path | 说明 |
|---|---|---|
| GET | `/api/v1/signals` | 信号列表 |
| GET | `/api/v1/signals/{signal_id}` | 信号详情 |
| GET | `/api/v1/signals/{signal_id}/inputs` | 信号输入行情与 mapping |

`SignalEvent`：

```json
{
  "signal_id": "uuid",
  "strategy_id": "uuid",
  "canonical_market_key": "nba:nba:lakers_vs_celtics:full_game:moneyline:na:home",
  "decision": "candidate",
  "edge_bps": 125,
  "lead_ms": 180,
  "external_prob": "0.5321",
  "polymarket_executable_prob": "0.5196",
  "reject_reason": null,
  "input_trace_ids": ["uuid-1", "uuid-2"]
}
```

### 3.6 Orders

| Method | Path | 说明 |
|---|---|---|
| GET | `/api/v1/orders/live` | live 订单列表 |
| GET | `/api/v1/orders/live/{order_id}` | live 订单详情 |
| POST | `/api/v1/orders/live/{order_id}/cancel` | 人工撤单 |
| GET | `/api/v1/orders/paper` | paper 订单列表 |
| GET | `/api/v1/orders/paper/{order_id}` | paper 订单详情 |

### 3.7 Risk

| Method | Path | 说明 | 鉴权 |
|---|---|---|---|
| GET | `/api/v1/risk/state` | 风控状态 | JWT |
| GET | `/api/v1/risk/limits` | 风控限制 | JWT |
| PATCH | `/api/v1/risk/limits` | 修改风控限制 | JWT + TOTP |
| POST | `/api/v1/risk/kill-switch` | 启动全局停机开关 | JWT + TOTP |
| POST | `/api/v1/risk/resume` | 解除停机并恢复到 `LIVE_READY` | JWT + TOTP |

### 3.8 Replay

| Method | Path | 说明 |
|---|---|---|
| POST | `/api/v1/replay/jobs` | 创建回放任务 |
| GET | `/api/v1/replay/jobs` | 回放任务列表 |
| GET | `/api/v1/replay/jobs/{job_id}` | 回放任务详情 |
| POST | `/api/v1/replay/jobs/{job_id}/cancel` | 取消回放 |
| GET | `/api/v1/replay/jobs/{job_id}/report` | 回放报告 |

创建回放请求：

```json
{
  "name": "nba-moneyline-2026-05-01",
  "from": "2026-05-01T00:00:00Z",
  "to": "2026-05-01T23:59:59Z",
  "markets": ["nba:nba:lakers_vs_celtics:full_game:moneyline:na:home"],
  "strategy_id": "uuid",
  "strategy_version": 12,
  "speed": 10,
  "mode": "paper"
}
```

### 3.9 Audit and Alerts

| Method | Path | 说明 |
|---|---|---|
| GET | `/api/v1/audit/events` | 审计检索 |
| GET | `/api/v1/audit/events/{audit_id}` | 审计详情 |
| GET | `/api/v1/alerts` | 告警列表 |
| PATCH | `/api/v1/alerts/{alert_id}` | acknowledge / mute |

## 4. 控制面 WebSocket

| Path | 用途 | 刷新 |
|---|---|---|
| `/ws/telemetry` | 总体健康、topic lag、source health、latency | 1-5 Hz |
| `/ws/market/{canonical_market_key}` | 单市场 quote、signal、order 状态 | 1-20 Hz |
| `/ws/alerts` | 告警推送 | 实时 |
| `/ws/replay/{job_id}` | 回放进度与指标 | 1 Hz |

WebSocket 消息：

```json
{
  "type": "quote_snapshot",
  "trace_id": "uuid",
  "ts": "2026-05-14T03:15:22.183Z",
  "data": {}
}
```

## 5. 内部 gRPC / HTTP 接口

### 5.1 RiskService

```proto
service RiskService {
  rpc Evaluate(OrderIntent) returns (RiskDecision);
  rpc GetRiskState(GetRiskStateRequest) returns (RiskState);
  rpc ActivateKillSwitch(KillSwitchRequest) returns (KillSwitchAck);
}
```

### 5.2 ExecutionService

```proto
service ExecutionService {
  rpc Execute(ApprovedIntent) returns (OrderAck);
  rpc Cancel(OrderCancelRequest) returns (OrderCancelAck);
  rpc GetOrder(OrderQuery) returns (OrderState);
  rpc Heartbeat(HeartbeatRequest) returns (HeartbeatAck);
  rpc PretradeCheck(PretradeCheckRequest) returns (PretradeCheckResult);
}
```

### 5.3 ClockService

```proto
service ClockService {
  rpc GetSourceOffsets(GetSourceOffsetsRequest) returns (SourceOffsets);
  rpc RecordHeartbeat(SourceHeartbeat) returns (HeartbeatAck);
}
```

## 6. Redpanda Event Schema

### 6.1 `RawMessage`

```json
{
  "schema_version": 1,
  "trace_id": "uuid",
  "source": "therundown",
  "source_channel": "v2_ws_markets",
  "received_at": "2026-05-14T03:15:22.183Z",
  "received_mono_ns": 8451294412233,
  "cursor_or_seq": "string|null",
  "message_hash": "sha256:...",
  "payload": {}
}
```

### 6.2 `OrderIntent`

```json
{
  "schema_version": 1,
  "intent_id": "uuid",
  "signal_id": "uuid",
  "strategy_id": "uuid",
  "canonical_market_key": "string",
  "venue": "polymarket",
  "token_id": "string",
  "side": "buy",
  "outcome": "yes",
  "price": "0.52",
  "size": "10.00",
  "time_in_force": "fak",
  "order_type": "FAK",
  "reason": "EXT_SOURCE_LEADS_PM",
  "created_at": "2026-05-14T03:15:22.183Z"
}
```

### 6.3 `RiskDecision`

```json
{
  "decision_id": "uuid",
  "intent_id": "uuid",
  "approved": false,
  "reason_code": "SOURCE_STALE",
  "policy_results": [
    {"policy": "FreshnessPolicy", "passed": false, "details": {"source_age_ms": 1400}}
  ],
  "created_at": "2026-05-14T03:15:22.183Z"
}
```

## 7. 统一错误码

| Code | HTTP | Retryable | 说明 |
|---|---:|---:|---|
| `AUTH_FAILED` | 401 | 否 | 本系统控制面鉴权失败 |
| `FORBIDDEN` | 403 | 否 | 权限不足 |
| `VALIDATION_ERROR` | 422 | 否 | 请求参数错误 |
| `SOURCE_STALE` | 409 | 是 | 数据源过期 |
| `SOURCE_DELAYED` | 409 | 否 | 数据源订阅层级存在延迟 |
| `MAP_FAIL` | 409 | 否 | 市场映射失败 |
| `MAP_CONF_LOW` | 409 | 否 | 映射置信度不足 |
| `EDGE_TOO_SMALL` | 409 | 否 | edge 不满足阈值 |
| `DEPTH_TOO_SMALL` | 409 | 是 | Polymarket 深度不足 |
| `RISK_REJECTED` | 409 | 否 | 风控拒绝 |
| `KILL_SWITCH_ACTIVE` | 423 | 否 | 全局停机开关已开启 |
| `PM_AUTH_FAILED` | 502 | 否 | Polymarket 认证失败 |
| `PM_RATE_LIMITED` | 502 | 是 | Polymarket 限流 |
| `PM_GEO_BLOCKED` | 451 | 否 | Polymarket 地理限制 |
| `PM_HEARTBEAT_LOST` | 503 | 是 | 订单心跳异常 |
| `TR_AUTH_FAILED` | 502 | 否 | TheRundown key 无效 |
| `TR_RATE_LIMITED` | 502 | 是 | TheRundown 限流 |
| `TR_WS_UNAVAILABLE` | 503 | 是 | TheRundown 当前套餐或连接不支持 WS |

## 8. 安全接口要求

1. 前端所有写操作必须带 CSRF/session 防护和 TOTP/WebAuthn 二次确认。
2. `/api/v1/risk/kill-switch` 不允许被自动脚本静默调用，必须记录 actor、reason、IP、session。
3. 内部服务使用 mTLS 或 service token，token TTL 不超过 1 小时。
4. Secret 只在 adapter/execution/signer 所需进程可见。
5. 日志中只记录 signature digest，不记录 secret、private key、完整 API key。

## 9. 参考来源

- [Polymarket API Introduction](https://docs.polymarket.com/api-reference/introduction)
- [Polymarket Authentication](https://docs.polymarket.com/api-reference/authentication)
- [Polymarket Rate Limits](https://docs.polymarket.com/api-reference/rate-limits)
- [Polymarket Market Channel](https://docs.polymarket.com/api-reference/wss/market)
- [Polymarket User Channel](https://docs.polymarket.com/api-reference/wss/user)
- [Polymarket Geographic Restrictions](https://docs.polymarket.com/api-reference/geoblock)
- [TheRundown Authentication](https://docs.therundown.io/authentication)
- [TheRundown Rate Limits](https://docs.therundown.io/rate-limits)
- [TheRundown WebSocket Streaming](https://docs.therundown.io/guides/websocket-streaming)
