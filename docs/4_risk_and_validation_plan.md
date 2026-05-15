# quantSys 风险与验证计划

本文档定义 quantSys 从开发到 live trading 的风险控制、测试矩阵、压测、故障演练和上线门禁。原则：没有可验证证据，不声明可上线；没有风控通过，不允许执行。

## 1. 风险分级

| 等级 | 含义 | 处理 |
|---|---|---|
| Critical | 可能导致违规、真实资金损失、重复下单、绕过地理限制、secret 泄露 | 阻塞 live；必须有自动防护、测试和 runbook |
| High | 可能导致错误信号、误映射、数据不可用、重大延迟、审计缺失 | 阻塞 paper/live 准入；必须有集成测试和告警 |
| Medium | 影响性能、可维护性、运维恢复效率 | 阻塞 production；可不阻塞 early paper |
| Low | 文档、易用性或扩展风险 | 可后续修复，但必须登记 |

## 2. 核心风险清单

| 风险 | 等级 | 触发场景 | 必须控制 | 验证方式 |
|---|---|---|---|---|
| TheRundown 非实时或无 WS | Critical | 套餐存在固定延迟、WS access false、heartbeat stale | SourceState 标记 delayed/stale，live 主信号禁用 | Mock headers、真实 probe 记录、risk policy test |
| Polymarket geoblock | Critical | 部署地域或账户不可交易 | geoblock 每日和每单 pretrade；blocked 时 kill live | geoblock blocked fixture、execution integration |
| Secret 泄露 | Critical | 日志、DB payload、前端、CI output、fixture 泄露 key/private key | secret scrubber、最小权限、secret scan | gitleaks/trivy、log snapshot tests |
| Risk Engine 绕过 | Critical | execution 直接调用 venue 或 risk timeout 被当 allow | Execution 只接受未过期 `ALLOW`，risk unavailable fail closed | Unit + integration + chaos |
| 重复执行 | Critical | retry、timeout、worker restart、receipt 丢失 | idempotency key、unique constraint、reconcile | Mock timeout/retry integration |
| 误映射 | Critical | 队名别名、home/away 反转、period/line 不一致 | mapping confidence、manual review、home-away invariant | Golden mapping tests |
| 外部 API 契约漂移 | High | 官方字段、payload 层级、限流或权限头变化 | Phase 1 contract fixtures、schema alert、unknown raw archive | Contract tests + spike report |
| Stale quote 触发信号 | High | WS 断线、queue lag、provider_ts 漂移 | source_age、PM age、queue lag policy | Fixture + chaos network latency |
| Off-board 价格误用 | High | TheRundown `0.0001` sentinel 被当低概率 | quality flag `off_board` 阻断信号 | Normalizer unit tests |
| Polymarket 深度不足 | High | best ask/bid 可见但成交深度不足 | depth policy、slippage model、min_depth | Signal/risk tests |
| TheRundown WS buffer drop | High | 订阅过宽或 handler 过慢导致 256-message buffer 溢出 | 强制过滤、parse + enqueue、lag metric、重连后 bootstrap | WS load + chaos |
| 外部 API 限频 | High | 重连风暴、polling 过快、discovery 过多、data points 耗尽 | endpoint token bucket、data-point budget、Retry-After、circuit breaker | Mock 429、load test |
| Queue backlog | High | consumer crash、DB 慢写、source burst | lag alert、source throttle、live block | Redpanda load + chaos |
| DB 写入瓶颈 | High | ClickHouse merge backlog、PG locks | batch insert、partition、read model | DB load test |
| Audit 缺失 | High | 配置或执行没有审计 | audit writer 强制接入，critical ops 100% | API/e2e audit assertions |
| Paper 过度乐观 | High | top-of-book 模型高估成交 | L2 conservative 模型作为准入，latency decay | Replay comparison |
| Replay 与 live 逻辑分叉 | High | 回放使用独立策略代码 | 复用 normalizer/signal/risk crates | Regression hash |
| Kill switch 传播慢 | High | Redis/DB 不一致、worker cache 不刷新 | Redis primary read + DB audit；TTL/cache <= 250ms-1s | Kill switch performance test |
| 日志爆炸 | Medium | 高频 payload 打日志 | payload 只归档，日志采样 | Soak + disk alert |
| 单机故障 | Medium | VM、磁盘、DB、Redis、Redpanda 不可用 | backup/restore、主备 profile、runbook | Failover drill |

## 3. 测试矩阵

| 测试类型 | 范围 | 工具/位置 | Release Gate |
|---|---|---|---|
| Unit | DTO、parser、normalizer、mapping、signal、risk、paper、execution state | `cargo test --workspace` | PR 必跑 |
| Contract | TheRundown/Polymarket payload -> internal schema、权限头、限流头、geoblock、heartbeat | `tests/contract/`、`tests/fixtures/external/`、[1_external_api_contract_spike](1_external_api_contract_spike.md) | Adapter PR 必跑；Phase 2 前必须有基线 |
| Integration | service + Redpanda/PG/CH/Redis/MinIO | `tests/integration/` + Compose/Testcontainers | PR/merge 必跑 |
| Replay Regression | 固定 replay dataset 输出 deterministic hash | `tests/replay/` | Strategy/risk/paper 变更必跑 |
| API E2E | REST/WS、auth、errors、pagination、audit | `tests/integration/api_*`、OpenAPI | API/frontend PR 必跑 |
| Frontend E2E | dashboard、mapping review、strategy、paper、kill switch | Playwright | Frontend PR 必跑 |
| Load | ingestion、normalizer、signal、risk、API、dashboard | `tests/load/`、loadgen/k6 | Release 必跑 |
| Soak | production-like 72h 长稳 | `make soak-test` | Production 必跑 |
| Chaos | source stale、429、DB slow、queue lag、worker kill、network latency | `tests/chaos/`、toxiproxy | Staging 必跑 |
| Security | secret scan、dependency scan、authz、CSRF、TLS config | gitleaks、trivy、zap | PR/release 必跑 |
| Backup/Restore | PG、CH、Redpanda offsets、MinIO raw archive | `deploy/*/backup/` | Production 必跑 |

## 4. 单元测试最低要求

| 模块 | 必测内容 |
|---|---|
| `crates/domain` | serde roundtrip、schema_version、状态机迁移、错误码稳定性。 |
| `crates/config` | required config、env override、secret ref、invalid config fail fast。 |
| `crates/telemetry` | JSON log fields、secret scrubber、trace_id propagation。 |
| `adapter-therundown` | auth、URL、heartbeat stale、429 retry、payload hash、message type dispatch。 |
| `adapter-polymarket-*` | market/user WS payload、ping/pong、book/price parser、user order parser、geoblock/time probe。 |
| `normalizer` | odds conversion、no-vig、off-board、provider_ts missing、out-of-order、Polymarket best bid/ask。 |
| `canonical-mapper` | aliases、home/away reversal、line tolerance、period mapping、confidence threshold。 |
| `latency-engine` | offset、source_age、lead calculation method、clock drift. |
| `signal-engine` | edge_bps、depth、freshness、dedup、cooldown、reject reason。 |
| `risk-engine` | 每个 policy 的 allow/block/manual/kill；risk unavailable fail closed。 |
| `paper-broker` | fill model、partial fill、fee、slippage、PnL、deterministic seed。 |
| `execution-gateway-pm` | idempotency、retry matrix、state transitions、heartbeat、secret redaction。 |
| `api-gateway` | validation、error envelope、pagination、authz、MFA dangerous actions。 |

## 5. 集成测试最低要求

| 链路 | 验证 |
|---|---|
| TheRundown adapter -> Redpanda -> raw archive | raw event 可按 `raw_ref` 找回，断线后重连，429 不突破限频。 |
| Polymarket market adapter -> Redpanda -> normalizer | book/price/best bid ask 转为 `NormalizedQuote`，Redis latest 更新。 |
| raw -> normalizer -> ClickHouse/Redis/topic | bad payload 入 DLQ，好 payload 不丢失，consumer offset 可提交。 |
| norm -> mapper -> signal | mapping confidence 和 reject reason 正确，home/away 反转不生成可执行 signal。 |
| signal -> risk -> paper | 所有 approved paper intent 产生 paper ledger；risk blocked 不成交。 |
| replay -> paper report | 同一 dataset 输出 hash 一致，report 可比较。 |
| risk -> execution mock | 只接受 `ALLOW`，timeout/unknown state 触发 reconcile。 |
| API -> frontend | UI 只展示已实现 API；危险操作 MFA + reason + audit。 |
| observability | Prometheus scrape、logs 带 trace、alert rule 有 runbook link。 |

## 6. 性能测试目标

| 链路 | Small Production Gate | Medium Gate | 说明 |
|---|---:|---:|---|
| Ingestion raw publish | 1k msg/s，P95 < 50ms | 10k msg/s，P95 < 50ms | 100k msg/s 仅容量实验 |
| Normalizer | P95 <= 40ms | P95 <= 25ms | batch write 开启 |
| Mapper | P95 <= 80ms | P95 <= 50ms | low confidence review 不阻塞 |
| Signal Engine | P95 <= 50ms | P95 <= 30ms | 同一 market 顺序保持 |
| Risk Engine | P95 <= 20ms | P95 <= 10ms | 0 bypass |
| Source -> Signal E2E | P95 <= 500ms | P95 <= 250ms | 按 source 到 signal 输出 |
| API read | P95 <= 150ms | P95 <= 150ms | 只读 read model |
| Kill switch | 生效 < 1s | 生效 < 500ms | 所有 execution worker 可见 |
| Queue lag | normal < 30s，critical 不超过 120s | 同左 | 超阈值禁 live |
| Soak | 72h 无崩溃、无 RSS 单调增长 | 72h | staging 必跑 |

## 7. Live Trading 准入门槛

| 门槛 | 准入条件 |
|---|---|
| 合规 | geoblock 正常、外部 API contract spike 已通过、TheRundown 套餐实时且允许使用。 |
| 数据 | 目标 source 连续 7 天稳定，DLQ 率低于阈值，source age 可观测。 |
| Mapping | P0 市场人工抽样确认；mapping confidence 阈值和 review 流程通过。 |
| Paper | 保守 L2 模型下 replay/paper 报告稳定；PnL 可分解为 edge/slippage/fee/latency decay。 |
| Risk | fail-closed、kill switch、queue lag、source stale、mapping low confidence、daily loss、rate limit 全部测试通过。 |
| Execution Mock | submit/cancel/get/heartbeat/reconcile 全链路在 mock CLOB 通过。 |
| Security | secret scan 无 critical/high；日志和 audit 不含 secret。 |
| Observability | metrics、logs、trace、alerts、dashboard、runbooks 可用。 |
| Backup/Restore | PG、CH、object archive 恢复演练通过，RTO/RPO 有记录。 |
| Human Gate | 操盘人用 MFA 手动切换 `LIVE_READY` -> `LIVE_ENABLED`，初始只允许小额。 |

## 8. 故障演练计划

| 演练 | 注入方式 | 预期系统行为 |
|---|---|---|
| TheRundown heartbeat stale | Mock WS 停 heartbeat 60s | Source stale、signal reject、alert critical、live block。 |
| TheRundown 429 / data points exhausted | Mock API 返回 Retry-After 或 `X-Datapoints-Remaining=0` | Adapter 退避、circuit breaker、无重连风暴；live 主信号禁用。 |
| TheRundown WS buffer pressure | Mock WS 以 1k msg/s 推送，handler 注入延迟 | lag metric 上升、source degraded、订阅收窄或重连 bootstrap，不丢 raw archive 可追溯性。 |
| Polymarket geoblock blocked | Mock geoblock true | Live 禁用，audit + alert，paper 不受影响。 |
| Queue lag | 暂停 signal consumer | lag alert；lag_age > 300s 时禁 live。 |
| ClickHouse 慢写 | toxiproxy latency | normalizer batch retry，队列 lag 可观测，不丢 raw。 |
| Risk Engine down | 停 risk-engine | Execution 和 paper live path fail closed，audit 记录。 |
| Execution timeout | Mock CLOB submit timeout | 不重复下单；unknown state 进入 reconcile。 |
| Kill switch | API 触发 | < 1s 拒绝新 execution，open order 按 runbook 处理。 |
| Worker crash | kill worker process | supervisor 重启，idempotent resume，无重复状态迁移。 |
| Backup restore | 恢复 staging 数据 | RTO/RPO 达标，raw_ref 和 trace 可用。 |

## 9. 审计与证据要求

每次 release 必须产出：

- 测试报告：unit、integration、contract、API、frontend、replay。
- 压测报告：吞吐、P50/P95/P99、error rate、queue lag、资源使用。
- 安全报告：secret/dependency/auth 检查。
- 故障演练报告：场景、注入、预期、实际、修复项。
- Live 演练报告：仅在 live 阶段，小额订单的 signal/risk/execution/reconcile/audit trace。
