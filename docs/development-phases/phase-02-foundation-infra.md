> 上级索引：[quantSys 功能化开发阶段规划](../3_development_phases.md)
> 阶段组：Foundation

# Phase 2：工程骨架与本地基础设施

| 项 | 内容 |
|---|---|
| 阶段目标 | 创建可编译、可测试、可本地启动的最小工程底座，为数据采集阶段服务。 |
| 输入文档 | [2_architecture_target](../2_architecture_target.md)、[1_external_api_contract_spike](../1_external_api_contract_spike.md)、[interface-document](../interface-document.md)、[5_deployment_requirements](../5_deployment_requirements.md)。 |
| 新增/修改文件 | `Cargo.toml`、`rust-toolchain.toml`、`Makefile`、`crates/domain/`、`crates/config/`、`crates/telemetry/`、`crates/eventbus/`、`crates/storage/`、`crates/source-sdk/`、`crates/test-support/`、`deploy/docker-compose/`、`migrations/`、`scripts/topic-init/`、`.github/workflows/ci.yml`。 |
| 关键功能 | Workspace、DTO 基础、配置加载、secret scrubber、JSON logs、Prometheus skeleton、本地 Redpanda/PostgreSQL/ClickHouse/Redis/MinIO、topic init、fixture loader。 |
| 验证方式 | `cargo test --workspace`、`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`docker compose --profile local up -d`、migration fresh DB pass、topic init 幂等。 |
| 单元测试要求 | DTO serde roundtrip、config invalid fail fast、secret scrubber、topic config parser、Redis key builder、object key builder。 |
| 集成测试要求 | 服务二进制 `/health/live` OK；Testcontainers 或 Compose 下 PG/CH/Redis/MinIO/Redpanda 写入读回。 |
| 性能测试要求 | Redpanda produce/consume smoke 1k msg/s；ClickHouse batch insert 10k rows/s baseline；config load P95 < 10ms。 |
| 风险点 | 过早实现策略或执行；基础设施未稳定就接真实 API 会让问题难以定位。 |
| 阶段交付文档 | `docs/development/phase-02-foundation-and-local-infra.md`、`docs/schema/topic-catalog.md`。 |

## 数据采集主线

数据采集阶段的验收口径是：系统可以长期采集 TheRundown 与 Polymarket 的目标市场数据，保留 raw，生成 normalized quote 和 mapping，提供数据健康与查询能力；此阶段不生成订单意图。
