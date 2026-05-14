# Polymarket / TheRundown 延迟信号系统技术方案文档

核验日期：2026-05-14  
来源文档：`docs/deep-research-report.md`

## 0. 技术定版

| 领域 | 定版 |
|---|---|
| 后端服务 | Rust workspace |
| 控制面 API | Rust Axum |
| 离线分析 | Python scripts，不进入 live path |
| 前端 | React + Vite + TypeScript |
| 消息总线 | Redpanda |
| 事务库 | PostgreSQL 16 + TimescaleDB extension |
| 分析库 | ClickHouse |
| 缓存 | Redis |
| 冷归档 | S3-compatible object storage，本地 MinIO |
| 部署 | 开发 Docker Compose；生产双节点 Docker Compose + systemd |
| P0 市场 | full-game moneyline |
| P0 live order | marketable limit + FAK |
| P0 签名 | deposit wallet / `POLY_1271` |

## 1. 技术方案摘要

本方案构建一个单用户、事件驱动、控制面与数据面分离的延迟信号交易系统。系统以 TheRundown 作为当前唯一外部体育赔率源，以 Polymarket CLOB 作为当前唯一真实执行 venue。核心闭环为：

```text
外部赔率变化 -> 归一化 -> canonical 映射 -> lead-lag / edge 判定 -> 风控 -> paper/live execution -> 审计与回放
```

关键修正：

1. TheRundown 不做执行 venue，只做数据源。
2. TheRundown 非实时套餐只能用于研究或纸面验证。
3. Polymarket live trading 必须通过 geoblock、签名器、heartbeat、限流和风控预检。
4. “延迟套利”在工程文档中应落成“统计延迟 edge 执行系统”，不能假设无风险收益。

## 2. 定版技术栈

| 层 | 定版 | 用途 | 采用原因 |
|---|---|---|---|
| 低时延服务 | Rust | adapters、normalizer、latency、signal、risk、execution | 类型安全、稳定延迟、适合高并发 |
| 研究分析 | Python | 回测、参数搜索、数据分析、报告 | 量化生态成熟 |
| 前端 | React + Vite + TypeScript | 单用户控制台 | 图表、状态管理、类型契约成熟 |
| API | Rust Axum | 控制面 REST/WS | 与后端主路径统一 |
| 消息总线 | Redpanda | Kafka protocol-compatible event bus | 运维比 Apache Kafka 轻，适合小团队 |
| 事务库 | PostgreSQL + TimescaleDB | 配置、映射、订单、审计 | 强一致与 SQL 查询 |
| 分析库 | ClickHouse | 高频行情、lag、signal | 高吞吐 append 与聚合 |
| 缓存 | Redis | latest state、幂等、限流、风险计数 | 成熟、低延迟 |
| 可观测性 | OpenTelemetry + Prometheus + Grafana + Loki/Tempo | metric/log/trace | 故障定位和链路追踪 |
| 冷归档 | S3-compatible object storage，本地 MinIO | raw payload、replay dataset | 成本可控、便于恢复 |
| 部署 | Docker Compose 开发；双节点 Docker Compose + systemd 生产 | 环境管理 | 简单、可恢复、符合单用户系统规模 |

P0 固定使用上述栈，不引入 Go、NestJS、Apache Kafka 或 Kubernetes。不要一开始为了多租户、权限组织、复杂审批工作流扩大范围。

## 3. 工程目录定版

```text
quantSys/
├─ services/
│  ├─ adapter-therundown/
│  ├─ adapter-polymarket-market/
│  ├─ adapter-polymarket-user/
│  ├─ normalizer/
│  ├─ canonical-mapper/
│  ├─ latency-engine/
│  ├─ signal-engine/
│  ├─ risk-engine/
│  ├─ execution-gateway-pm/
│  ├─ paper-broker/
│  ├─ replay-service/
│  ├─ alert-service/
│  └─ api-gateway/
├─ libs/
│  ├─ domain-model/
│  ├─ source-adapter-sdk/
│  ├─ execution-sdk/
│  ├─ risk-policy/
│  ├─ telemetry/
│  └─ config/
├─ frontend/
├─ migrations/
│  ├─ postgres/
│  └─ clickhouse/
├─ infra/
│  ├─ docker-compose.yml
│  ├─ redpanda/
│  ├─ minio/
│  ├─ grafana/
│  └─ deployment/
├─ scripts/
│  ├─ topic-init/
│  ├─ replay/
│  └─ loadtest/
├─ docs/
└─ tests/
   ├─ integration/
   ├─ replay-fixtures/
   └─ load/
```

## 4. 阶段路线

### Phase 0：事实核验与账户准备

目标：确定系统能不能进入 live trading 研发。

交付物：

| 交付物 | 验收 |
|---|---|
| TheRundown 订阅层级确认 | `data_delay_seconds=0` 且 `websocket_access=true`，否则只进入 research/paper |
| Polymarket 凭证与钱包模式确认 | L1/L2 key 可用，signature type/funder 明确 |
| geoblock 检查 | 合法部署地域返回不受限 |
| 目标市场范围 | P0 固定 full-game moneyline；资金规模未提供时用开发默认风控参数 |
| 合规审查 | 不实现规避地理限制、未授权抓取或再分发 |

### Phase 1：基础设施和数据接入

目标：数据能稳定流入、落库、可重放。

任务：

1. 建立 monorepo、共享 domain model、配置加载、日志与 trace。
2. 部署 Redpanda、PostgreSQL、ClickHouse、Redis、MinIO。
3. 实现 TheRundown adapter：REST bootstrap、markets delta、V2 WS、tier/limit probe。
4. 实现 Polymarket market adapter：market discovery、market WS、重连。
5. 实现 raw topic、DLQ、S3-compatible 对象归档。
6. 实现 normalizer 与 ClickHouse 写入。

验收：

| 指标 | 目标 |
|---|---|
| adapter 运行 | 连续 24 小时无人工干预 |
| raw event | 可按 topic/offset 回溯 |
| normalized quote | ClickHouse 可查询，Redis 有 latest |
| DLQ | 坏消息不阻塞主链路 |

### Phase 2：映射、延迟和信号

目标：从 quote 生成可解释信号，但不下 live order。

任务：

1. 实现 canonical event/market mapping。
2. 加入 mapping confidence、人工 override 和低置信拒绝。
3. 实现 clock probe、source offset、lead-lag 统计。
4. 实现 signal engine：edge、depth、freshness、noise filters。
5. 信号写 ClickHouse/PostgreSQL 摘要，前端可查询。

验收：

| 指标 | 目标 |
|---|---|
| mapping | P0 市场人工抽样无误映射 |
| lead-lag | 能输出 p50/p95/p99 和计算方法 |
| signal | 每个信号有输入 quote、edge、拒绝原因 |
| replay | 固定样本结果可复现 |

### Phase 3：Paper Trading

目标：完整策略闭环在纸面模式稳定运行。

任务：

1. 实现 risk engine 的 P0 policy。
2. 实现 paper broker 三层撮合模型。
3. 实现 replay service 和 replay report。
4. 建立固定 replay fixtures 和回归阈值。
5. 前端展示 paper order、fill、PnL、slippage。

验收：

| 指标 | 目标 |
|---|---|
| 纸面撮合 | 同一数据同一参数结果一致 |
| 指标 | PnL、hit rate、fill ratio、max drawdown 可输出 |
| 风控 | 拒绝原因和限额可解释 |
| 参数 | 修改参数后能回放对比 |

### Phase 4：Polymarket Live Execution

目标：在小额和严格限制下打通真实下单/撤单/对账。

任务：

1. 实现 signer/KMS 封装，不让策略服务接触私钥。
2. 实现 execution gateway：pretrade check、submit、cancel、get order、heartbeat。
3. 实现 user WS order update 和 REST 对账。
4. 实现 geoblock、heartbeat、限流、连续拒单熔断。
5. 前端加入 `LIVE_READY` 到 `LIVE_ENABLED` 的二次确认流程。

验收：

| 指标 | 目标 |
|---|---|
| 安全 | secret 不进日志、不进前端、不进普通 DB |
| 小额订单 | 可完整下单、确认、撤单、对账 |
| 心跳 | open order heartbeat 可监控 |
| 熔断 | geoblock/heartbeat/限流/拒单会停新单 |

### Phase 5：生产化

目标：可长期运行、可恢复、可观测。

任务：

1. 压测 1k/10k msg/s，100k 作为容量实验。
2. 完成备份恢复演练。
3. 编写 runbook：source 断连、限流、geoblock、heartbeat lost、DB 磁盘、kill switch。
4. 建立 CI/CD：lint、unit、integration、replay regression、image build。
5. 建立告警：数据延迟、DLQ、backlog、订单拒绝、PnL、熔断。

## 5. 核心算法方案

### 5.1 Lead-lag / Edge

```python
def evaluate_external_move(ext_quote, pm_snapshot, cfg):
    if ext_quote.has_flag("off_board") or ext_quote.is_stale(cfg.max_source_age_ms):
        return reject("SOURCE_INVALID")

    if pm_snapshot.is_stale(cfg.max_pm_age_ms):
        return reject("PM_STALE")

    if ext_quote.mapping_confidence < cfg.min_mapping_confidence:
        return reject("MAP_CONF_LOW")

    external_prob = ext_quote.no_vig_prob
    executable_prob = pm_snapshot.best_ask_for(ext_quote.outcome)
    edge_bps = (external_prob - executable_prob) * 10000

    lead_ms = estimate_lead_ms(ext_quote, pm_snapshot)

    if lead_ms < cfg.min_lead_ms:
        return reject("LEAD_TOO_SMALL")
    if edge_bps < cfg.min_edge_bps:
        return reject("EDGE_TOO_SMALL")
    if pm_snapshot.depth_usdc < cfg.min_depth_usdc:
        return reject("DEPTH_TOO_SMALL")

    return order_intent(edge_bps=edge_bps, lead_ms=lead_ms)
```

### 5.2 Sizing

P0 不使用 Kelly。采用保守分段 sizing：

| 条件 | size |
|---|---:|
| edge < `min_edge_bps` | 0 |
| edge 达标但 lead 分位数不稳定 | `min(max_order_size * 0.1, depth * 0.05)` |
| edge 和 lead 均稳定 | `min(max_order_size, depth * depth_take_ratio, market_remaining_limit)` |
| 当日亏损接近限制 | 线性降 size |

### 5.3 风控硬规则

| 规则 | 默认处理 |
|---|---|
| kill switch active | 拒绝所有新单 |
| geoblock blocked | 拒绝所有 live 新单 |
| TheRundown delayed | 禁止 live 主信号 |
| source stale | 拒绝 |
| Polymarket stale/depth low | 拒绝 |
| mapping confidence low | 拒绝 |
| order rate exceeded | 拒绝并告警 |
| daily loss exceeded | 自动 kill switch |
| heartbeat lost | 停止新单并尝试撤单 |

## 6. 前端方案

前端是操作台，不是营销页。第一屏应直接给出系统状态、source health、lag、risk state、active signals、order summary。

页面：

| 页面 | 必备能力 |
|---|---|
| Overview | 模式、source、queue lag、time offset、risk、PnL 摘要 |
| Market Monitor | 双源价格、lead-lag、orderbook、signal timeline |
| Strategy Control | 参数编辑、版本对比、启停、回滚 |
| Orders | live/paper 订单、详情、撤单、对账状态 |
| Replay Center | 创建任务、查看进度、报告对比 |
| Audit Log | trace 检索、原始消息引用、风险原因 |
| Alerts | 告警、静音、runbook、处理记录 |
| Settings | source config、retention、deployment metadata |

交互要求：

1. live enable、risk limit 修改、kill switch resume 必须二次确认。
2. 高频行情通过 WebSocket 聚合推送，不把逐笔明细直接塞 DOM。
3. 所有状态卡必须有最近更新时间。
4. 所有订单和信号都可跳到 trace。

## 7. 测试方案

| 测试层 | 范围 | 工具 |
|---|---|---|
| Unit | odds conversion、normalizer、mapping、risk policies、sizing | Rust test / pytest |
| Contract | TheRundown/Polymarket payload fixture 到内部 schema | snapshot tests |
| Integration | adapter -> Redpanda -> normalizer -> storage | docker compose |
| Replay Regression | 固定历史窗口策略结果 | replay service |
| Execution Mock | Polymarket submit/cancel/get/heartbeat mock | wiremock / mock server |
| Load | 1k/10k/100k msg/s | k6/custom producer |
| Frontend | API mock + WS mock + critical flows | Playwright |
| Security | secret scan、authz、CSRF、mTLS config | gitleaks、integration checks |

### 7.1 必须的测试数据

| Fixture | 目的 |
|---|---|
| TheRundown `market_price` 正常消息 | V2 WS parser |
| TheRundown heartbeat | health 与 clock |
| TheRundown `0.0001` off-board | 质量标记 |
| TheRundown 429 | retry/backoff |
| Polymarket market `book` | orderbook parser |
| Polymarket market `price_change` | top-of-book update |
| Polymarket user order update | order state sync |
| Polymarket geoblock blocked | live 禁用 |

## 8. 可观测性方案

### 8.1 指标

| 指标 | 标签 |
|---|---|
| `adapter_messages_total` | source、channel、type |
| `adapter_reconnects_total` | source、reason |
| `normalizer_errors_total` | source、error_code |
| `kafka_consumer_lag` | topic、consumer_group |
| `source_age_ms` | source、market |
| `lead_ms` | source、market、strategy |
| `signal_events_total` | strategy、decision、reason |
| `risk_rejections_total` | policy、reason |
| `execution_latency_ms` | venue、operation |
| `order_rejections_total` | venue、reason |
| `paper_pnl` | strategy、replay_job |

### 8.2 Trace

Trace span：

```text
adapter.receive
  -> kafka.produce.raw
  -> normalizer.parse
  -> mapper.resolve
  -> latency.compute
  -> signal.evaluate
  -> risk.evaluate
  -> execution.pretrade
  -> execution.submit
  -> order.sync
```

每个 span 记录 `trace_id`、`market_key`、`source`、`strategy_id`、`order_id` 的可用子集。

## 9. 安全与合规

| 领域 | 要求 |
|---|---|
| 地理限制 | 任何 geoblock 失败都不规避，直接禁用 live trading |
| TheRundown 条款 | 只在合同允许范围内使用和存储数据，不做未授权再分发 |
| 私钥管理 | 策略服务不接触私钥；signer/KMS 独立 |
| 凭证 | API key/secret/passphrase 只在 secret manager 中 |
| 前端 | 单用户也要 JWT + TOTP/WebAuthn，危险操作二次确认 |
| 审计 | 配置修改、启停、kill switch、订单、拒绝、异常全部落审计 |
| 日志 | secret scrubber，禁止完整 payload header 入日志 |

## 10. 上线门槛

不能跳过纸面阶段。固定上线门槛：

| 门槛 | 条件 |
|---|---|
| 数据稳定 | 目标 source 连续 7 天采集稳定，DLQ 率低于阈值 |
| 映射可靠 | 目标市场人工抽样确认，无严重误映射 |
| 纸面盈利解释 | paper PnL 可分解为 edge、slippage、fee、latency decay |
| 回归稳定 | 最近 N 次代码/参数变更 replay 结果无异常漂移 |
| 小额演练 | live 小额下单/撤单/对账/heartbeat 全流程通过 |
| 风控演练 | kill switch、geoblock、heartbeat lost、限流、DB 异常演练通过 |
| 安全检查 | secret scan、权限、日志脱敏通过 |

## 11. 风险清单

| 风险 | 等级 | 缓解 |
|---|---|---|
| TheRundown 套餐不是实时 | 高 | 自动降级，禁止 live 主信号 |
| Polymarket 订单路径变化 | 高 | 通过 SDK/adapter 封装，contract tests |
| geoblock 或账户限制 | 高 | 每日预检 + 每单预检 |
| 盘口语义误映射 | 高 | mapping confidence + 人工 override + P0 白名单 |
| edge 被延迟吃掉 | 高 | paper latency decay + live 小额验证 |
| 过度下单被限流 | 中 | 本地令牌桶 + order rate policy |
| 数据库成本膨胀 | 中 | ClickHouse TTL + 冷归档 + topic retention |
| 单点故障 | 中 | 双机主备、备份恢复、进程隔离 |
| UI 操作误触 | 中 | 二次确认、危险按钮冷却、审计 |

## 12. 交付物清单

| 类别 | 交付物 |
|---|---|
| 设计文档 | 本目录 7 份架构/业务/数据/接口/数据库/技术文档 |
| 代码骨架 | monorepo、服务目录、共享模型、配置、日志、健康检查 |
| 基础设施 | Docker Compose、topic init、migrations |
| 适配器 | TheRundown、Polymarket market、Polymarket user |
| 核心服务 | normalizer、mapper、latency、signal、risk、paper、execution |
| 控制面 | api-gateway、frontend、alert、replay |
| 测试 | fixtures、unit、integration、replay regression、load |
| 运维 | runbook、backup/restore、dashboards、alerts |

## 13. 后续开发任务优先级

| 优先级 | 任务 |
|---|---|
| P0 | 共享 domain model、migrations、topic schema |
| P0 | TheRundown adapter 和 Polymarket market adapter |
| P0 | Normalizer、mapping、latency、signal dry-run |
| P0 | Paper Broker、Replay Service、Risk Engine |
| P1 | Polymarket Execution Gateway、Signer、User WS sync |
| P1 | API Gateway、Frontend core pages |
| P1 | CI/CD、observability、runbook |
| P2 | 第二执行 venue 文档边界、更多外部源调研、co-location 调研；不作为当前代码依赖 |

## 14. 参考来源

- [Polymarket API Introduction](https://docs.polymarket.com/api-reference/introduction)
- [Polymarket Authentication](https://docs.polymarket.com/api-reference/authentication)
- [Polymarket Rate Limits](https://docs.polymarket.com/api-reference/rate-limits)
- [Polymarket WebSocket Overview](https://docs.polymarket.com/market-data/websocket/overview)
- [Polymarket Market Channel](https://docs.polymarket.com/api-reference/wss/market)
- [Polymarket User Channel](https://docs.polymarket.com/api-reference/wss/user)
- [Polymarket Geographic Restrictions](https://docs.polymarket.com/api-reference/geoblock)
- [TheRundown Authentication](https://docs.therundown.io/authentication)
- [TheRundown Rate Limits](https://docs.therundown.io/rate-limits)
- [TheRundown WebSocket Streaming](https://docs.therundown.io/guides/websocket-streaming)
- [TheRundown V1 to V2 Migration Guide](https://docs.therundown.io/guides/v1-to-v2-migration)
