> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：实盘模拟

# Phase 12：模拟交易控制台与 Paper 准入报告

| 项 | 内容 |
|---|---|
| 阶段目标 | 将实盘模拟阶段做成可操作控制台：策略参数、dry-run、paper orders、PnL、risk、replay、audit、alerts 都能查看和导出。 |
| 输入文档 | [interface-document](../interface-document.md)、Phase 8-11 docs。 |
| 新增/修改文件 | `services/api-gateway/`、`frontend/src/pages/simulation/`、`frontend/tests/simulation.spec.ts`、`docs/frontend-simulation-console.md`、`docs/reports/paper-readiness-YYYY-MM-DD.md`。 |
| 关键功能 | Strategy config version、enable/disable dry-run、paper ledger、PnL dashboard、risk decision drilldown、replay report viewer、kill switch UI、audit export。 |
| 验证方式 | Playwright critical flows；危险操作需要 MFA/reason；paper readiness report 自动生成。 |
| 单元测试要求 | API response mapping、authz/MFA gate、frontend reducers、report serializer。 |
| 集成测试要求 | Seeded paper/replay/risk/audit -> API -> frontend e2e。 |
| 性能测试要求 | Dashboard read P95 <= 150ms；paper stream 1-5Hz 不掉帧。 |
| 风险点 | UI 暴露 live 操作；策略参数修改无版本或审计；报告无法复现。 |
| 阶段交付文档 | `docs/frontend-simulation-console.md`、`docs/reports/paper-readiness-*.md`。 |

## 真实量化交易主线

真实交易阶段只允许在 Phase 12 通过后开始。验收口径是先 mock CLOB，再小额 live，再生产化；任何 geoblock、risk、source stale、queue lag、secret scan 不通过，都不得开启 live。
