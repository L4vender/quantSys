> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：数据采集

# Phase 3：TheRundown 数据采集

| 项 | 内容 |
|---|---|
| 阶段目标 | 实现 TheRundown REST bootstrap、market delta、V2 WS 和源能力探测，稳定产出 `raw.therundown`。 |
| 输入文档 | [1_external_api_contract_spike](../1_external_api_contract_spike.md)、[interface-document](../interface-document.md)、Phase 1 契约产物、`tests/fixtures/external/therundown/`、`docs/adapters/api-contract-baseline.md`。 |
| 新增/修改文件 | `services/adapter-therundown/`、`crates/source-sdk/src/therundown.rs`、`tests/contract/therundown_*`、`configs/sources/therundown.example.toml`、`docs/adapters/therundown.md`。 |
| 关键功能 | Header auth、WS query key、subscription filters、events bootstrap、markets delta cursor、heartbeat stale、`X-Data-Delay-Seconds`/`X-Websocket-Access`/`X-Datapoints-*` 记录、429 Retry-After、circuit breaker、raw publish。 |
| 验证方式 | Mock server 模拟 200/401/429/5xx/WS stale/data-points exhausted；raw events 入 `raw.therundown`；source state 更新。 |
| 单元测试要求 | URL/auth 构造、rate/data-point budget parser、retry jitter、heartbeat stale、payload hash、`meta.type` message dispatch。 |
| 集成测试要求 | adapter -> Redpanda -> raw archive smoke；断线后重连并按 cursor/REST bootstrap 恢复。 |
| 性能测试要求 | P0 1k msg/s raw publish；WS handler parse + enqueue P95 < 20ms；重连风暴不突破 rate budget。 |
| 风险点 | 套餐非实时、WS buffer drop、delta cursor stale、off-board sentinel 被误用。 |
| 阶段交付文档 | `docs/adapters/therundown.md`，包含字段、套餐探测结果、降级规则和真实/脱敏 fixture 对照。 |
