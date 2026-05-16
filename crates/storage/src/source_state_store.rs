use crate::RedisKeyBuilder;
use chrono::{DateTime, Utc};
use quantsys_domain::{Provider, SourceState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceStateSnapshot {
    pub source: String,
    pub provider: Provider,
    pub state: SourceState,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub struct InMemorySourceStateStore {
    state: Arc<Mutex<SourceStateStoreState>>,
}

#[derive(Clone, Debug, Default)]
struct SourceStateStoreState {
    latest: BTreeMap<String, SourceStateSnapshot>,
    history: BTreeMap<String, Vec<SourceStateSnapshot>>,
}

impl InMemorySourceStateStore {
    pub fn update(
        &self,
        source: impl Into<String>,
        provider: Provider,
        state: SourceState,
    ) -> SourceStateSnapshot {
        let source = source.into();
        let snapshot = SourceStateSnapshot {
            source: source.clone(),
            provider,
            state,
            updated_at: Utc::now(),
        };
        let mut store = self.state.lock().expect("source state mutex poisoned");
        store
            .history
            .entry(source.clone())
            .or_default()
            .push(snapshot.clone());
        store.latest.insert(source, snapshot.clone());
        snapshot
    }

    pub fn latest(&self, source: &str) -> Option<SourceStateSnapshot> {
        self.state
            .lock()
            .expect("source state mutex poisoned")
            .latest
            .get(source)
            .cloned()
    }

    pub fn list_latest(&self) -> Vec<SourceStateSnapshot> {
        self.state
            .lock()
            .expect("source state mutex poisoned")
            .latest
            .values()
            .cloned()
            .collect()
    }

    pub fn history(&self, source: &str) -> Vec<SourceStateSnapshot> {
        self.state
            .lock()
            .expect("source state mutex poisoned")
            .history
            .get(source)
            .cloned()
            .unwrap_or_default()
    }

    pub fn redis_latest_health(
        &self,
        keys: &RedisKeyBuilder,
        source: &str,
    ) -> Option<(String, serde_json::Value)> {
        self.latest(source).map(|snapshot| {
            (
                keys.source_health(source),
                serde_json::to_value(snapshot).expect("source state snapshot serializes"),
            )
        })
    }
}
