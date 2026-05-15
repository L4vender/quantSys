> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：Foundation

# Phase 1：外部 API 契约校准

| 项 | 内容 |
|---|---|
| 阶段目标 | 在写 adapter 前，用官方文档、真实探测和脱敏 fixture 固化 TheRundown / Polymarket 的字段、权限、限流、心跳、geoblock 和降级规则。 |
| 输入文档 | [1_external_api_contract_spike](../1_external_api_contract_spike.md)、[interface-document](../interface-document.md)、TheRundown 官方 V2 WebSocket / Rate Limits / Events / Markets Delta、Polymarket 官方 Market Channel / User Channel / Rate Limits / Geoblock / Orders / Authentication。 |
| 新增/修改文件 | `docs/reports/external-api-contract-spike-YYYY-MM-DD.md`、`tests/fixtures/external/therundown/`、`tests/fixtures/external/polymarket/`、`tests/contract/external_api_contract_manifest.yaml`、`configs/sources/*.example.toml`、`docs/adapters/api-contract-baseline.md`。 |
| 关键功能 | TheRundown entitlement probe、V2 WS `meta.type` parser 契约、REST bootstrap/delta cursor 契约、Polymarket `assets_ids` 订阅契约、`custom_feature_enabled`、endpoint 级 rate budget、geoblock hard gate、secret scrub。 |
| 验证方式 | 官方样例和真实脱敏样例均进入 fixture；contract manifest 记录来源；mock 401/429/stale/geoblock；fixture parser smoke pass；secret scan pass。 |
| 单元测试要求 | JSON schema validation、unknown field tolerance、required field missing -> DLQ、rate header parser、heartbeat stale detector、secret scrubber。 |
| 集成测试要求 | Mock TheRundown WS/REST 与 Mock Polymarket WS/CLOB/geoblock 产出 raw event 和 source state；不接真实交易。 |
| 性能测试要求 | WS message handler 只做 parse + enqueue；mock 1k msg/s 不触发 unbounded memory；reconnect/backoff 不突破配置限流。 |
| 风险点 | 无实时 TheRundown 权限、Polymarket geoblock、官方字段漂移、WS buffer drop、fixture 含 secret。 |
| 阶段交付文档 | `docs/reports/external-api-contract-spike-YYYY-MM-DD.md`、`docs/adapters/api-contract-baseline.md`。 |
