> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：真实量化交易

# Phase 15：Live 策略运营、额度扩大与告警联动

| 项 | 内容 |
|---|---|
| 阶段目标 | 在小额 live 通过后，建立真实策略运营流程：额度分层、暂停/恢复、告警联动、异常处理和人工复核。 |
| 输入文档 | Phase 12-14 reports、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、[5_deployment_requirements](../5_deployment_requirements.md)。 |
| 新增/修改文件 | `services/alert-service/`、`docs/live-operations.md`、`docs/runbooks/live-incident.md`、`configs/risk/live-tiers.example.toml`、`frontend/src/pages/live/`。 |
| 关键功能 | Live size tiers、daily/market exposure caps、loss limits、source stale live block、queue lag live block、alert rules、manual resume workflow、post-trade review。 |
| 验证方式 | Live cannot auto-expand size；alerts block new orders；manual resume requires MFA/reason/audit。 |
| 单元测试要求 | Tier policy、loss/exposure counters、alert-to-risk block mapping、resume state machine。 |
| 集成测试要求 | Live incident fixture -> alert -> risk block -> UI -> manual resume -> audit。 |
| 性能测试要求 | Alert evaluation <= 10s；kill switch visible to workers < 1s。 |
| 风险点 | 盈亏异常未停机、告警只通知不阻断、额度扩大缺证据。 |
| 阶段交付文档 | `docs/live-operations.md`、`docs/runbooks/live-incident.md`。 |
