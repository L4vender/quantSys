# quantSys 目标架构设计

本文档把现有研究文档收敛成可直接开发落地的目标架构。若旧文档存在冲突，以 [0_project_audit](0_project_audit.md) 的冲突处理和本文目标口径为准。

## 1. 设计原则

1. P0 只实现 TheRundown V2 数据源、Polymarket market/user 数据源、Polymarket CLOB 执行 venue、full-game moneyline 交易闭环。
2. TheRundown 永远不进入执行 venue 抽象；它只提供赔率、赛事、盘口和变化信号。
3. 所有 live 执行必须经过 Signal Engine、Risk Engine、Execution Gateway、geoblock、heartbeat、audit；任一环节不可用时 fail closed。
4. Replay、Paper、Live 尽量复用 normalizer、mapper、signal、risk 的同一套代码，避免回测与实盘逻辑分叉。
5. 数据面用 Redpanda topic 解耦，控制面用 REST/WS，后台任务用 scheduler/lease/retry。
6. 所有外部 payload 先 raw archive，再解析；解析失败进入 DLQ，不阻塞主链路。
7. 以可观测、可恢复和可审计作为 production blocking 能力，而不是上线后的补丁。

## 2. 推荐目录结构

```text
quantSys/
├─ Cargo.toml
├─ rust-toolchain.toml
├─ Makefile
├─ README.md
├─ crates/
│  ├─ domain/                 # DTO、状态机、错误码、schema version
│  ├─ config/                 # 配置 schema、env override、secret ref
│  ├─ telemetry/              # tracing、metrics、log scrubber
│  ├─ eventbus/               # Redpanda producer/consumer、topic metadata
│  ├─ storage/                # PostgreSQL、ClickHouse、Redis、object storage clients
│  ├─ source-sdk/             # SourceAdapter trait、rate limit、circuit breaker
│  ├─ execution-sdk/          # ExecutionVenueAdapter、pretrade、idempotency
│  ├─ risk-policy/            # P0 risk policies
│  └─ test-support/           # fixtures、mock servers、snapshot helpers
├─ services/
│  ├─ adapter-therundown/
│  ├─ adapter-polymarket-market/
│  ├─ adapter-polymarket-user/
│  ├─ raw-archive/
│  ├─ normalizer/
│  ├─ canonical-mapper/
│  ├─ latency-engine/
│  ├─ signal-engine/
│  ├─ risk-engine/
│  ├─ paper-broker/
│  ├─ replay-service/
│  ├─ execution-gateway-pm/
│  ├─ signer/
│  ├─ scheduler/
│  ├─ alert-service/
│  └─ api-gateway/
├─ frontend/
│  ├─ package.json
│  ├─ src/
│  └─ tests/
├─ migrations/
│  ├─ postgres/
│  └─ clickhouse/
├─ deploy/
│  ├─ cloud-vm/
│  │  ├─ systemd/
│  │  ├─ nginx/
│  │  ├─ caddy/
│  │  ├─ backup/
│  │  └─ rollback/
│  ├─ docker-compose/
│  │  ├─ docker-compose.yml
│  │  ├─ docker-compose.local.yml
│  │  ├─ docker-compose.prod-single.yml
│  │  └─ .env.example
│  └─ k8s/                    # non-blocking high-frequency profile
├─ infra/
│  ├─ redpanda/
│  ├─ prometheus/
│  ├─ grafana/
│  ├─ loki/
│  └─ tempo/
├─ scripts/
│  ├─ topic-init/
│  ├─ seed/
│  ├─ replay/
│  ├─ loadtest/
│  └─ ops/
├─ tests/
│  ├─ fixtures/
│  │  ├─ external/
│  │  ├─ normalized/
│  │  └─ replay/
│  ├─ integration/
│  ├─ contract/
│  ├─ load/
│  └─ chaos/
└─ docs/
   ├─ runbooks/
   └─ ...
```

## 3. 模块边界

| 模块 | 负责 | 不负责 | 关键输入 | 关键输出 |
|---|---|---|---|---|
| `adapter-therundown` | TheRundown REST bootstrap、delta、V2 WS、tier/limit probe | 策略判断、下单、模拟成交 | source config、API key、subscription plan | `raw.therundown`、source health |
| `adapter-polymarket-market` | Gamma/CLOB market discovery、market WS、book/price/best bid ask | 私钥、订单提交 | market allowlist、asset/condition IDs | `raw.polymarket.market`、market metadata |
| `adapter-polymarket-user` | user WS、订单/成交状态、REST 对账读取 | 新建订单 | L2 creds、condition IDs | `raw.polymarket.user`、execution receipt events |
| `raw-archive` | raw event 归档、hash、object key、DLQ 引用 | 业务解析 | raw topics | S3/MinIO raw object、archive index |
| `normalizer` | 赔率/价格/盘口/时间戳归一化、quality flags、ClickHouse/Redis 写入 | mapping 和交易判断 | raw topics、schema fixtures | `norm.quote`、latest cache、CH rows |
| `canonical-mapper` | event/market/outcome/side/line 对齐、confidence、review task | 低置信强行交易 | normalized quotes、platform metadata、overrides | `mapping.decision`、canonical tables |
| `latency-engine` | source age、offset、lead/lag、clock probe | 用单一时间戳做交易结论 | normalized quotes、heartbeat、server time | `latency.sample`、latency metrics |
| `signal-engine` | edge、lead、depth、freshness、dedup、cooldown、OrderIntent 候选 | 直接签名或绕过风控 | norm quote、mapping、latency、strategy config | `signal.event`、`order.intent` |
| `risk-engine` | kill switch、source health、mapping、freshness、exposure、rate、loss、queue lag | 签名、调用外部 venue | order intent、risk config、account state | `risk.decision`、risk audit |
| `paper-broker` | paper order、paper fill、PnL、slippage、latency decay | 将 TheRundown 当执行 venue | approved paper intent、historical/top-of-book data | `paper.fill`、paper ledger |
| `replay-service` | 按 raw/norm/topic offset 和参数版本回放 | 改写历史 raw 数据 | replay job、dataset、strategy version | replay report、deterministic hash |
| `execution-gateway-pm` | Polymarket pretrade、FAK submit、cancel、get order、heartbeat、reconcile | 接受未批准 intent、持有策略逻辑 | approved live intent、signer、L2 creds | `execution.request`、`execution.receipt`、live ledger |
| `signer` | EIP-712 signing、KMS/HSM/private key boundary | 读取行情、读取策略 | typed order payload | signature、signature digest |
| `scheduler` | source probe、market discovery、archive、retention、reconcile、alert eval、backup trigger | 高频策略计算 | job table、leases、retry queue | job status、worker heartbeat |
| `api-gateway` | REST/WS、auth、read model、dangerous action MFA | 高频 signal loop | JWT session、PG/CH/Redis | control API response、WS update |
| `frontend` | 单用户控制台、监控、审计、策略、paper/replay、kill switch | 保存 secret、直接连数据库 | REST/WS | UI actions、operator visibility |
| `alert-service` | alert rules、Alertmanager bridge、runbook link、incident events | 静默 critical execution 异常 | metrics/events/risk alerts | alerts、notifications、audit |

## 4. 服务拆分方式

### 4.1 数据采集层

服务：

- `adapter-therundown`
- `adapter-polymarket-market`
- `adapter-polymarket-user`
- `raw-archive`

要求：

- WebSocket 优先，REST bootstrap/delta 补洞。
- 每个平台 client 独立 rate limiter、timeout、retry with jitter、circuit breaker。
- 所有 raw payload 带 `trace_id`、`received_at`、`received_mono_ns`、`payload_hash`、`source_channel`、`schema_version`。
- 真实字段待 fixture 确认；所有 parser 改动必须先更新 contract fixture。

### 4.2 数据标准化层

服务：

- `normalizer`
- `canonical-mapper`
- `latency-engine`

要求：

- `NormalizedQuote` 是策略主输入，必须包含 provider ids、canonical ids、market type、period、side、line、raw price、normalized probability、best bid/ask、size、provider_ts、ingest_ts、ingest_mono_ns、raw_ref、quality_flags。
- 主客队校验必须先过 participant normalization，再过 home/away invariant tests。
- `mapping_confidence < live_threshold` 的市场只能进入 review/paper，不允许 live。
- `lead_ms` 必须标注计算方法：`provider_ts_adjusted`、`ingest_delta` 或 `unknown`。

### 4.3 实时事件总线/队列层

目标 topic：

| Topic | Key | Producer | Consumer | Retention |
|---|---|---|---|---|
| `raw.therundown` | provider event/instrument | adapter-therundown | normalizer、archive、replay | 14d |
| `raw.polymarket.market` | asset_id/token_id | adapter-polymarket-market | normalizer、archive、replay | 14d |
| `raw.polymarket.user` | venue_order_id | adapter-polymarket-user | execution sync、archive | 90d |
| `norm.quote` | canonical_market_key | normalizer | mapper、latency、signal、CH sink | 14d |
| `mapping.decision` | canonical_event_id | canonical-mapper | signal、api、review | 30d |
| `latency.sample` | canonical_market_key | latency-engine | signal、alert、api | 30d |
| `signal.event` | canonical_market_key | signal-engine | risk、api、CH sink | 30d |
| `order.intent` | intent_id | signal-engine | risk | 90d |
| `risk.decision` | intent_id | risk-engine | paper、execution、api | 90d |
| `execution.request` | venue_account_id | risk/manual | execution-gateway-pm | 90d |
| `execution.receipt` | venue_order_id | execution/user adapter | ledger、audit、reconcile | 365d |
| `paper.fill` | paper_order_id | paper-broker | replay、api、analytics | 180d |
| `dlq.*` | message_hash | any service | operator/replay | 30d |

语义：至少一次投递；消费者必须用 idempotency key、state version、unique constraint 防重复。

### 4.4 策略计算层

输入：

- `norm.quote`
- `mapping.decision`
- `latency.sample`
- Redis latest Polymarket top-of-book/depth
- strategy config version

输出：

- `signal.event`
- `order.intent`，只表示候选执行意图，不表示可执行订单

P0 策略：

- 只处理 full-game moneyline。
- external probability 使用 TheRundown sportsbook odds 经 no-vig 后的概率。
- Polymarket executable probability 使用可成交 best ask/bid，不使用 mid price 下单。
- 必须检查 source freshness、mapping confidence、line/side consistency、PM depth、edge、lead、cooldown。

### 4.5 模拟交易层

服务：

- `paper-broker`
- `replay-service`

模型：

1. Top-of-book optimistic：研究上限，不作为 live 准入唯一依据。
2. L2 depth conservative：P0 默认验收模型。
3. Latency decay + partial fill + reject rate：production 前必须通过。

验收：

- 同一 replay dataset、同一 strategy/risk config version、同一 deterministic seed 输出相同 report hash。
- 每个 paper fill 都能回溯 signal、risk decision、quote snapshot、raw_ref。

### 4.6 风控层

服务：

- `risk-engine`
- `scheduler` 中的 kill switch propagation/reconcile jobs

P0 policy：

- `KillSwitchPolicy`
- `GeoblockPolicy`
- `SourceFreshnessPolicy`
- `PolymarketFreshnessPolicy`
- `MappingConfidencePolicy`
- `MarketStatusPolicy`
- `DepthPolicy`
- `MinEdgePolicy`
- `OrderRatePolicy`
- `ExposurePolicy`
- `DailyLossPolicy`
- `QueueLagPolicy`

所有 policy 返回结构化结果。Risk Engine 不可用时，Execution Gateway 必须默认拒绝。

### 4.7 API 层

基础路径：

- `/health/live`
- `/health/ready`
- `/metrics`
- `/api/v1/system`
- `/api/v1/sources`
- `/api/v1/markets`
- `/api/v1/mappings`
- `/api/v1/strategies`
- `/api/v1/signals`
- `/api/v1/risk`
- `/api/v1/orders`
- `/api/v1/executions`
- `/api/v1/replay`
- `/api/v1/audit`
- `/api/v1/alerts`
- `/ws/system`
- `/ws/markets/{canonical_market_key}`
- `/ws/signals`
- `/ws/alerts`
- `/sse/replay/{job_id}`

要求：

- 所有响应带 `trace_id`、`data`、`error`、`ts`。
- 危险写操作必须 JWT + role + MFA/TOTP/WebAuthn + reason。
- 列表接口必须 cursor pagination、time range、indexed sort。
- 高频行情只走聚合 WS/SSE，不直接给前端灌逐笔 DOM。

### 4.8 前端可视化层

页面：

- Overview：系统模式、source health、queue lag、time offset、risk state、paper/live 摘要。
- Market Monitor：双源价格、lead-lag、orderbook、mapping confidence、signal timeline。
- Mapping Review：候选映射、主客队校验、人工 override、审计原因。
- Strategy Control：参数版本、启停、回滚、dry-run。
- Paper and Replay：回放任务、paper orders/fills、PnL、slippage、report diff。
- Orders and Execution：live/paper order、receipt、reconcile drift、cancel。
- Risk：limits、kill switch、policy results、incidents。
- Audit and Alerts：trace 搜索、告警、runbook、处理记录。
- Settings：source config、retention、deployment metadata。

### 4.9 运维监控层

交付目录：

- `crates/telemetry`
- `infra/prometheus`
- `infra/grafana`
- `infra/loki`
- `infra/tempo`
- `docs/runbooks`

核心指标：

- source heartbeat age、source error rate、429 count、snapshots/sec
- normalizer latency、DLQ rate、schema error rate
- mapping success rate、mapping confidence、manual review backlog
- signal latency、edge distribution、dedup rate、reject ratio
- risk decision latency、block by policy、kill switch status
- execution attempt/failure/receipt latency/reconcile drift
- queue lag age、consumer lag、Redpanda throughput
- PostgreSQL/ClickHouse/Redis write latency and resource usage
- API P95、WS connection count、frontend web vitals

## 5. 目标数据存储口径

| 存储 | 目标职责 | P0 关键对象 |
|---|---|---|
| PostgreSQL + TimescaleDB | 配置、映射、订单、审计、job、低频时序 | `core.data_sources`、`market.*`、`signal.*`、`risk.*`、`execution.*`、`audit.*`、`ops.*` |
| ClickHouse | 高频 quote、latency、signal、execution analytics | `normalized_quote`、`latency_sample`、`signal_analytics`、`execution_event` |
| Redis | latest state、dedup、rate limit、risk counters、kill switch、worker heartbeat | `latest:*`、`dedup:*`、`rl:*`、`system:kill_switch` |
| Redpanda | 数据面事件、短期 replay、模块解耦 | 见 topic 表 |
| S3/MinIO | raw payload、cold archive、replay dataset、backup | `raw/YYYY/MM/DD/...`、`replay-datasets/`、`backups/` |

## 6. 目标状态门禁

| 门禁 | 进入条件 | 禁止动作 |
|---|---|---|
| `RESEARCH_ONLY` | 依赖可启动但未完成 paper 验证 | live order |
| `PAPER_ONLY` | raw/norm/mapping/signal/risk/paper 可运行 | live order |
| `LIVE_READY` | geoblock、credentials、heartbeat、risk、paper、replay、observability、backup 演练通过 | 自动扩大 size |
| `LIVE_ENABLED` | 人工 MFA 启用，limits 已确认，小额验证通过 | 绕过 risk 或 signer |
| `EXECUTION_DEGRADED` | execution 异常、heartbeat lost、reconcile drift、external 5xx/429 | 新开 live order |
| `KILL_SWITCHED` | 人工或自动 kill switch | 除撤单、查询、审计、reconcile 之外的执行 |

