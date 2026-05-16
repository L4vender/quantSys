use chrono::{DateTime, Utc};
use quantsys_domain::{Provider, RawArchiveStatus, RawMessage, SourceChannel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RawArchiveIndexRecord {
    pub raw_id: String,
    pub provider: Provider,
    pub topic: String,
    pub source_channel: SourceChannel,
    pub provider_message_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub provider_market_id: Option<String>,
    pub payload_hash: String,
    pub raw_ref: String,
    pub schema_version: String,
    pub trace_id: String,
    pub received_at: DateTime<Utc>,
    pub archived_at: DateTime<Utc>,
    pub archive_status: RawArchiveStatus,
    pub payload_size_bytes: u64,
    pub quality_flags: serde_json::Value,
    pub duplicate_count: u64,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RawArchiveIndexRecord {
    pub fn from_raw(raw: &RawMessage, archive_status: RawArchiveStatus) -> Self {
        let now = Utc::now();
        Self {
            raw_id: raw.raw_id.clone(),
            provider: raw.provider.clone(),
            topic: topic_for(&raw.provider, &raw.source_channel).to_string(),
            source_channel: raw.source_channel.clone(),
            provider_message_id: raw.provider_message_id.clone(),
            provider_event_id: raw.provider_event_id.clone(),
            provider_market_id: raw.provider_market_id.clone(),
            payload_hash: raw.payload_hash.clone(),
            raw_ref: raw.raw_ref.clone(),
            schema_version: raw.schema_version.clone(),
            trace_id: raw.trace_id.to_string(),
            received_at: raw.received_at,
            archived_at: now,
            archive_status,
            payload_size_bytes: serde_json::to_vec(&raw.payload)
                .map(|bytes| bytes.len() as u64)
                .unwrap_or(0),
            quality_flags: serde_json::to_value(&raw.quality_flags)
                .unwrap_or_else(|_| serde_json::json!({})),
            duplicate_count: 0,
            last_seen_at: now,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RawArchiveSearchQuery {
    pub provider: Option<Provider>,
    pub topic: Option<String>,
    pub source_channel: Option<SourceChannel>,
    pub provider_event_id: Option<String>,
    pub provider_market_id: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawArchiveUpsertResult {
    pub raw_id: String,
    pub duplicate: bool,
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryRawArchiveIndex {
    state: Arc<Mutex<RawArchiveIndexState>>,
}

#[derive(Clone, Debug, Default)]
struct RawArchiveIndexState {
    by_raw_id: BTreeMap<String, RawArchiveIndexRecord>,
    by_raw_ref: BTreeMap<String, String>,
}

impl InMemoryRawArchiveIndex {
    pub fn upsert(&self, mut record: RawArchiveIndexRecord) -> RawArchiveUpsertResult {
        let mut state = self.state.lock().expect("raw archive index mutex poisoned");
        if let Some(existing) = state.by_raw_id.get_mut(&record.raw_id) {
            existing.duplicate_count += 1;
            existing.last_seen_at = Utc::now();
            existing.updated_at = Utc::now();
            existing.archive_status = RawArchiveStatus::Duplicate;
            return RawArchiveUpsertResult {
                raw_id: existing.raw_id.clone(),
                duplicate: true,
            };
        }

        record.updated_at = Utc::now();
        state
            .by_raw_ref
            .insert(record.raw_ref.clone(), record.raw_id.clone());
        let raw_id = record.raw_id.clone();
        state.by_raw_id.insert(raw_id.clone(), record);
        RawArchiveUpsertResult {
            raw_id,
            duplicate: false,
        }
    }

    pub fn get(&self, raw_id: &str) -> Option<RawArchiveIndexRecord> {
        self.state
            .lock()
            .expect("raw archive index mutex poisoned")
            .by_raw_id
            .get(raw_id)
            .cloned()
    }

    pub fn get_by_raw_ref(&self, raw_ref: &str) -> Option<RawArchiveIndexRecord> {
        let state = self.state.lock().expect("raw archive index mutex poisoned");
        let raw_id = state.by_raw_ref.get(raw_ref)?;
        state.by_raw_id.get(raw_id).cloned()
    }

    pub fn search(&self, query: RawArchiveSearchQuery) -> Vec<RawArchiveIndexRecord> {
        self.state
            .lock()
            .expect("raw archive index mutex poisoned")
            .by_raw_id
            .values()
            .filter(|record| {
                query
                    .provider
                    .as_ref()
                    .is_none_or(|value| value == &record.provider)
            })
            .filter(|record| {
                query
                    .topic
                    .as_ref()
                    .is_none_or(|value| value == &record.topic)
            })
            .filter(|record| {
                query
                    .source_channel
                    .as_ref()
                    .is_none_or(|value| value == &record.source_channel)
            })
            .filter(|record| {
                query
                    .provider_event_id
                    .as_ref()
                    .is_none_or(|value| record.provider_event_id.as_ref() == Some(value))
            })
            .filter(|record| {
                query
                    .provider_market_id
                    .as_ref()
                    .is_none_or(|value| record.provider_market_id.as_ref() == Some(value))
            })
            .filter(|record| query.from.is_none_or(|from| record.received_at >= from))
            .filter(|record| query.to.is_none_or(|to| record.received_at <= to))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.state
            .lock()
            .expect("raw archive index mutex poisoned")
            .by_raw_id
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn list(&self) -> Vec<RawArchiveIndexRecord> {
        self.state
            .lock()
            .expect("raw archive index mutex poisoned")
            .by_raw_id
            .values()
            .cloned()
            .collect()
    }
}

fn topic_for(provider: &Provider, channel: &SourceChannel) -> &'static str {
    match (provider, channel) {
        (Provider::TheRundown, _) => "raw.therundown",
        (Provider::Polymarket, SourceChannel::WsUser) => "raw.polymarket.user",
        (Provider::Polymarket, _) => "raw.polymarket.market",
    }
}
