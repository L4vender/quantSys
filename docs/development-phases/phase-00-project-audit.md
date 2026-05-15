> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：Foundation

# Phase 0：文档审计与目标口径收敛

| 项 | 内容 |
|---|---|
| 阶段目标 | 完成文档审计、冲突记录、目标架构、功能化阶段计划、风险验证、部署要求。 |
| 输入文档 | [README](../../README.md)、[0_project_audit](../0_project_audit.md)、现有保留文档。 |
| 新增/修改文件 | `docs/0_project_audit.md`、`docs/2_architecture_target.md`、`docs/3_development_phases.md`、`docs/4_risk_and_validation_plan.md`、`docs/5_deployment_requirements.md`。 |
| 关键功能 | 统一 P0 范围、记录冲突、定义目标目录、模块边界、服务拆分、功能阶段验收。 |
| 验证方式 | Markdown 结构检查、相对链接检查、确认不删除旧文档。 |
| 单元测试要求 | 无代码单测；用脚本检查文档链接和标题结构。 |
| 集成测试要求 | 无。 |
| 性能测试要求 | 无。 |
| 风险点 | 旧文档仍可能被后续误用；README 和本文件必须保持同一阶段口径。 |
| 阶段交付文档 | 本文件组。 |
