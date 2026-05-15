# quantSys 功能化开发阶段规划

本文档是阶段总索引。具体执行细节已经拆分到 `docs/development-phases/` 下的独立阶段文件，后续 Codex 执行时应一次只打开当前阶段文件和它引用的输入文档。

总顺序固定为：

1. Foundation：文档、外部 API 契约、工程骨架和本地基础设施。
2. 数据采集：先把 TheRundown 与 Polymarket 的实时/准实时数据稳定采进来、存下来、查得到、可观测。
3. 实盘模拟：使用真实行情和真实盘口，但只走模拟账户、模拟撮合、回放和策略报告。
4. 真实量化交易：在模拟阶段验收后，再接入真实签名、真实下单、小额演练、生产部署。

任何阶段未完成验收，下游阶段只能使用 mock 或 fixture，不得绕过前置能力。

## 阶段总览

| 阶段组 | 阶段 | 目标闭环 | 允许输出 | 禁止输出 |
|---|---|---|---|---|
| Foundation | Phase 0-2 | 统一口径、API 契约、工程可编译、本地基础设施可启动 | docs、fixtures、workspace、local infra | 策略信号、模拟成交、真实下单 |
| 数据采集 | Phase 3-7 | 外部数据 -> raw -> normalized -> mapped -> observable | raw event、normalized quote、mapping、source health、数据 API | order intent、paper fill、live order |
| 实盘模拟 | Phase 8-12 | 真实行情 -> dry-run signal -> risk -> paper fills -> replay report -> console | signal、paper order/fill、PnL、replay report、模拟控制台 | Polymarket signed order、真实资金交易 |
| 真实量化交易 | Phase 13-16 | approved intent -> signer -> mock CLOB -> 小额 live -> 生产运维 | execution receipt、live ledger、小额演练报告、runbook | 自动扩大仓位、未审计恢复 |
| 扩展 | Phase 17+ | 多 market type、多数据源、多策略、高频扩展 | 独立扩展设计 | 影响 P0 moneyline 稳定性 |

## 阶段文件

| 阶段 | 阶段组 | 文件 |
|---|---|---|
| Phase 0 | Foundation | [文档审计与目标口径收敛](development-phases/phase-00-project-audit.md) |
| Phase 1 | Foundation | [外部 API 契约校准](development-phases/phase-01-external-api-contract.md) |
| Phase 2 | Foundation | [工程骨架与本地基础设施](development-phases/phase-02-foundation-infra.md) |
| Phase 3 | 数据采集 | [TheRundown 数据采集](development-phases/phase-03-therundown-ingestion.md) |
| Phase 4 | 数据采集 | [Polymarket 数据采集](development-phases/phase-04-polymarket-ingestion.md) |
| Phase 5 | 数据采集 | [Raw Archive、采集健康与限流控制](development-phases/phase-05-raw-archive-health.md) |
| Phase 6 | 数据采集 | [标准化、赛事映射与主客队校验](development-phases/phase-06-normalization-mapping.md) |
| Phase 7 | 数据采集 | [数据查询 API、采集控制台与数据质量报告](development-phases/phase-07-data-api-console.md) |
| Phase 8 | 实盘模拟 | [策略信号 Dry-Run](development-phases/phase-08-signal-dry-run.md) |
| Phase 9 | 实盘模拟 | [模拟风控、Kill Switch 与审计](development-phases/phase-09-simulation-risk.md) |
| Phase 10 | 实盘模拟 | [真实行情驱动 Paper Broker](development-phases/phase-10-paper-broker.md) |
| Phase 11 | 实盘模拟 | [Replay / Backtest 与策略报告](development-phases/phase-11-replay-backtest.md) |
| Phase 12 | 实盘模拟 | [模拟交易控制台与 Paper 准入报告](development-phases/phase-12-simulation-console.md) |
| Phase 13 | 真实量化交易 | [Execution Contract、Signer 与 Mock CLOB](development-phases/phase-13-execution-contract-mock.md) |
| Phase 14 | 真实量化交易 | [小额 Live 交易演练](development-phases/phase-14-live-small-order.md) |
| Phase 15 | 真实量化交易 | [Live 策略运营、额度扩大与告警联动](development-phases/phase-15-live-operations.md) |
| Phase 16 | 真实量化交易 | [生产部署、监控、压测与故障演练](development-phases/phase-16-production-deployment.md) |
| Phase 17+ | 扩展 | [后续扩展阶段](development-phases/future-extensions.md) |

## 当前阶段入口

当前状态：**Phase 2 Foundation Implemented**。Phase 1 外部 API 契约校准已产出契约报告、脱敏 fixture、contract manifest、source config 样例、adapter 契约基线和 `make contract-test` smoke test。Phase 2 已新增 Rust workspace、本地基础设施、migration、topic init、CI 和最小健康服务；运行说明见 [Phase 2 Foundation And Local Infra](development/phase-02-foundation-and-local-infra.md)。

进入 Phase 2 不代表允许 live execution。真实执行仍必须等待后续 paper、risk、geoblock、heartbeat、audit、mock execution 和小额 live 演练阶段通过。
