> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：实盘模拟

# Phase 9：模拟风控、Kill Switch 与审计

| 项 | 内容 |
|---|---|
| 阶段目标 | 在 paper/live 共用的风控模型中先实现模拟准入，所有 dry-run intent 都必须经过 risk，所有决策落审计。 |
| 输入文档 | [4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、[2_architecture_target](../2_architecture_target.md)、Phase 8 docs。 |
| 新增/修改文件 | `services/risk-engine/`、`crates/risk-policy/`、`crates/domain/src/risk.rs`、`crates/audit/` 或 `crates/telemetry/src/audit.rs`、`tests/fixtures/risk/`、`docs/risk-policies.md`。 |
| 关键功能 | Policy registry、risk decision state、source stale、mapping confidence、depth、queue lag、rate/data-point budget、cooldown、paper limits、kill switch Redis+DB、audit writer。 |
| 验证方式 | Table-driven risk tests；risk service down 时 paper path fail closed；kill switch propagation < 1s。 |
| 单元测试要求 | 每个 policy 的 allow/block/manual/kill 结果、config version、counter TTL、audit payload scrub。 |
| 集成测试要求 | dry-run signal -> order intent candidate -> risk decision -> audit log -> alert event。 |
| 性能测试要求 | Risk P95 <= 20ms Small；2k req/s 10m 0 bypass。 |
| 风险点 | 风控不可用时被绕过；Redis eviction 影响 kill switch；配置变更未版本化。 |
| 阶段交付文档 | `docs/risk-policies.md`，包含模拟和未来 live 复用规则。 |
