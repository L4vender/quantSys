> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：数据采集

# Phase 5：Raw Archive、采集健康与限流控制

| 项 | 内容 |
|---|---|
| 阶段目标 | 把所有外部 raw event 可靠归档、去重、可追溯，并提供 source health、lag、rate budget、DLQ。 |
| 输入文档 | [2_architecture_target](../2_architecture_target.md)、[interface-document](../interface-document.md)、[4_risk_and_validation_plan](../4_risk_and_validation_plan.md)、Phase 3/4 数据采集 adapter docs。 |
| 新增/修改文件 | `services/raw-archive/`、`services/source-health/`、`crates/domain/src/raw.rs`、`crates/storage/src/object_archive.rs`、`migrations/postgres/*source*.sql`、`tests/fixtures/raw/`、`docs/schema/raw-event.md`。 |
| 关键功能 | Raw payload hash、object archive、raw_ref、DLQ、source heartbeat、stale/degraded/delayed/no_ws/geoblocked/rate_limited 状态、rate budget counters、consumer offset tracking。 |
| 验证方式 | raw event 可按 `raw_ref` 找回；重复消息幂等；坏 payload 入 DLQ；source state API 可读。 |
| 单元测试要求 | Message hash/idempotency、object key builder、DLQ reason、source state transition、rate budget TTL。 |
| 集成测试要求 | TheRundown + Polymarket raw topics -> archive -> PG source state -> Redis latest health。 |
| 性能测试要求 | Raw archive sustained 1k msg/s；object archive batch 不阻塞 consumer；DLQ 不影响好消息。 |
| 风险点 | raw 丢失导致无法审计；日志直接打印大 payload；source health 与真实连接状态不一致。 |
| 阶段交付文档 | `docs/schema/raw-event.md`、`docs/runbooks/source-health.md`。 |
