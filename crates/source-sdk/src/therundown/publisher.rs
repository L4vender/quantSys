use crate::therundown::error::TheRundownError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quantsys_domain::RawMessage;
use quantsys_eventbus::{EventProducer, EventbusError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

pub const RAW_THERUNDOWN_TOPIC: &str = "raw.therundown";
pub const DLQ_EXTERNAL_TOPIC: &str = "dlq.external";

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

    pub async fn publish_raw(&self, raw: &RawMessage) -> Result<(), TheRundownError> {
        let key = raw
            .provider_event_id
            .as_ref()
            .or(raw.provider_market_id.as_ref())
            .unwrap_or(&raw.raw_id)
            .as_bytes()
            .to_vec();
        let payload =
            serde_json::to_vec(raw).map_err(|err| TheRundownError::Transport(err.to_string()))?;
        self.producer
            .publish(RAW_THERUNDOWN_TOPIC, &key, &payload)
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
    async fn publish_dlq(&self, record: DlqRecord) -> Result<(), TheRundownError>;
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
    async fn publish_dlq(&self, record: DlqRecord) -> Result<(), TheRundownError> {
        self.records
            .lock()
            .expect("dlq mutex poisoned")
            .push(record);
        Ok(())
    }
}

fn eventbus_err(err: EventbusError) -> TheRundownError {
    TheRundownError::Transport(err.to_string())
}
