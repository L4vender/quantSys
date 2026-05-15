> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：实盘模拟

# Phase 8：策略信号 Dry-Run

| 项 | 内容 |
|---|---|
| 阶段目标 | 基于真实采集数据计算 source age、offset、lead/lag、edge 和 reject reason，只生成 signal，不生成任何订单。 |
| 输入文档 | [2_architecture_target](../2_architecture_target.md)、[interface-document](../interface-document.md)、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、Phase 3-7 docs。 |
| 新增/修改文件 | `services/latency-engine/`、`services/signal-engine/`、`crates/domain/src/signal.rs`、`tests/fixtures/signals/`、`docs/strategy/lead-lag-v1.md`。 |
| 关键功能 | Clock probe、lead calculation method、edge_bps、freshness、depth read、dedup、cooldown、reject reason、strategy config version、dry-run signal topic。 |
| 验证方式 | Labeled signal fixtures；每个 signal 可追溯 input quote/mapping/latency；reject reason 覆盖。 |
| 单元测试要求 | Lead calculation、edge calculation、depth check、dedup fingerprint、cooldown、state transitions。 |
| 集成测试要求 | `norm.quote` + `mapping.decision` + `latency.sample` -> `signal.event`，不产生 `order.intent`。 |
| 性能测试要求 | Signal P95 <= 50ms Small；同一 market 顺序保持；consumer lag 可观测。 |
| 风险点 | 过拟合阈值、误用 ingest_delta、忽略 depth/slippage、重复信号。 |
| 阶段交付文档 | `docs/strategy/lead-lag-v1.md`。 |
