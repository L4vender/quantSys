> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：数据采集

# Phase 7：数据查询 API、采集控制台与数据质量报告

| 项 | 内容 |
|---|---|
| 阶段目标 | 将数据采集阶段做成可独立运行的产品能力：可以看 source、market、raw、normalized、mapping、lag、DLQ 和采集质量。 |
| 输入文档 | [interface-document](../interface-document.md)、[2_architecture_target](../2_architecture_target.md)、Phase 3-6 docs。 |
| 新增/修改文件 | `services/api-gateway/`、`frontend/src/pages/data/`、`frontend/tests/data.spec.ts`、`docs/api/openapi.yaml`、`docs/data-collection-console.md`、`docs/reports/data-quality-baseline-YYYY-MM-DD.md`。 |
| 关键功能 | `/sources`、`/markets`、`/raw-events`、`/normalized-quotes`、`/mappings`、`/dlq`、source health WS/SSE、mapping review UI、data quality report。 |
| 验证方式 | OpenAPI contract tests；seeded raw/norm/mapping 数据可通过 API 和 UI 查询；DLQ 和 source stale 页面可见。 |
| 单元测试要求 | Request validation、pagination、error envelope、read model mapping、frontend state reducers。 |
| 集成测试要求 | Seeded DB/Redis/CH -> API -> frontend e2e；WS/SSE source stream。 |
| 性能测试要求 | API read P95 <= 150ms；dashboard global 1Hz/detail 5-20Hz 不掉帧。 |
| 风险点 | 控制台提前暴露策略/交易按钮；API 扫 ClickHouse 大表；mapping review 操作无审计。 |
| 阶段交付文档 | `docs/data-collection-console.md`、首个 `docs/reports/data-quality-baseline-*.md`。 |

## 实盘模拟主线

实盘模拟阶段使用真实实时行情和真实盘口深度，但绝不提交真实订单。验收口径是：策略、风控、paper broker、replay、报告和控制台可以连续运行，并证明策略行为可解释、可回放、可止损。
