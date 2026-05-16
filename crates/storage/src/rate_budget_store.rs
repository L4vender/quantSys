use crate::RedisKeyBuilder;
use chrono::{DateTime, Duration, Utc};
use quantsys_domain::Provider;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RateBudgetStatus {
    Ok,
    Exhausted,
    RateLimited,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RateBudgetSnapshot {
    pub provider: Provider,
    pub endpoint: String,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_at: Option<DateTime<Utc>>,
    pub retry_after_seconds: Option<u64>,
    pub updated_at: DateTime<Utc>,
    pub status: RateBudgetStatus,
}

impl RateBudgetSnapshot {
    pub fn new(
        provider: Provider,
        endpoint: impl Into<String>,
        limit: Option<u64>,
        remaining: Option<u64>,
        reset_at: Option<DateTime<Utc>>,
        retry_after_seconds: Option<u64>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        let status = if remaining == Some(0) {
            RateBudgetStatus::Exhausted
        } else if retry_after_seconds.is_some() {
            RateBudgetStatus::RateLimited
        } else {
            RateBudgetStatus::Ok
        };
        Self {
            provider,
            endpoint: endpoint.into(),
            limit,
            remaining,
            reset_at,
            retry_after_seconds,
            updated_at,
            status,
        }
    }

    pub fn retry_after_until(&self) -> Option<DateTime<Utc>> {
        self.retry_after_seconds
            .map(|seconds| self.updated_at + Duration::seconds(seconds as i64))
    }

    pub fn is_exhausted_at(&self, now: DateTime<Utc>) -> bool {
        self.remaining == Some(0) && self.reset_at.is_none_or(|reset_at| reset_at > now)
    }
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryRateBudgetStore {
    snapshots: Arc<Mutex<BTreeMap<(Provider, String), RateBudgetSnapshot>>>,
}

impl InMemoryRateBudgetStore {
    pub fn update(&self, snapshot: RateBudgetSnapshot) {
        self.snapshots
            .lock()
            .expect("rate budget mutex poisoned")
            .insert(
                (snapshot.provider.clone(), snapshot.endpoint.clone()),
                snapshot,
            );
    }

    pub fn latest(&self, provider: &Provider, endpoint: &str) -> Option<RateBudgetSnapshot> {
        self.snapshots
            .lock()
            .expect("rate budget mutex poisoned")
            .get(&(provider.clone(), endpoint.to_string()))
            .cloned()
    }

    pub fn list(&self) -> Vec<RateBudgetSnapshot> {
        self.snapshots
            .lock()
            .expect("rate budget mutex poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub fn redis_latest(&self, keys: &RedisKeyBuilder) -> Vec<(String, serde_json::Value)> {
        self.list()
            .into_iter()
            .map(|snapshot| {
                (
                    keys.rate_budget(snapshot.provider.slug(), &snapshot.endpoint),
                    serde_json::to_value(snapshot).expect("rate budget snapshot serializes"),
                )
            })
            .collect()
    }
}
