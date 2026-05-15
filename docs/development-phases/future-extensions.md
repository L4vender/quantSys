> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)

# 后续扩展阶段

| 阶段 | 目标 | 前置条件 |
|---|---|---|
| Phase 17：多 market type | 扩展 spread/total paper/live 支持 | moneyline paper/live 稳定，mapping 误差可控 |
| Phase 18：更多外部赔率源 | 接入新增合规数据源 | SourceAdapter、contract test、数据授权确认 |
| Phase 19：策略组合与参数搜索 | 多策略 paper 并行、参数优化 | Replay/Paper 稳定且有足够历史数据 |
| Phase 20：Kubernetes 高频部署 | HPA、多环境、标准化平台运维 | cloud-vm/Compose production 稳定后 |
| Phase 21：第二执行 venue | 新 venue 合规与接口单独审查 | Polymarket 执行稳定，风控可抽象扩展 |
