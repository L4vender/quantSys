use crate::config::RawArchiveProcessorConfig;
use crate::dlq::{dlq_for_error, infer_provider_channel};
use crate::error::RawArchiveError;
use chrono::Utc;
use quantsys_domain::{
    compute_payload_hash, compute_raw_id, scan_json_for_secrets, Provider, RawArchiveStatus,
    RawMessage, SourceChannel, SourceMode, SourceState, SourceStatus,
};
use quantsys_eventbus::EventEnvelope;
use quantsys_storage::{
    ArchiveReadRequest, ArchiveWriteRequest, InMemoryDlqStore, InMemoryObjectArchive,
    InMemoryRawArchiveIndex, InMemorySourceStateStore, ObjectArchive, ObjectKeyBuilder,
    RawArchiveIndexRecord,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RawArchiveProcessResult {
    pub raw_id: String,
    pub raw_ref: String,
    pub duplicate: bool,
    pub archive_status: RawArchiveStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RawPayloadRead {
    pub raw_ref: String,
    pub payload: Value,
}

#[derive(Clone)]
pub struct RawArchiveProcessor {
    config: RawArchiveProcessorConfig,
    archive: InMemoryObjectArchive,
    index: InMemoryRawArchiveIndex,
    dlq: InMemoryDlqStore,
    source_states: InMemorySourceStateStore,
    key_builder: ObjectKeyBuilder,
}

impl RawArchiveProcessor {
    pub fn new(
        config: RawArchiveProcessorConfig,
        archive: InMemoryObjectArchive,
        index: InMemoryRawArchiveIndex,
        dlq: InMemoryDlqStore,
        source_states: InMemorySourceStateStore,
    ) -> Self {
        let key_builder = ObjectKeyBuilder::new(config.object_key_prefix.clone());
        Self {
            config,
            archive,
            index,
            dlq,
            source_states,
            key_builder,
        }
    }

    pub async fn process_envelope(
        &self,
        envelope: EventEnvelope,
    ) -> Result<RawArchiveProcessResult, RawArchiveError> {
        match self.process_envelope_inner(envelope.clone()).await {
            Ok(result) => Ok(result),
            Err(err) => {
                let (provider, source_channel) = infer_provider_channel(&envelope.topic);
                let dlq_event = dlq_for_error(
                    None,
                    provider,
                    envelope.topic,
                    source_channel,
                    &err,
                    &envelope.payload,
                    Utc::now(),
                    &self.key_builder,
                    &self.archive,
                );
                self.dlq.insert(dlq_event);
                Err(err)
            }
        }
    }

    fn validate_raw(&self, raw: &RawMessage, topic: &str) -> Result<(), RawArchiveError> {
        if !matches!(
            topic,
            "raw.therundown" | "raw.polymarket.market" | "raw.polymarket.user"
        ) {
            return Err(RawArchiveError::InvalidEnvelope(format!(
                "unsupported raw topic {topic}"
            )));
        }
        match (&raw.provider, topic, &raw.source_channel) {
            (Provider::TheRundown, "raw.therundown", _) => {}
            (Provider::Polymarket, "raw.polymarket.user", SourceChannel::WsUser) => {}
            (Provider::Polymarket, "raw.polymarket.market", channel)
                if *channel != SourceChannel::WsUser => {}
            _ => {
                return Err(RawArchiveError::Validation(
                    "provider/source_channel/topic mismatch".to_string(),
                ));
            }
        }

        let expected_payload_hash = compute_payload_hash(&raw.payload);
        if raw.payload_hash != expected_payload_hash {
            return Err(RawArchiveError::Validation(
                "payload_hash mismatch".to_string(),
            ));
        }
        let expected_raw_id = compute_raw_id(
            &raw.provider,
            &raw.source_channel,
            raw.provider_message_id.as_deref(),
            raw.provider_event_id.as_deref(),
            raw.provider_market_id.as_deref(),
            &raw.payload_hash,
        );
        if raw.raw_id != expected_raw_id {
            return Err(RawArchiveError::Validation("raw_id mismatch".to_string()));
        }
        scan_json_for_secrets(&raw.payload)
            .map_err(|err| RawArchiveError::Validation(err.to_string()))?;
        Ok(())
    }

    async fn process_envelope_inner(
        &self,
        envelope: EventEnvelope,
    ) -> Result<RawArchiveProcessResult, RawArchiveError> {
        let mut raw: RawMessage = serde_json::from_slice(&envelope.payload)?;
        self.validate_raw(&raw, &envelope.topic)?;

        let raw_ref = self.key_builder.raw_archive_key(
            raw.provider.slug(),
            raw.source_channel.slug(),
            &raw.raw_id,
            raw.received_at,
        );
        raw.raw_ref = raw_ref.clone();
        raw.archive_status = RawArchiveStatus::Archived;

        let payload_bytes = serde_json::to_vec(&raw.payload)?;
        let write = self.archive.write(ArchiveWriteRequest::json(
            raw_ref.clone(),
            payload_bytes,
            raw.raw_id.clone(),
        ))?;
        let mut record = RawArchiveIndexRecord::from_raw(&raw, RawArchiveStatus::Archived);
        if write.duplicate {
            record.archive_status = RawArchiveStatus::Duplicate;
        }
        let upsert = self.index.upsert(record);
        self.update_source_state(&raw);

        Ok(RawArchiveProcessResult {
            raw_id: raw.raw_id,
            raw_ref,
            duplicate: write.duplicate || upsert.duplicate,
            archive_status: if write.duplicate || upsert.duplicate {
                RawArchiveStatus::Duplicate
            } else {
                RawArchiveStatus::Archived
            },
        })
    }

    pub fn read_by_ref(&self, raw_ref: &str) -> Result<RawPayloadRead, RawArchiveError> {
        let read = self
            .archive
            .read(ArchiveReadRequest::by_ref(raw_ref.to_string()))?;
        let payload = serde_json::from_slice(&read.bytes)?;
        Ok(RawPayloadRead {
            raw_ref: read.raw_ref,
            payload,
        })
    }

    pub fn index(&self) -> InMemoryRawArchiveIndex {
        self.index.clone()
    }

    pub fn dlq(&self) -> InMemoryDlqStore {
        self.dlq.clone()
    }

    pub fn source_states(&self) -> InMemorySourceStateStore {
        self.source_states.clone()
    }

    fn update_source_state(&self, raw: &RawMessage) {
        let source = match (&raw.provider, &raw.source_channel) {
            (Provider::TheRundown, _) => "therundown",
            (Provider::Polymarket, SourceChannel::WsUser) => "polymarket_user",
            (Provider::Polymarket, SourceChannel::RestGeoblock) => "polymarket_geoblock",
            (Provider::Polymarket, SourceChannel::RestTime) => "polymarket_time",
            (Provider::Polymarket, _) => "polymarket_market",
        };
        let state = SourceState {
            source: source.to_string(),
            mode: match raw.source_channel {
                SourceChannel::RestBootstrap => SourceMode::RestBootstrap,
                SourceChannel::RestDelta => SourceMode::RestDelta,
                SourceChannel::RestDiscovery => SourceMode::RestDiscovery,
                SourceChannel::RestGeoblock => SourceMode::RestGeoblock,
                SourceChannel::RestTime => SourceMode::RestTime,
                SourceChannel::WsMarket | SourceChannel::WsUser => SourceMode::LiveWs,
                SourceChannel::RestClob => SourceMode::Mock,
            },
            tier: None,
            data_delay_seconds: None,
            websocket_access: Some(matches!(
                raw.source_channel,
                SourceChannel::WsMarket | SourceChannel::WsUser
            )),
            status: SourceStatus::Ok,
            last_message_at: Some(raw.received_at),
            last_heartbeat_at: None,
            stale_after_seconds: self.config.stale_after_seconds,
            rate_limited: false,
            geoblocked: false,
            error: None,
            live_signal_allowed: matches!(
                (&raw.provider, &raw.source_channel),
                (Provider::TheRundown, _)
                    | (Provider::Polymarket, SourceChannel::WsMarket)
                    | (Provider::Polymarket, SourceChannel::RestDiscovery)
            ),
            live_execution_allowed: false,
            block_reason: None,
        };
        self.source_states
            .update(source, raw.provider.clone(), state);
    }
}
