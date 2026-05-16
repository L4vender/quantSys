use crate::RedisKeyBuilder;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConsumerLagSnapshot {
    pub topic: String,
    pub consumer_group: String,
    pub partition: i32,
    pub last_consumed_offset: i64,
    pub high_watermark: i64,
    pub lag: i64,
    pub updated_at: DateTime<Utc>,
}

impl ConsumerLagSnapshot {
    pub fn new(
        topic: impl Into<String>,
        consumer_group: impl Into<String>,
        partition: i32,
        last_consumed_offset: i64,
        high_watermark: i64,
        updated_at: DateTime<Utc>,
    ) -> Self {
        let lag = high_watermark.saturating_sub(last_consumed_offset).max(0);
        Self {
            topic: topic.into(),
            consumer_group: consumer_group.into(),
            partition,
            last_consumed_offset,
            high_watermark,
            lag,
            updated_at,
        }
    }

    pub fn is_lagging(&self, threshold: i64) -> bool {
        self.lag > threshold
    }
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryConsumerLagStore {
    snapshots: Arc<Mutex<ConsumerLagMap>>,
}

type ConsumerLagKey = (String, String, i32);
type ConsumerLagMap = BTreeMap<ConsumerLagKey, ConsumerLagSnapshot>;

impl InMemoryConsumerLagStore {
    pub fn update(&self, snapshot: ConsumerLagSnapshot) {
        self.snapshots
            .lock()
            .expect("consumer lag mutex poisoned")
            .insert(
                (
                    snapshot.topic.clone(),
                    snapshot.consumer_group.clone(),
                    snapshot.partition,
                ),
                snapshot,
            );
    }

    pub fn latest_for_topic(&self, topic: &str) -> Vec<ConsumerLagSnapshot> {
        self.snapshots
            .lock()
            .expect("consumer lag mutex poisoned")
            .values()
            .filter(|snapshot| snapshot.topic == topic)
            .cloned()
            .collect()
    }

    pub fn list(&self) -> Vec<ConsumerLagSnapshot> {
        self.snapshots
            .lock()
            .expect("consumer lag mutex poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub fn redis_latest(&self, keys: &RedisKeyBuilder) -> Vec<(String, serde_json::Value)> {
        self.list()
            .into_iter()
            .map(|snapshot| {
                (
                    keys.consumer_lag(&snapshot.topic, &snapshot.consumer_group),
                    serde_json::to_value(snapshot).expect("consumer lag snapshot serializes"),
                )
            })
            .collect()
    }
}
