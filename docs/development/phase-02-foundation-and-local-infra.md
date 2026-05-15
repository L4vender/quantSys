# Phase 2 Foundation And Local Infra

本文档记录 Phase 2 的工程底座运行方式。Phase 2 只交付 workspace、基础 DTO、配置、telemetry、eventbus/storage/source-sdk 抽象、fixture helper、本地依赖、migration、topic init 和健康服务；不实现策略、edge、风控、paper broker、signer、真实下单或前端页面。

## 交付范围

- Rust workspace：`crates/domain`、`crates/config`、`crates/telemetry`、`crates/eventbus`、`crates/storage`、`crates/source-sdk`、`crates/test-support`、`services/api-gateway`。
- 本地基础设施：`deploy/docker-compose/docker-compose.yml`，profile 为 `local`。
- 数据初始化：`migrations/postgres/0001_init.sql`、`migrations/clickhouse/0001_init.sql`。
- Topic 初始化：`scripts/topic-init/topics.toml` 和 `scripts/topic-init/topic_init.py`。
- CI：`.github/workflows/ci.yml` 执行 fmt、clippy、test、contract-test 和 topic dry-run。

## 本地验证

```bash
make fmt
make clippy
make test
make contract-test
make topic-init-dry-run
```

启动本地依赖：

```bash
cp deploy/docker-compose/.env.example deploy/docker-compose/.env
make compose-up
make migrate-local
make topic-init
```

健康服务：

```bash
cargo run -p quantsys-api-gateway
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready
curl http://localhost:8080/metrics
```

关闭本地依赖：

```bash
make compose-down
```

## 配置说明

`crates/config` 读取 Phase 1 的 `configs/sources/*.example.toml`。Secret 字段只保存环境变量引用，例如 `THERUNDON_API_KEY`、`POLYMARKET_API_KEY`、`POLYMARKET_SECRET`、`POLYMARKET_PASSPHRASE` 和 `POLYMARKET_PRIVATE_KEY`，不会解析或打印真实值。

当前支持的非 secret override：

- `QUANTSYS_THERUNDOWN_ENABLED`
- `QUANTSYS_POLYMARKET_CUSTOM_FEATURE_ENABLED`

Phase 2 示例配置仍保持 `polymarket.execution_enabled = false`。任何 live execution 能力都留给后续 mock execution、小额 live 和 production 阶段。

## 验收边界

Phase 2 通过代表工程和本地依赖可以启动，并不代表允许 live trading。进入后续阶段仍必须逐步完成 raw archive、normalization、mapping、dry-run signal、risk、paper、replay/report、mock execution、小额 live 和 production 门禁。
