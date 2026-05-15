# quantSys 文档收敛记录

日期：2026-05-15

本文档记录当前保留的开发文档、已删除的旧草案文档，以及后续开发应遵循的权威入口。文档收敛后的目标是：后续 Codex 或人工开发时只需要打开少量明确文件，不再在旧研究稿、旧架构稿和重复路线图之间来回判断。

## 1. 当前保留文档

| 文件 | 用途 |
|---|---|
| [README](../README.md) | 项目入口、功能化开发顺序、当前状态、硬门槛。 |
| [1_external_api_contract_spike](1_external_api_contract_spike.md) | TheRundown / Polymarket 官方 API 契约、fixture、contract test 和 live 前置门槛。 |
| [2_architecture_target](2_architecture_target.md) | 目标架构、服务边界、模块边界、数据面/控制面方向。 |
| [3_development_phases](3_development_phases.md) | 功能化阶段总索引；每个阶段的细节拆在 `docs/development-phases/`。 |
| [4_risk_and_validation_plan](4_risk_and_validation_plan.md) | 风险清单、测试矩阵、压测、故障演练和 live 准入门槛。 |
| [5_deployment_requirements](5_deployment_requirements.md) | 云服务器、Docker Compose、监控、备份、恢复和生产门禁要求。 |
| [interface-document](interface-document.md) | 外部 API、内部 API、事件 schema 和错误码基线。 |
| [development-phases](development-phases/phase-01-external-api-contract.md) | Phase 0-16 的单阶段执行文件。 |

## 2. 已删除旧文档

以下文档内容已经被上面的保留文档吸收或替代，后续开发不再引用：

| 删除文件 | 删除原因 | 替代入口 |
|---|---|---|
| `docs/deep-research-report.md` | 原始研究输入过长，含旧假设和旧接口口径。 | [1_external_api_contract_spike](1_external_api_contract_spike.md)、[2_architecture_target](2_architecture_target.md) |
| `docs/architecture-design.md` | 旧架构稿与新功能化阶段拆分重复。 | [2_architecture_target](2_architecture_target.md) |
| `docs/business-flow-and-function-list.md` | 旧业务流程已拆进模拟、风控、live 各阶段文件。 | [3_development_phases](3_development_phases.md)、[4_risk_and_validation_plan](4_risk_and_validation_plan.md) |
| `docs/data-architecture.md` | 数据模型与 topic 规划已分散到架构、接口和数据采集阶段。 | [2_architecture_target](2_architecture_target.md)、[interface-document](interface-document.md) |
| `docs/database-design.md` | 旧数据库草案不再作为迁移输入。 | [5_deployment_requirements](5_deployment_requirements.md)、Phase 2/5/6/10 阶段文件 |
| `docs/module-relationship.md` | 模块边界已被目标架构与阶段文件替代。 | [2_architecture_target](2_architecture_target.md)、[3_development_phases](3_development_phases.md) |
| `docs/production-ready-engineering-spec.md` | 单文件过大，且与功能化阶段拆分重复。 | [2_architecture_target](2_architecture_target.md)、[4_risk_and_validation_plan](4_risk_and_validation_plan.md)、[5_deployment_requirements](5_deployment_requirements.md) |
| `docs/technical-solution.md` | 旧技术路线与当前“数据采集 -> 实盘模拟 -> 真实交易”顺序不一致。 | [3_development_phases](3_development_phases.md) |

## 3. 当前权威口径

| 领域 | 权威入口 |
|---|---|
| 项目目标与开发顺序 | [README](../README.md) |
| 外部 API 契约 | [1_external_api_contract_spike](1_external_api_contract_spike.md)、[interface-document](interface-document.md) |
| 架构与模块边界 | [2_architecture_target](2_architecture_target.md) |
| 阶段拆分 | [3_development_phases](3_development_phases.md)、`docs/development-phases/*.md` |
| 风险、测试、准入 | [4_risk_and_validation_plan](4_risk_and_validation_plan.md) |
| 部署、监控、恢复 | [5_deployment_requirements](5_deployment_requirements.md) |

## 4. 后续开发原则

1. 当前阶段从 [Phase 1 外部 API 契约校准](development-phases/phase-01-external-api-contract.md) 开始。
2. 后续每个阶段只打开对应 `docs/development-phases/phase-*.md` 和其中引用的保留文档。
3. 若某个实现细节在保留文档中不存在，不回溯已删除旧文档；应在当前阶段交付文档中补充，并注明“待确认”或“推荐方案”。
4. 删除旧文档后，任何新增文档必须有明确后续开发用途；临时研究资料应写入 `docs/reports/`，并在阶段结束时决定是否保留。
