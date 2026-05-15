> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：实盘模拟

# Phase 11：Replay / Backtest 与策略报告

| 项 | 内容 |
|---|---|
| 阶段目标 | 复用 live/paper 同一套 normalizer、mapper、signal、risk、paper 逻辑，完成可重复 replay/backtest 和报告。 |
| 输入文档 | Phase 3-10 docs、[2_architecture_target](../2_architecture_target.md)、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)。 |
| 新增/修改文件 | `services/replay-service/`、`tests/replay/`、`tests/fixtures/replay/`、`docs/replay-and-backtest.md`、`docs/reports/replay-baseline-YYYY-MM-DD.md`。 |
| 关键功能 | Replay job lifecycle、dataset manifest、topic offset replay、strategy/risk config version、deterministic report hash、PnL/hit-rate/slippage/latency attribution。 |
| 验证方式 | 同数据同配置重复 replay 输出 hash 一致；报告可分解 edge、slippage、fee、latency decay。 |
| 单元测试要求 | Dataset manifest parser、deterministic seed、report hash、time-window replay。 |
| 集成测试要求 | Historical raw/norm -> normalizer/mapper/signal/risk/paper -> report；API 可查询 replay progress。 |
| 性能测试要求 | Replay 10x speed 不产生 unbounded memory；large dataset 分批处理。 |
| 风险点 | Replay 与实时逻辑分叉；回测过拟合；数据集不含坏场景。 |
| 阶段交付文档 | `docs/replay-and-backtest.md`、首个 `docs/reports/replay-baseline-*.md`。 |
