# quantSys 部署要求

本文档定义 quantSys 的可部署形态、服务器要求、配置与密钥、网络、安全、备份恢复、监控和上线流程。目标是让系统能在真实服务器长期运行，而不是只在本地演示。

## 1. 部署 Profile

| Profile | 阻塞级别 | 用途 | 交付目录 |
|---|---|---|---|
| `local` | P0 | 本地开发和集成测试，使用 mock external API 与本地依赖 | `deploy/docker-compose/` |
| `staging-compose` | P0 | 单台云服务器 Compose 演练，接 mock 或低权限真实源 | `deploy/docker-compose/` |
| `prod-cloud-vm` | Production blocking | Ubuntu + systemd + 原生 Rust binaries；数据服务可托管或独立 VM | `deploy/cloud-vm/` |
| `prod-compose` | Production blocking | Docker Engine + Compose v2 单机或 app/data profile | `deploy/docker-compose/` |
| `prod-multi` | P1 | 多机 app/data/monitoring 分离 | `deploy/cloud-vm/`、`deploy/docker-compose/` |
| `k8s` | Non-blocking | 高频、多环境、HPA、标准平台运维 | `deploy/k8s/` |

生产首发必须同时通过 `prod-cloud-vm` 与 `prod-compose` 的 staging 演练。Kubernetes 不阻塞 P0/P1。

## 2. 推荐服务器规格

| 档位 | CPU | RAM | Disk | Network | 适用 |
|---|---:|---:|---|---|---|
| Small Production | 16 vCPU | 64 GB | 2 TB NVMe，独立数据盘 | 1 Gbps，固定公网 IP，私网 VPC | P0/P1，1k msg/s |
| Medium Production | app 3 x 16 vCPU；data 3 x 24 vCPU | app 64 GB；data 128 GB | data 4-8 TB NVMe + object storage | 10 Gbps private | 10k msg/s |
| High-Frequency | app 6+ x 32 vCPU；data 6+ x 48 vCPU | app 128 GB；data 256 GB | 20+ TB NVMe + object storage | 10-25 Gbps | 50k sustained，100k burst experiment |

系统默认 OS：Ubuntu 24.04 LTS。
待确认：最终生产地域必须同时满足合规可交易和到 TheRundown/Polymarket 的实测延迟要求。

## 3. `prod-cloud-vm` 原生部署要求

目录：

```text
/opt/quantsys/
  releases/<version>/
  current -> releases/<version>
  scripts/
    migrate.sh
    topic-init.sh
    backup-postgres.sh
    backup-clickhouse.sh
    backup-object-archive.sh
    rollback.sh
/etc/quantsys/
  quantsys.toml
  services/*.env
  secrets/              # root only 或 Vault Agent 渲染
/var/log/quantsys/
/data/quantsys/
  postgres/
  clickhouse/
  redpanda/
  redis/
```

systemd units：

| Unit | 启动依赖 | Restart | 健康检查 |
|---|---|---|---|
| `quantsys-api-gateway.service` | network、postgres、redis、clickhouse | always, 5s | `/health/ready` |
| `quantsys-adapter-therundown.service` | redpanda、redis | always, 5s | worker heartbeat |
| `quantsys-adapter-polymarket-market.service` | redpanda、redis | always, 5s | worker heartbeat |
| `quantsys-adapter-polymarket-user.service` | redpanda、postgres、redis | always, 5s | worker heartbeat |
| `quantsys-raw-archive.service` | redpanda、object storage | always, 5s | consumer lag |
| `quantsys-normalizer.service` | redpanda、clickhouse、redis | always, 5s | consumer lag |
| `quantsys-canonical-mapper.service` | postgres、redis、redpanda | always, 5s | `/health/ready` |
| `quantsys-latency-engine.service` | redpanda、redis、clickhouse | always, 5s | latency samples |
| `quantsys-signal-engine.service` | redpanda、redis、postgres | always, 5s | signal lag |
| `quantsys-risk-engine.service` | postgres、redis | always, 3s | fail-closed readiness |
| `quantsys-paper-broker.service` | postgres、redpanda、clickhouse | always, 5s | paper queue lag |
| `quantsys-replay-service.service` | postgres、redpanda、clickhouse | always, 10s | job heartbeat |
| `quantsys-execution-gateway-pm.service` | risk-engine、postgres、redis、signer | always, 5s | execution heartbeat |
| `quantsys-signer.service` | KMS/HSM or secret mount | always, 5s | signing health without secret exposure |
| `quantsys-scheduler.service` | postgres、redis、redpanda | always, 10s | due task lag |
| `quantsys-alert-service.service` | prometheus/api-gateway | always, 10s | alert eval heartbeat |

发布流程：

1. CI 产出 immutable binaries、frontend build、SBOM、checksum。
2. 上传到 `/opt/quantsys/releases/<version>`。
3. 执行 migration，必须 backward-compatible。
4. 执行 topic init，必须幂等。
5. 切换 `current` symlink。
6. 按 `risk -> execution -> api -> adapters -> normalizer -> mapper -> latency -> signal -> paper/replay -> scheduler/alert` 顺序 rolling restart；workers 先 drain。
7. 验证 health、metrics、topic lag、worker heartbeat、audit write。
8. 失败时 rollback 到上一版本；数据库不允许破坏性 downgrade。

## 4. `prod-compose` 部署要求

Compose profiles：

| Profile | 服务 |
|---|---|
| `local` | 全依赖 + mock external + 全服务 |
| `prod-single` | 单机生产：edge、data-plane、storage、observability |
| `prod-app` | API、frontend、workers，不含 stateful dependencies |
| `prod-data` | PostgreSQL、ClickHouse、Redis、Redpanda、MinIO |
| `observability` | Prometheus、Grafana、Loki、Tempo、Alertmanager |

Compose 必须包含：

- per-service `healthcheck`
- `restart: unless-stopped`
- CPU/memory limits
- named volumes，数据盘挂载
- `.env.example`，不含真实 secret
- network 分区：edge、app、data、observability
- backup/restore scripts
- log driver 或 Vector/Loki collector
- readiness gate：migration/topic init 完成后业务服务才启动

单机扩容触发：

| 指标 | 阈值 | 动作 |
|---|---|---|
| ClickHouse insert P95 | > 200ms 持续 15m | ClickHouse 独立节点 |
| Redpanda disk util | > 70% 持续 15m | Queue 独立节点或缩短 retention |
| PostgreSQL lock wait | > 500ms | 优化事务、read replica、pgbouncer |
| Redis memory | > 75% | 分离 risk Redis 与 cache Redis |
| Worker CPU | > 70% sustained | 增加 worker 节点或分片 |

## 5. 网络与安全要求

| 控制项 | 要求 |
|---|---|
| HTTPS | TLS 1.2+，HSTS，自动证书轮换。 |
| 管理入口 | 只通过 VPN/Zero Trust/IP allowlist；危险操作 MFA。 |
| 防火墙 | 公开只开放 443 和 bastion SSH；PG/CH/Redis/Redpanda/MinIO 只私网。 |
| 出站控制 | allowlist TheRundown、Polymarket、监控/对象存储端点；禁止未知出站。 |
| Secret 管理 | SOPS/Vault/KMS；K8s 用 external-secrets；禁止把 secret 写入 `.env.example`、日志、DB payload、前端。 |
| 服务权限 | per-service DB role、最小权限、short-lived internal token 或 mTLS。 |
| Signer 隔离 | signer 独立进程，只有 execution gateway 可访问；策略服务不可访问私钥。 |
| Audit | append-only audit table + object archive WORM policy。 |
| SSH | no password login、MFA/VPN/bastion、auditd、fail2ban。 |

## 6. 配置与环境变量

配置分层：

1. `quantsys.toml`：非 secret、全局默认值、feature flags。
2. `services/*.env`：服务级非 secret 运行参数。
3. Secret manager：API key、L2 creds、private key/KMS ref、DB password。
4. PostgreSQL `system_configs`：可审计的运行时配置和策略版本。

每个服务至少需要：

- `QUANTSYS_ENV`
- `SERVICE_NAME`
- `SERVICE_INSTANCE_ID`
- `CONFIG_PATH`
- `DATABASE_URL` 或 service-specific DB ref
- `REDPANDA_BROKERS`
- `REDIS_URL`
- `CLICKHOUSE_URL`，若服务需要
- `OBJECT_STORAGE_URL`，若服务需要
- `OTEL_EXPORTER_OTLP_ENDPOINT`
- `LOG_LEVEL`

待确认：真实 secret backend 选择 SOPS、Vault、云 KMS 或等价方案。

## 7. 备份与恢复要求

| 组件 | RPO | RTO | 备份方式 | 恢复演练 |
|---|---:|---:|---|---|
| PostgreSQL | <= 15m for production | <= 60m Small | pgBackRest/WAL archive/daily full | 每月 |
| ClickHouse | <= 1h | <= 4h | partition backup + object storage | 每月 |
| Redpanda | <= topic retention checkpoint | <= 2h | retention + mirror/export offsets | 每季度 |
| Redis risk/kill/idempotency | <= 1m | <= 30m | AOF everysec + replica | 每月 |
| Object raw archive | <= 15m | <= 4h | versioned bucket + lifecycle + replication | 每季度 |
| Config/secrets | 每次变更 | <= 30m | encrypted git/SOPS or Vault backup | 每月 |

恢复验收：

- 恢复后 `trace_id -> raw_ref -> signal -> risk -> execution` 可查询。
- 恢复后 replay dataset hash 与备份前一致。
- 恢复后 live execution 默认保持 disabled，需人工 MFA 恢复。

## 8. 监控与告警要求

必须部署：

- Prometheus
- Grafana
- Loki
- Tempo
- Alertmanager
- Node exporter / cAdvisor
- Redpanda、PostgreSQL、ClickHouse、Redis exporters

Critical alerts：

| Alert | 条件 | Runbook |
|---|---|---|
| Source unavailable | heartbeat age > 60s | `docs/runbooks/source-unavailable.md` |
| Source delayed | `data_delay_seconds > 0` in live mode | `docs/runbooks/source-latency.md` |
| Queue lag critical | lag_age > 120s | `docs/runbooks/queue-lag.md` |
| Risk fail closed | risk errors > 0 or unavailable | `docs/runbooks/risk-fail-closed.md` |
| Execution failures | fail rate > 2% over 5m | `docs/runbooks/execution-failures.md` |
| Kill switch active | `kill_switch_status=1` | `docs/runbooks/kill-switch.md` |
| DB write slow | P95 write > 200ms | `docs/runbooks/db-write-slow.md` |
| Worker down | heartbeat age > 30s | `docs/runbooks/worker-down.md` |
| Secret scan finding | critical/high | `docs/runbooks/security-incident.md` |

## 9. 上线前 Checklist

| 类别 | 必须完成 |
|---|---|
| 文档 | 目标架构、API、schema、runbooks、deployment guide 与 release notes 更新。 |
| 合规 | geoblock、TheRundown 套餐、数据使用权限、账户权限确认。 |
| 数据 | Source 连续稳定采集；DLQ 可控；mapping review 完成 P0 市场。 |
| Risk | Fail-closed、kill switch、source stale、mapping low confidence、queue lag、daily loss 全部通过。 |
| Paper | Replay deterministic；paper report 稳定；PnL/slippage/fee/latency decay 可解释。 |
| Execution | Mock CLOB 全链路通过；live 小额验证前必须人工 MFA。 |
| Security | Secret scan、dependency scan、authz、CSRF/TLS 检查无 critical/high。 |
| Observability | Metrics、logs、traces、alerts、dashboards、runbook links 可用。 |
| Backup | PG/CH/Object/Redis/config restore 演练通过。 |
| Deployment | `prod-cloud-vm` 与 `prod-compose` staging 演练通过；rollback 演练通过。 |

## 10. 生产运行规则

1. 默认启动状态为 `RESEARCH_ONLY` 或 `PAPER_ONLY`，绝不自动进入 `LIVE_ENABLED`。
2. 任何 production deploy 后，live execution 保持 disabled，直到 health、risk、source、geoblock、heartbeat 全部通过。
3. 参数或风控变更必须版本化、审计、可回滚。
4. Source delayed、mapping low confidence、queue lag critical、risk unavailable、geoblock blocked 时，系统自动禁止 live 新单。
5. 所有 incident 都要产生 audit log、alert、runbook 处理记录和复盘项。
