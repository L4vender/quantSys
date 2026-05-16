use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

mod consumer_lag_store;
mod dlq_store;
mod local_csv;
mod object_archive;
mod rate_budget_store;
mod raw_index;
mod source_state_store;

pub use consumer_lag_store::{ConsumerLagSnapshot, InMemoryConsumerLagStore};
pub use dlq_store::InMemoryDlqStore;
pub use local_csv::{
    american_odds_to_implied_probability, market_decimal_mid, records_from_raw, CsvProvider,
    CsvProviderRecord, LocalCsvError, LocalCsvSink, LocalCsvWriteResult, MarketFileKey, MarketLine,
};
pub use object_archive::{
    ArchiveError, ArchiveReadRequest, ArchiveReadResult, ArchiveWriteRequest, ArchiveWriteResult,
    InMemoryObjectArchive, LocalFilesystemObjectArchive, ObjectArchive, ObjectArchiveBackend,
    ObjectArchiveConfig, ObjectKey, S3CompatibleObjectArchive,
};
pub use rate_budget_store::{InMemoryRateBudgetStore, RateBudgetSnapshot, RateBudgetStatus};
pub use raw_index::{
    InMemoryRawArchiveIndex, RawArchiveIndexRecord, RawArchiveSearchQuery, RawArchiveUpsertResult,
};
pub use source_state_store::{InMemorySourceStateStore, SourceStateSnapshot};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageConfig {
    pub postgres: PostgresConfig,
    pub clickhouse: ClickHouseConfig,
    pub redis: RedisConfig,
    pub object_storage: ObjectStorageConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PostgresConfig {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClickHouseConfig {
    pub url: String,
    pub database: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RedisConfig {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObjectStorageConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_env: String,
    pub secret_key_env: String,
}

impl StorageConfig {
    pub fn local_compose() -> Self {
        Self {
            postgres: PostgresConfig {
                url: "postgres://quantsys:quantsys@localhost:5432/quantsys".to_string(),
            },
            clickhouse: ClickHouseConfig {
                url: "http://localhost:8123".to_string(),
                database: "quantsys".to_string(),
            },
            redis: RedisConfig {
                url: "redis://localhost:6379/0".to_string(),
            },
            object_storage: ObjectStorageConfig {
                endpoint: "http://localhost:9000".to_string(),
                bucket: "quantsys-raw".to_string(),
                region: "local".to_string(),
                access_key_env: "MINIO_ROOT_USER".to_string(),
                secret_key_env: "MINIO_ROOT_PASSWORD".to_string(),
            },
        }
    }

    pub fn smoke_targets(&self) -> Vec<SmokeTarget> {
        vec![
            SmokeTarget::new("postgres", &self.postgres.url),
            SmokeTarget::new("clickhouse", &self.clickhouse.url),
            SmokeTarget::new("redis", &self.redis.url),
            SmokeTarget::new("object_storage", &self.object_storage.endpoint),
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmokeTarget {
    pub name: String,
    pub endpoint: String,
}

impl SmokeTarget {
    pub fn new(name: &str, endpoint: &str) -> Self {
        Self {
            name: name.to_string(),
            endpoint: endpoint.to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedisKeyBuilder {
    prefix: String,
}

impl RedisKeyBuilder {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    pub fn latest_quote(&self, provider: &str, canonical_market_key: &str) -> String {
        format!(
            "{}:latest:quote:{}:{}",
            self.prefix, provider, canonical_market_key
        )
    }

    pub fn worker_heartbeat(&self, service: &str, instance_id: &str) -> String {
        format!(
            "{}:worker:heartbeat:{}:{}",
            self.prefix, service, instance_id
        )
    }

    pub fn source_state(&self, source: &str) -> String {
        format!("{}:source:state:{}", self.prefix, source)
    }

    pub fn source_health(&self, source: &str) -> String {
        format!("{}:source_health:{}", self.prefix, source)
    }

    pub fn rate_budget(&self, provider: &str, endpoint: &str) -> String {
        format!("{}:rate_budget:{}:{}", self.prefix, provider, endpoint)
    }

    pub fn consumer_lag(&self, topic: &str, consumer_group: &str) -> String {
        format!("{}:consumer_lag:{}:{}", self.prefix, topic, consumer_group)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectKeyBuilder {
    prefix: String,
}

impl ObjectKeyBuilder {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    pub fn raw_payload(
        &self,
        provider: &str,
        channel: &str,
        provider_event_id: &str,
        raw_id: &str,
        received_at: DateTime<Utc>,
    ) -> String {
        format!(
            "{}/{:04}/{:02}/{:02}/{}/{}/{}/{}.json",
            self.prefix,
            received_at.year(),
            received_at.month(),
            received_at.day(),
            sanitize(provider),
            sanitize(channel),
            sanitize(provider_event_id),
            sanitize(raw_id)
        )
    }

    pub fn raw_archive_key(
        &self,
        provider: &str,
        source_channel: &str,
        raw_id: &str,
        received_at: DateTime<Utc>,
    ) -> String {
        self.archive_key("raw", provider, source_channel, raw_id, received_at)
    }

    pub fn dlq_archive_key(
        &self,
        provider: &str,
        source_channel: &str,
        raw_id: &str,
        received_at: DateTime<Utc>,
    ) -> String {
        self.archive_key("dlq", provider, source_channel, raw_id, received_at)
    }

    fn archive_key(
        &self,
        kind: &str,
        provider: &str,
        source_channel: &str,
        raw_id: &str,
        received_at: DateTime<Utc>,
    ) -> String {
        let prefix = self.prefix.trim_matches('/');
        let kind = if prefix.is_empty() {
            kind.to_string()
        } else {
            format!("{prefix}/{kind}")
        };
        format!(
            "{}/{}/{}/{:04}/{:02}/{:02}/{:02}/{}.json",
            kind,
            sanitize(provider),
            sanitize(source_channel),
            received_at.year(),
            received_at.month(),
            received_at.day(),
            received_at.hour(),
            sanitize(raw_id)
        )
    }
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
