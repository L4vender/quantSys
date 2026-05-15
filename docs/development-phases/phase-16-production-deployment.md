> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：真实量化交易

# Phase 16：生产部署、监控、压测与故障演练

| 项 | 内容 |
|---|---|
| 阶段目标 | 完成 cloud-vm 与 Docker Compose 两条生产部署路径、CI/CD、安全扫描、备份恢复、load/soak/chaos、runbooks 和 release checklist。 |
| 输入文档 | [5_deployment_requirements](../5_deployment_requirements.md)、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、[2_architecture_target](../2_architecture_target.md)、Phase 3-15 docs。 |
| 新增/修改文件 | `deploy/cloud-vm/`、`deploy/docker-compose/`、`infra/prometheus/`、`infra/grafana/`、`docs/runbooks/`、`tests/load/`、`tests/chaos/`、`.github/workflows/*`、`docs/deployment-production.md`。 |
| 关键功能 | systemd units、Compose profiles、backup/restore scripts、rollback scripts、Prom rules、Grafana dashboards、Alertmanager routes、loadgen、chaos drills、release gates。 |
| 验证方式 | Staging cloud-vm deploy pass；staging Compose deploy pass；72h soak；backup restore；kill switch drill；queue lag/source stale/execution failure drills。 |
| 单元测试要求 | Config rendering、alert rule syntax、runbook link checker、backup script dry-run parser。 |
| 集成测试要求 | Full stack smoke、failover restore、Prometheus scrape、Grafana dashboard import、Alertmanager route test。 |
| 性能测试要求 | Small 1k msg/s release gate；Medium 10k msg/s before scale-up；100k msg/s only capacity experiment。 |
| 风险点 | 部署脚本与本地环境漂移；备份未演练；监控只采集不告警。 |
| 阶段交付文档 | `docs/deployment-production.md`、`docs/reports/load-test-*.md`、`docs/reports/chaos-drill-*.md`、`docs/runbooks/*.md`。 |
