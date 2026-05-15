> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：实盘模拟

# Phase 10：真实行情驱动 Paper Broker

| 项 | 内容 |
|---|---|
| 阶段目标 | 使用真实采集行情和 Polymarket L2 深度做模拟订单、模拟成交、PnL、滑点和延迟衰减，不提交真实订单。 |
| 输入文档 | [4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、[2_architecture_target](../2_architecture_target.md)、Phase 8/9 docs。 |
| 新增/修改文件 | `services/paper-broker/`、`crates/domain/src/paper.rs`、`migrations/postgres/*paper*.sql`、`tests/fixtures/paper/`、`docs/paper-trading.md`。 |
| 关键功能 | Paper order/fill ledger、L2 conservative fill、partial fill、reject model、latency decay、fee/slippage config、paper account balance、paper exposure、paper PnL attribution。 |
| 验证方式 | Approved paper intent 产生 paper ledger；blocked intent 不成交；paper order/fill 可追溯到 signal/risk/input quote。 |
| 单元测试要求 | Fill model、partial fill、reject model、PnL、fee、slippage、deterministic seed。 |
| 集成测试要求 | `signal.event` -> risk -> paper order/fill -> ledger -> API read。 |
| 性能测试要求 | Rated load 下 paper P95 <= 100ms；ledger write 不阻塞 signal/risk consumer。 |
| 风险点 | Paper 模型过于乐观；使用 top-of-book 高估成交；未模拟 latency decay。 |
| 阶段交付文档 | `docs/paper-trading.md`。 |
