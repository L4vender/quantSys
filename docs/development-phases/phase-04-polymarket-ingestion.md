> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：数据采集

# Phase 4：Polymarket 数据采集

| 项 | 内容 |
|---|---|
| 阶段目标 | 实现 Polymarket 市场发现、market WS、user WS 只读状态、geoblock/time probe，稳定产出 `raw.polymarket.*`。 |
| 输入文档 | [1_external_api_contract_spike](../1_external_api_contract_spike.md)、[interface-document](../interface-document.md)、[2_architecture_target](../2_architecture_target.md)、`tests/fixtures/external/polymarket/`。 |
| 新增/修改文件 | `services/adapter-polymarket-market/`、`services/adapter-polymarket-user/`、`crates/source-sdk/src/polymarket.rs`、`configs/sources/polymarket.example.toml`、`tests/contract/polymarket_*`、`docs/adapters/polymarket.md`。 |
| 关键功能 | Gamma/CLOB market discovery、condition/token cache、`assets_ids` subscription、`custom_feature_enabled`、book/price_change/best_bid_ask/last_trade_price/tick_size_change、user WS order/fill raw、PING/PONG、geoblock probe、server time offset。 |
| 验证方式 | Mock WS + fixture parse；public market WS smoke；market raw 入 `raw.polymarket.market`；user raw 入 `raw.polymarket.user`；geoblock blocked 写 source state。 |
| 单元测试要求 | Subscription payload contract、book parser、price_change parser、best_bid_ask parser、user order parser、time/geoblock parser、secret redaction。 |
| 集成测试要求 | discovery -> token cache -> subscribe -> raw publish -> archive；user WS update -> raw topic；geoblock blocked -> source state。 |
| 性能测试要求 | 订阅 1k token IDs 内存稳定；market updates 1k msg/s P95 publish < 50ms。 |
| 风险点 | market discovery 错配 token、user WS secret 泄露、geoblock 被误当 warning、扩展事件未开 `custom_feature_enabled`。 |
| 阶段交付文档 | `docs/adapters/polymarket.md`，包含 fixture 来源、字段差异、token cache 和 geoblock 处理。 |
