use crate::polymarket::error::PolymarketError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quantsys_domain::{RawMessage, SourceChannel};
use quantsys_eventbus::{EventProducer, EventbusError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const RAW_POLYMARKET_MARKET_TOPIC: &str = "raw.polymarket.market";
pub const RAW_POLYMARKET_USER_TOPIC: &str = "raw.polymarket.user";
pub const DLQ_RAW_TOPIC: &str = "dlq.raw";

#[derive(Clone, Debug)]
pub struct RawPublisher<P> {
    producer: P,
}

impl<P> RawPublisher<P>
where
    P: EventProducer,
{
    pub fn new(producer: P) -> Self {
        Self { producer }
    }

    pub fn inner(&self) -> &P {
        &self.producer
    }

    pub async fn publish_raw(&self, raw: &RawMessage) -> Result<(), PolymarketError> {
        let topic = topic_for_channel(&raw.source_channel);
        let key = raw
            .provider_market_id
            .as_ref()
            .or(raw.provider_event_id.as_ref())
            .or(raw.provider_message_id.as_ref())
            .unwrap_or(&raw.raw_id)
            .as_bytes()
            .to_vec();
        let payload =
            serde_json::to_vec(raw).map_err(|err| PolymarketError::Transport(err.to_string()))?;
        self.producer
            .publish(topic, &key, &payload)
            .await
            .map_err(eventbus_err)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DlqRecord {
    pub error_code: String,
    pub error_message: String,
    pub provider: String,
    pub source_channel: String,
    pub payload_hash: String,
    pub raw_ref: String,
    pub received_at: DateTime<Utc>,
    pub schema_version: String,
    pub trace_id: String,
}

#[async_trait]
pub trait DlqSink: Clone + Send + Sync + 'static {
    async fn publish_dlq(&self, record: DlqRecord) -> Result<(), PolymarketError>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryDlqSink {
    records: Arc<Mutex<Vec<DlqRecord>>>,
}

impl InMemoryDlqSink {
    pub fn records(&self) -> Vec<DlqRecord> {
        self.records.lock().expect("dlq mutex poisoned").clone()
    }
}

#[async_trait]
impl DlqSink for InMemoryDlqSink {
    async fn publish_dlq(&self, record: DlqRecord) -> Result<(), PolymarketError> {
        self.records
            .lock()
            .expect("dlq mutex poisoned")
            .push(record);
        Ok(())
    }
}

fn topic_for_channel(channel: &SourceChannel) -> &'static str {
    match channel {
        SourceChannel::WsUser => RAW_POLYMARKET_USER_TOPIC,
        SourceChannel::RestBootstrap
        | SourceChannel::RestDelta
        | SourceChannel::RestDiscovery
        | SourceChannel::WsMarket
        | SourceChannel::RestGeoblock
        | SourceChannel::RestTime
        | SourceChannel::RestClob => RAW_POLYMARKET_MARKET_TOPIC,
    }
}

fn eventbus_err(err: EventbusError) -> PolymarketError {
    PolymarketError::Transport(err.to_string())
}
