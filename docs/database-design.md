# Polymarket / TheRundown 延迟信号系统数据库设计文档

核验日期：2026-05-14  
来源文档：`docs/deep-research-report.md`

## 0. 数据库定版

| 领域 | 定版 |
|---|---|
| 事务库 | PostgreSQL 16 + TimescaleDB extension |
| 高频分析 | ClickHouse |
| 事件总线 | Redpanda，Kafka protocol compatible |
| 热缓存 | Redis |
| 冷归档 | S3-compatible object storage |
| PostgreSQL schema | `core`、`trading`、`audit`、`replay` |
| Redpanda 保留 | raw/norm 14 天，signal 30 天，order/risk 90 天，DLQ 30 天 |
| ClickHouse 保留 | quote/latency 90 天，signal 180 天，execution 365 天 |
| Live 订单真源 | PostgreSQL `trading.live_order` |

## 1. 数据库职责划分

| 存储 | 职责 | 不适合承担 |
|---|---|---|
| PostgreSQL 16 + TimescaleDB | 配置、映射、订单、审计、replay job、低频时序 | 高频全量 quote 扫描 |
| ClickHouse | 高频 normalized quote、latency、signal、聚合分析 | 强事务、订单状态机 |
| Redis | latest quote、幂等键、风险计数器、短 TTL health | 长期审计 |
| Redpanda | 事件总线、短期回放、模块解耦 | 查询型数据库 |
| S3-compatible object storage | 原始 payload 归档、冷数据、replay dataset | 低延迟热读 |

## 2. PostgreSQL Schema

固定 schema：

```sql
CREATE SCHEMA IF NOT EXISTS core;
CREATE SCHEMA IF NOT EXISTS trading;
CREATE SCHEMA IF NOT EXISTS audit;
CREATE SCHEMA IF NOT EXISTS replay;
```

### 2.1 枚举类型

```sql
CREATE TYPE core.system_mode AS ENUM (
  'RESEARCH_ONLY',
  'PAPER_ONLY',
  'LIVE_READY',
  'LIVE_ENABLED',
  'EXECUTION_DEGRADED',
  'KILL_SWITCHED'
);

CREATE TYPE trading.order_status AS ENUM (
  'CREATED',
  'RISK_REJECTED',
  'APPROVED',
  'SUBMITTED',
  'ACKED',
  'PARTIALLY_FILLED',
  'FILLED',
  'CANCEL_REQUESTED',
  'CANCELLED',
  'REJECTED',
  'FAILED'
);

CREATE TYPE core.source_status AS ENUM (
  'UNKNOWN',
  'OK',
  'DELAYED',
  'STALE',
  'RATE_LIMITED',
  'AUTH_FAILED',
  'DISABLED'
);
```

### 2.2 系统状态

```sql
CREATE TABLE core.system_state (
  id BOOLEAN PRIMARY KEY DEFAULT true CHECK (id),
  mode core.system_mode NOT NULL DEFAULT 'RESEARCH_ONLY',
  kill_switch_active BOOLEAN NOT NULL DEFAULT false,
  kill_switch_reason TEXT,
  updated_by TEXT NOT NULL DEFAULT 'system',
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO core.system_state (id)
VALUES (true)
ON CONFLICT (id) DO NOTHING;
```

### 2.3 数据源状态

```sql
CREATE TABLE core.source_state (
  source TEXT PRIMARY KEY,
  status core.source_status NOT NULL DEFAULT 'UNKNOWN',
  mode TEXT NOT NULL DEFAULT 'disabled',
  tier TEXT,
  data_delay_seconds INT,
  websocket_access BOOLEAN,
  geoblocked BOOLEAN,
  host_offset_ms NUMERIC(18,6),
  source_offset_ms NUMERIC(18,6),
  last_message_at TIMESTAMPTZ,
  last_heartbeat_at TIMESTAMPTZ,
  last_probe_at TIMESTAMPTZ,
  error_code TEXT,
  error_message TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_source_state_status ON core.source_state (status);
```

### 2.4 Canonical Event / Market

```sql
CREATE TABLE core.canonical_event (
  canonical_event_id UUID PRIMARY KEY,
  sport TEXT NOT NULL,
  league TEXT,
  home_team TEXT,
  away_team TEXT,
  scheduled_start TIMESTAMPTZ,
  status TEXT NOT NULL DEFAULT 'unknown',
  source_map JSONB NOT NULL DEFAULT '{}'::jsonb,
  mapping_confidence NUMERIC(6,4) NOT NULL DEFAULT 0,
  match_features JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE core.canonical_market (
  canonical_market_key TEXT PRIMARY KEY,
  canonical_event_id UUID NOT NULL REFERENCES core.canonical_event(canonical_event_id),
  market_type TEXT NOT NULL,
  period TEXT NOT NULL DEFAULT 'unknown',
  side_schema TEXT NOT NULL,
  line_value NUMERIC(18,8),
  status TEXT NOT NULL DEFAULT 'unknown',
  mapping_confidence NUMERIC(6,4) NOT NULL DEFAULT 0,
  source_map JSONB NOT NULL DEFAULT '{}'::jsonb,
  polymarket_condition_id TEXT,
  polymarket_token_yes TEXT,
  polymarket_token_no TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_canonical_market_event ON core.canonical_market (canonical_event_id);
CREATE INDEX idx_canonical_market_status ON core.canonical_market (status);
CREATE INDEX idx_canonical_market_type ON core.canonical_market (market_type, period);
```

### 2.5 Mapping Overrides

```sql
CREATE TABLE core.mapping_override (
  override_id UUID PRIMARY KEY,
  canonical_market_key TEXT NOT NULL REFERENCES core.canonical_market(canonical_market_key),
  source TEXT NOT NULL,
  provider_event_id TEXT,
  provider_market_id TEXT,
  provider_instrument_id TEXT,
  override_payload JSONB NOT NULL,
  reason TEXT NOT NULL,
  active BOOLEAN NOT NULL DEFAULT true,
  created_by TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mapping_override_active
  ON core.mapping_override (source, active, canonical_market_key);
```

### 2.6 Strategy Config

```sql
CREATE TABLE trading.strategy_config (
  strategy_id UUID PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  enabled BOOLEAN NOT NULL DEFAULT false,
  mode TEXT NOT NULL DEFAULT 'paper',
  params JSONB NOT NULL,
  risk_limits JSONB NOT NULL,
  version INT NOT NULL DEFAULT 1,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_by TEXT NOT NULL DEFAULT 'system'
);

CREATE TABLE trading.strategy_config_history (
  history_id BIGSERIAL PRIMARY KEY,
  strategy_id UUID NOT NULL,
  version INT NOT NULL,
  enabled BOOLEAN NOT NULL,
  mode TEXT NOT NULL,
  params JSONB NOT NULL,
  risk_limits JSONB NOT NULL,
  changed_by TEXT NOT NULL,
  change_reason TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (strategy_id, version)
);
```

### 2.7 Signals

PostgreSQL 只保存信号摘要，高频明细在 ClickHouse。

```sql
CREATE TABLE trading.signal_summary (
  signal_id UUID PRIMARY KEY,
  strategy_id UUID NOT NULL REFERENCES trading.strategy_config(strategy_id),
  canonical_market_key TEXT NOT NULL REFERENCES core.canonical_market(canonical_market_key),
  decision TEXT NOT NULL,
  edge_bps NUMERIC(18,6),
  lead_ms NUMERIC(18,6),
  external_prob NUMERIC(18,8),
  polymarket_executable_prob NUMERIC(18,8),
  reject_reason TEXT,
  input_trace_ids UUID[] NOT NULL DEFAULT '{}',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_signal_summary_market_time
  ON trading.signal_summary (canonical_market_key, created_at DESC);
CREATE INDEX idx_signal_summary_strategy_time
  ON trading.signal_summary (strategy_id, created_at DESC);
```

### 2.8 Live Orders

```sql
CREATE TABLE trading.live_order (
  order_id UUID PRIMARY KEY,
  intent_id UUID NOT NULL,
  signal_id UUID,
  strategy_id UUID NOT NULL REFERENCES trading.strategy_config(strategy_id),
  canonical_market_key TEXT NOT NULL REFERENCES core.canonical_market(canonical_market_key),
  venue TEXT NOT NULL DEFAULT 'polymarket',
  venue_order_id TEXT,
  condition_id TEXT,
  token_id TEXT NOT NULL,
  side TEXT NOT NULL,
  outcome TEXT NOT NULL,
  price NUMERIC(18,8) NOT NULL,
  size NUMERIC(18,8) NOT NULL,
  filled_size NUMERIC(18,8) NOT NULL DEFAULT 0,
  status trading.order_status NOT NULL,
  request_payload JSONB NOT NULL,
  response_payload JSONB,
  risk_decision JSONB NOT NULL,
  submitted_at TIMESTAMPTZ,
  acked_at TIMESTAMPTZ,
  terminal_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (venue, venue_order_id)
);

CREATE INDEX idx_live_order_market_status
  ON trading.live_order (canonical_market_key, status);
CREATE INDEX idx_live_order_strategy_time
  ON trading.live_order (strategy_id, created_at DESC);
CREATE INDEX idx_live_order_venue_order
  ON trading.live_order (venue, venue_order_id);
```

### 2.9 Paper Orders and Fills

```sql
CREATE TABLE trading.paper_order (
  paper_order_id UUID PRIMARY KEY,
  replay_job_id UUID,
  intent_id UUID NOT NULL,
  signal_id UUID,
  strategy_id UUID NOT NULL REFERENCES trading.strategy_config(strategy_id),
  canonical_market_key TEXT NOT NULL REFERENCES core.canonical_market(canonical_market_key),
  token_id TEXT,
  side TEXT NOT NULL,
  outcome TEXT NOT NULL,
  price NUMERIC(18,8) NOT NULL,
  size NUMERIC(18,8) NOT NULL,
  status trading.order_status NOT NULL,
  model_name TEXT NOT NULL,
  model_params JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE trading.paper_fill (
  paper_fill_id UUID PRIMARY KEY,
  paper_order_id UUID NOT NULL REFERENCES trading.paper_order(paper_order_id),
  fill_price NUMERIC(18,8) NOT NULL,
  fill_size NUMERIC(18,8) NOT NULL,
  fee NUMERIC(18,8) NOT NULL DEFAULT 0,
  slippage_bps NUMERIC(18,6),
  fill_reason TEXT NOT NULL,
  quote_ref JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_paper_order_replay ON trading.paper_order (replay_job_id);
CREATE INDEX idx_paper_fill_order ON trading.paper_fill (paper_order_id);
```

### 2.10 Audit Log

```sql
CREATE TABLE audit.audit_log (
  audit_id BIGSERIAL PRIMARY KEY,
  trace_id UUID NOT NULL,
  category TEXT NOT NULL,
  severity TEXT NOT NULL,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  entity_type TEXT,
  entity_id TEXT,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_trace_id ON audit.audit_log (trace_id);
CREATE INDEX idx_audit_created_at ON audit.audit_log (created_at DESC);
CREATE INDEX idx_audit_category_time ON audit.audit_log (category, created_at DESC);
```

### 2.11 Replay Jobs

```sql
CREATE TABLE replay.replay_job (
  replay_job_id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'queued',
  from_ts TIMESTAMPTZ NOT NULL,
  to_ts TIMESTAMPTZ NOT NULL,
  markets TEXT[] NOT NULL DEFAULT '{}',
  strategy_id UUID NOT NULL REFERENCES trading.strategy_config(strategy_id),
  strategy_version INT NOT NULL,
  speed NUMERIC(18,4) NOT NULL DEFAULT 1,
  mode TEXT NOT NULL DEFAULT 'paper',
  params JSONB NOT NULL DEFAULT '{}'::jsonb,
  result_summary JSONB,
  error_message TEXT,
  created_by TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ
);

CREATE INDEX idx_replay_job_status ON replay.replay_job (status, created_at DESC);
```

## 3. ClickHouse DDL

### 3.1 Normalized Quote

```sql
CREATE TABLE IF NOT EXISTS normalized_quote
(
    schema_version UInt16,
    trace_id UUID,
    source LowCardinality(String),
    source_channel LowCardinality(String),
    provider_event_id String,
    provider_market_id String,
    provider_instrument_id String,
    canonical_event_id Nullable(UUID),
    canonical_market_key String,
    market_type LowCardinality(String),
    period LowCardinality(String),
    side LowCardinality(String),
    line_value Nullable(Decimal(18, 8)),
    price_raw String,
    price_norm_prob Nullable(Decimal(18, 8)),
    best_bid Nullable(Decimal(18, 8)),
    best_ask Nullable(Decimal(18, 8)),
    size Nullable(Decimal(18, 8)),
    book_or_affiliate_id String,
    provider_ts Nullable(DateTime64(3, 'UTC')),
    provider_ts_type LowCardinality(String),
    ingest_ts DateTime64(3, 'UTC'),
    ingest_mono_ns Int64,
    cursor_or_seq String,
    message_hash String,
    raw_ref String,
    quality_flags Array(LowCardinality(String))
)
ENGINE = MergeTree
PARTITION BY toDate(ingest_ts)
ORDER BY (canonical_market_key, ingest_ts, source, provider_instrument_id)
TTL ingest_ts + INTERVAL 90 DAY DELETE;
```

### 3.2 Latency Sample

```sql
CREATE TABLE IF NOT EXISTS latency_sample
(
    trace_id UUID,
    source LowCardinality(String),
    canonical_market_key String,
    provider_ts Nullable(DateTime64(3, 'UTC')),
    ingest_ts DateTime64(3, 'UTC'),
    source_offset_ms Nullable(Float64),
    source_age_ms Nullable(Float64),
    lead_ms Nullable(Float64),
    host_offset_ms Nullable(Float64),
    quality_flags Array(LowCardinality(String))
)
ENGINE = MergeTree
PARTITION BY toDate(ingest_ts)
ORDER BY (canonical_market_key, ingest_ts, source)
TTL ingest_ts + INTERVAL 90 DAY DELETE;
```

### 3.3 Signal Event

```sql
CREATE TABLE IF NOT EXISTS signal_event
(
    signal_id UUID,
    trace_id UUID,
    strategy_id UUID,
    strategy_version UInt32,
    canonical_market_key String,
    decision LowCardinality(String),
    edge_bps Nullable(Float64),
    lead_ms Nullable(Float64),
    external_prob Nullable(Decimal(18, 8)),
    polymarket_executable_prob Nullable(Decimal(18, 8)),
    reject_reason LowCardinality(String),
    input_trace_ids Array(UUID),
    payload String,
    created_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
PARTITION BY toDate(created_at)
ORDER BY (strategy_id, canonical_market_key, created_at)
TTL created_at + INTERVAL 180 DAY DELETE;
```

### 3.4 Execution Event

```sql
CREATE TABLE IF NOT EXISTS execution_event
(
    trace_id UUID,
    order_id UUID,
    intent_id UUID,
    signal_id Nullable(UUID),
    venue LowCardinality(String),
    venue_order_id String,
    event_type LowCardinality(String),
    status LowCardinality(String),
    latency_ms Nullable(Float64),
    payload String,
    created_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
PARTITION BY toDate(created_at)
ORDER BY (venue, venue_order_id, created_at)
TTL created_at + INTERVAL 365 DAY DELETE;
```

## 4. Redis Key 设计

| Key | 类型 | TTL | 说明 |
|---|---|---:|---|
| `latest:quote:{canonical_market_key}:{source}` | HASH | 5m | 最新 quote |
| `latest:book:{token_id}` | HASH | 5m | Polymarket orderbook 摘要 |
| `source:health:{source}` | HASH | 1h | source health |
| `latency:pct:{source}:{market}` | HASH | 1h | p50/p95/p99 |
| `risk:counter:orders:{strategy_id}:{minute}` | INCR | 2h | 下单频率 |
| `risk:exposure:{market}` | HASH | 24h | 市场敞口 |
| `idempotency:{message_hash}` | STRING | 24h | 原始消息幂等 |
| `idempotency:intent:{intent_id}` | STRING | 7d | 订单意图幂等 |
| `kill_switch` | STRING | until cleared | 全局停机 |
| `replay:job:{job_id}:progress` | HASH | 7d | 回放进度 |

## 5. 一致性规则

1. 订单状态以 PostgreSQL 为事务真源，ClickHouse 只作分析副本。
2. 原始消息不可修改；修正只能通过 mapping override 或 replay 参数体现。
3. `live_order.status` 状态流转必须单向，失败补偿通过新 audit/event 记录。
4. Redpanda event 至少一次投递，消费者必须用 `message_hash` 或业务 ID 幂等。
5. Strategy config 每次修改都写 history，live order 保存当时版本。

## 6. 索引与查询模式

| 查询 | 表 | 索引 |
|---|---|---|
| 单市场订单 | `trading.live_order` | `(canonical_market_key, status)` |
| 策略最近信号 | `trading.signal_summary` | `(strategy_id, created_at DESC)` |
| trace 追踪 | `audit.audit_log` | `trace_id` |
| 最近 quote | ClickHouse `normalized_quote` | `(canonical_market_key, ingest_ts, source)` |
| latency 分位数 | ClickHouse `latency_sample` | `(canonical_market_key, ingest_ts, source)` |
| 回放任务 | `replay.replay_job` | `(status, created_at DESC)` |

## 7. Migration 策略

1. PostgreSQL 使用顺序 migration，文件命名 `YYYYMMDDHHMM__description.sql`。
2. 每个 migration 必须包含 forward SQL；生产不强制自动 down migration。
3. ClickHouse DDL 使用 additive change 优先，避免阻塞大表重写。
4. Redis key 版本通过 prefix 控制，例如 `v1:latest:quote...`。
5. Redpanda event schema 采用 `schema_version`，消费者支持至少一个旧版本。

## 8. 数据安全

| 数据 | 安全要求 |
|---|---|
| API key / secret / passphrase | 不入库；只进 secret manager |
| Polymarket signature | 只保存 digest 和签名类型，不保存私钥 |
| 原始响应 | 过滤敏感 header 后归档 |
| 前端配置 | 不返回 secret，只返回 masked metadata |
| 审计日志 | 保留 actor、IP、session、reason |

## 9. 参考来源

- [Polymarket Authentication](https://docs.polymarket.com/api-reference/authentication)
- [Polymarket Rate Limits](https://docs.polymarket.com/api-reference/rate-limits)
- [Polymarket WebSocket Overview](https://docs.polymarket.com/market-data/websocket/overview)
- [TheRundown Authentication](https://docs.therundown.io/authentication)
- [TheRundown WebSocket Streaming](https://docs.therundown.io/guides/websocket-streaming)
- [TheRundown V1 to V2 Migration Guide](https://docs.therundown.io/guides/v1-to-v2-migration)
