use async_trait::async_trait;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TopicConfig {
    pub name: String,
    pub key: String,
    pub producer: String,
    #[serde(default)]
    pub consumers: Vec<String>,
    pub partitions: u32,
    pub replicas: u32,
    #[serde(rename = "retention_days")]
    pub retention: TopicRetention,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TopicRetention {
    Days(u32),
}

impl<'de> Deserialize<'de> for TopicRetention {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let days = u32::deserialize(deserializer)?;
        Ok(Self::Days(days))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TopicCatalog {
    topics: BTreeMap<String, TopicConfig>,
}

#[derive(Debug, Error)]
pub enum EventbusError {
    #[error("failed to read topic catalog {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse topic catalog {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("eventbus operation failed: {0}")]
    Operation(String),
}

impl TopicCatalog {
    pub fn phase2_default() -> Self {
        let mut catalog = Self::default();
        for topic in phase2_topics() {
            catalog.insert(topic);
        }
        catalog
    }

    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, EventbusError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| EventbusError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let raw: RawTopicCatalog =
            toml::from_str(&text).map_err(|source| EventbusError::Parse {
                path: path.display().to_string(),
                source,
            })?;
        Ok(Self::from_topics(raw.topics))
    }

    pub fn from_topics(topics: Vec<TopicConfig>) -> Self {
        let mut catalog = Self::default();
        for topic in topics {
            catalog.insert(topic);
        }
        catalog
    }

    pub fn get(&self, name: &str) -> Option<&TopicConfig> {
        self.topics.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TopicConfig> {
        self.topics.values()
    }

    fn insert(&mut self, topic: TopicConfig) {
        self.topics.insert(topic.name.clone(), topic);
    }
}

#[async_trait]
pub trait EventProducer {
    async fn publish(&self, topic: &str, key: &[u8], payload: &[u8]) -> Result<(), EventbusError>;
}

#[async_trait]
pub trait EventConsumer {
    async fn poll(&self) -> Result<Option<EventEnvelope>, EventbusError>;
    async fn commit(&self, envelope: &EventEnvelope) -> Result<(), EventbusError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventEnvelope {
    pub topic: String,
    pub key: Vec<u8>,
    pub payload: Vec<u8>,
    pub offset: i64,
    pub partition: i32,
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryEventProducer {
    events: Arc<Mutex<Vec<EventEnvelope>>>,
}

impl InMemoryEventProducer {
    pub fn events(&self) -> Vec<EventEnvelope> {
        self.events
            .lock()
            .expect("event producer mutex poisoned")
            .clone()
    }

    pub fn clear(&self) {
        self.events
            .lock()
            .expect("event producer mutex poisoned")
            .clear();
    }
}

#[async_trait]
impl EventProducer for InMemoryEventProducer {
    async fn publish(&self, topic: &str, key: &[u8], payload: &[u8]) -> Result<(), EventbusError> {
        let mut events = self.events.lock().expect("event producer mutex poisoned");
        let offset = events.len() as i64;
        events.push(EventEnvelope {
            topic: topic.to_string(),
            key: key.to_vec(),
            payload: payload.to_vec(),
            offset,
            partition: 0,
        });
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct RawTopicCatalog {
    topics: Vec<TopicConfig>,
}

fn phase2_topics() -> Vec<TopicConfig> {
    vec![
        topic(
            "raw.therundown",
            "provider_event_id",
            "adapter-therundown",
            &["normalizer", "raw-archive", "replay"],
            14,
        ),
        topic(
            "raw.polymarket.market",
            "provider_market_id",
            "adapter-polymarket-market",
            &["normalizer", "raw-archive", "replay"],
            14,
        ),
        topic(
            "raw.polymarket.user",
            "venue_order_id",
            "adapter-polymarket-user",
            &["archive", "execution-sync"],
            90,
        ),
        topic(
            "norm.quote",
            "canonical_market_key",
            "normalizer",
            &["mapper", "latency", "ch-sink"],
            14,
        ),
        topic(
            "mapping.decision",
            "canonical_event_id",
            "canonical-mapper",
            &["api", "review"],
            30,
        ),
        topic(
            "latency.sample",
            "canonical_market_key",
            "latency-engine",
            &["alert", "api"],
            30,
        ),
        topic(
            "signal.event",
            "canonical_market_key",
            "signal-engine",
            &["api", "ch-sink"],
            30,
        ),
        topic("order.intent", "intent_id", "signal-engine", &["risk"], 90),
        topic(
            "risk.decision",
            "intent_id",
            "risk-engine",
            &["paper", "execution", "api"],
            90,
        ),
        topic(
            "execution.request",
            "venue_account_id",
            "risk-manual",
            &["execution-gateway-pm"],
            90,
        ),
        topic(
            "execution.receipt",
            "venue_order_id",
            "execution-user-adapter",
            &["ledger", "audit", "reconcile"],
            365,
        ),
        topic(
            "paper.fill",
            "paper_order_id",
            "paper-broker",
            &["replay", "api", "analytics"],
            180,
        ),
        topic(
            "dlq.raw",
            "message_hash",
            "any-service",
            &["operator", "replay"],
            30,
        ),
    ]
}

fn topic(
    name: &str,
    key: &str,
    producer: &str,
    consumers: &[&str],
    retention_days: u32,
) -> TopicConfig {
    TopicConfig {
        name: name.to_string(),
        key: key.to_string(),
        producer: producer.to_string(),
        consumers: consumers
            .iter()
            .map(|consumer| (*consumer).to_string())
            .collect(),
        partitions: 3,
        replicas: 1,
        retention: TopicRetention::Days(retention_days),
    }
}
