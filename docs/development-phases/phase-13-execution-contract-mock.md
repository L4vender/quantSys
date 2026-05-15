> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：真实量化交易

# Phase 13：Execution Contract、Signer 与 Mock CLOB

| 项 | 内容 |
|---|---|
| 阶段目标 | 在完全 mock 的外部执行环境中实现 Polymarket 下单契约、签名、幂等、状态机和 reconcile。 |
| 输入文档 | [1_external_api_contract_spike](../1_external_api_contract_spike.md)、[5_deployment_requirements](../5_deployment_requirements.md)、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、Phase 9-12 docs。 |
| 新增/修改文件 | `services/signer/`、`services/execution-gateway-pm/`、`crates/execution-sdk/`、`tests/fixtures/execution/`、`tests/integration/execution_mock_*`、`docs/execution-contract.md`。 |
| 关键功能 | Typed order payload、deposit wallet/`POLY_1271` signing path、L2 headers、marketable limit + FAK、idempotency key、state transitions、bounded retry、mock receipt、reconcile worker、secret redaction。 |
| 验证方式 | Mock CLOB full lifecycle；timeout/unknown/retry 不重复下单；secret scan；geoblock blocked 禁 live。 |
| 单元测试要求 | Signing request shape、secret redaction、idempotency key、state transitions、retry matrix、heartbeat expiry。 |
| 集成测试要求 | approved risk -> execution request -> signer -> mock Polymarket -> receipt -> user WS fixture -> ledger reconcile。 |
| 性能测试要求 | Signal/Risk -> Execution Gateway P95 <= 20ms excluding external API；reconcile 不阻塞 submit worker。 |
| 风险点 | 真实资金风险在 mock 阶段被低估；idempotency 不完整；secret 泄露。 |
| 阶段交付文档 | `docs/execution-contract.md`、`docs/runbooks/execution-mock.md`。 |
