> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：真实量化交易

# Phase 14：小额 Live 交易演练

| 项 | 内容 |
|---|---|
| 阶段目标 | 在人工门禁、最小额度、完整审计下执行 Polymarket 小额真实下单/撤单/对账演练。 |
| 输入文档 | Phase 13 docs、[5_deployment_requirements](../5_deployment_requirements.md)、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)。 |
| 新增/修改文件 | `docs/live-execution-runbook.md`、`docs/reports/live-small-order-verification-YYYY-MM-DD.md`、`configs/live/*.example.toml`、`tests/integration/live_gate_*`。 |
| 关键功能 | Live preflight、geoblock per-order check、credential check、funding check、kill switch pre-check、manual approval、small-size FAK submit、cancel/get/reconcile、live ledger、audit trace。 |
| 验证方式 | 小额 live checklist 手动执行并记录；每个 live order 可追溯 signal/risk/execution/user WS/reconcile/audit。 |
| 单元测试要求 | Live gate policy、size caps、manual approval token、preflight reason codes。 |
| 集成测试要求 | Staging config -> live gate -> approved tiny order -> user WS/reconcile -> ledger。 |
| 性能测试要求 | 不以吞吐为目标；关注 external latency、reconcile drift、error rate。 |
| 风险点 | 部署地域 blocked、账户权限不足、重复订单、partial fill 未对账、人工误操作。 |
| 阶段交付文档 | `docs/live-execution-runbook.md`、`docs/reports/live-small-order-verification-*.md`。 |
