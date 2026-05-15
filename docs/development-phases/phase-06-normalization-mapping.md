> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：数据采集

# Phase 6：标准化、赛事映射与主客队校验

| 项 | 内容 |
|---|---|
| 阶段目标 | 将 raw event 转为统一 `NormalizedQuote`，建立 canonical event/market/outcome mapping，并完成主客队/队伍/选手校验。 |
| 输入文档 | [2_architecture_target](../2_architecture_target.md)、[interface-document](../interface-document.md)、[0_project_audit](../0_project_audit.md)、Phase 3-5 docs。 |
| 新增/修改文件 | `services/normalizer/`、`services/canonical-mapper/`、`crates/domain/src/normalized.rs`、`crates/domain/src/mapping.rs`、`migrations/postgres/*mapping*.sql`、`tests/fixtures/normalized/`、`tests/fixtures/mapping/`、`docs/schema/normalized-quote.md`、`docs/mapping-rules.md`。 |
| 关键功能 | American odds conversion、no-vig、Polymarket executable bid/ask、off-board sentinel、provider/ingest time、out-of-order、team aliases、home/away invariant、market type/period/line/side matching、confidence scoring、manual override。 |
| 验证方式 | Golden raw -> normalized -> mapping fixtures；home/away reversed fixture 必须拒绝；override 后 mapping version 更新。 |
| 单元测试要求 | Odds conversion、no-vig、quality flags、time parsing、alias normalization、time window matching、line tolerance、confidence formula。 |
| 集成测试要求 | raw topics -> normalizer -> `norm.quote` + ClickHouse + Redis latest -> mapping decision -> PG canonical tables。 |
| 性能测试要求 | Normalization P95 <= 40ms Small；Mapping P95 <= 80ms Small；批量 recompute 不阻塞实时 consumer。 |
| 风险点 | 误映射、off-board 当低概率、provider_ts 缺失误算 lead、使用 Polymarket mid price 代替可成交价。 |
| 阶段交付文档 | `docs/schema/normalized-quote.md`、`docs/mapping-rules.md`。 |
