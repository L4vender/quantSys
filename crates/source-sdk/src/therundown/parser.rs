use crate::therundown::error::TheRundownError;
use chrono::{DateTime, Utc};
use quantsys_domain::{Provider, QualityFlags, RawMessage, SourceChannel};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedPayloadKind {
    MarketPrice,
    Heartbeat,
    Unknown { meta_type: Option<String> },
    RestBootstrap { delta_last_id: Option<String> },
    RestDelta { next_last_id: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedPayload {
    pub raw: RawMessage,
    pub kind: ParsedPayloadKind,
    pub quality_flags: QualityFlags,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParserError {
    MissingRequiredField { field: String },
    InvalidPayload { message: String },
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequiredField { field } => {
                write!(f, "missing required field {field}")
            }
            Self::InvalidPayload { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for ParserError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheRundownParser {
    schema_version: String,
}

impl TheRundownParser {
    pub fn new(schema_version: impl Into<String>) -> Self {
        Self {
            schema_version: schema_version.into(),
        }
    }

    pub fn parse_ws_payload(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<ParsedPayload, ParserError> {
        let meta_type = payload
            .pointer("/meta/type")
            .and_then(Value::as_str)
            .map(str::to_string);

        match meta_type.as_deref() {
            Some("market_price") => self.parse_market_price(payload, received_at, received_mono_ns),
            Some("heartbeat") => Ok(self.parse_heartbeat(payload, received_at, received_mono_ns)),
            _ => Ok(self.parse_unknown(payload, received_at, received_mono_ns, meta_type)),
        }
    }

    pub fn parse_rest_bootstrap(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<ParsedPayload, TheRundownError> {
        let delta_last_id = payload
            .pointer("/meta/delta_last_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let provider_event_id = first_event_id(&payload);
        let provider_market_id = first_bootstrap_market_id(&payload);
        let raw = self.raw(RawFields {
            source_channel: SourceChannel::RestBootstrap,
            provider_message_id: delta_last_id.clone(),
            provider_event_id,
            provider_market_id,
            received_at,
            received_mono_ns,
            payload,
        });
        Ok(ParsedPayload {
            raw,
            kind: ParsedPayloadKind::RestBootstrap { delta_last_id },
            quality_flags: QualityFlags::default(),
        })
    }

    pub fn parse_rest_delta(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<ParsedPayload, TheRundownError> {
        let next_last_id = payload
            .pointer("/meta/next_last_id")
            .or_else(|| payload.pointer("/meta/last_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let first = payload
            .get("data")
            .and_then(Value::as_array)
            .and_then(|items| items.first());
        let provider_message_id = first
            .and_then(|item| item.get("id"))
            .and_then(value_to_string)
            .or_else(|| next_last_id.clone());
        let provider_event_id = first
            .and_then(|item| item.get("event_id"))
            .and_then(value_to_string);
        let provider_market_id = first
            .and_then(|item| item.get("market_id"))
            .and_then(value_to_string);

        let raw = self.raw(RawFields {
            source_channel: SourceChannel::RestDelta,
            provider_message_id,
            provider_event_id,
            provider_market_id,
            received_at,
            received_mono_ns,
            payload,
        });
        Ok(ParsedPayload {
            raw,
            kind: ParsedPayloadKind::RestDelta { next_last_id },
            quality_flags: QualityFlags::default(),
        })
    }

    fn parse_market_price(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<ParsedPayload, ParserError> {
        let data = payload
            .get("data")
            .and_then(Value::as_object)
            .ok_or_else(|| ParserError::MissingRequiredField {
                field: "data".to_string(),
            })?;
        for field in REQUIRED_MARKET_PRICE_FIELDS {
            if data.get(*field).is_none() {
                return Err(ParserError::MissingRequiredField {
                    field: format!("data.{field}"),
                });
            }
        }

        let provider_message_id = data.get("id").and_then(value_to_string);
        let provider_event_id = data.get("event_id").and_then(value_to_string);
        let provider_market_id = data.get("market_id").and_then(value_to_string);
        let off_board = data.get("price").is_some_and(is_off_board_price);
        let quality_flags = QualityFlags {
            off_board,
            ..QualityFlags::default()
        };

        let raw = self.raw(RawFields {
            source_channel: SourceChannel::WsMarket,
            provider_message_id,
            provider_event_id,
            provider_market_id,
            received_at,
            received_mono_ns,
            payload,
        });
        Ok(ParsedPayload {
            raw,
            kind: ParsedPayloadKind::MarketPrice,
            quality_flags,
        })
    }

    fn parse_heartbeat(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> ParsedPayload {
        let provider_message_id = payload
            .pointer("/data/now")
            .and_then(value_to_string)
            .or_else(|| payload.pointer("/meta/timestamp").and_then(value_to_string))
            .map(|value| format!("heartbeat:{value}"));
        let raw = self.raw(RawFields {
            source_channel: SourceChannel::WsMarket,
            provider_message_id,
            provider_event_id: None,
            provider_market_id: None,
            received_at,
            received_mono_ns,
            payload,
        });
        ParsedPayload {
            raw,
            kind: ParsedPayloadKind::Heartbeat,
            quality_flags: QualityFlags::default(),
        }
    }

    fn parse_unknown(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
        meta_type: Option<String>,
    ) -> ParsedPayload {
        let provider_message_id = payload
            .pointer("/meta/timestamp")
            .and_then(value_to_string)
            .or_else(|| payload.pointer("/data/id").and_then(value_to_string));
        let provider_event_id = payload.pointer("/data/event_id").and_then(value_to_string);
        let provider_market_id = payload.pointer("/data/market_id").and_then(value_to_string);
        let raw = self.raw(RawFields {
            source_channel: SourceChannel::WsMarket,
            provider_message_id,
            provider_event_id,
            provider_market_id,
            received_at,
            received_mono_ns,
            payload,
        });
        let quality_flags = QualityFlags {
            unknown_schema: true,
            ..QualityFlags::default()
        };
        ParsedPayload {
            raw,
            kind: ParsedPayloadKind::Unknown { meta_type },
            quality_flags,
        }
    }

    fn raw(&self, fields: RawFields) -> RawMessage {
        let raw_ref = raw_ref(
            &fields.source_channel,
            fields.provider_event_id.as_deref(),
            fields.provider_message_id.as_deref(),
            &payload_hash(&fields.payload),
        );
        RawMessage::new(
            Provider::TheRundown,
            fields.source_channel,
            fields.provider_message_id,
            fields.provider_event_id,
            fields.provider_market_id,
            fields.received_at,
            fields.received_mono_ns,
            raw_ref,
            self.schema_version.clone(),
            fields.payload,
        )
    }
}

struct RawFields {
    source_channel: SourceChannel,
    provider_message_id: Option<String>,
    provider_event_id: Option<String>,
    provider_market_id: Option<String>,
    received_at: DateTime<Utc>,
    received_mono_ns: u64,
    payload: Value,
}

pub fn payload_hash(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).expect("serde_json::Value serialization cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", to_hex(&hasher.finalize()))
}

pub fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn first_event_id(payload: &Value) -> Option<String> {
    payload
        .get("events")
        .and_then(Value::as_array)
        .and_then(|events| events.first())
        .and_then(|event| event.get("event_id"))
        .and_then(value_to_string)
}

fn first_bootstrap_market_id(payload: &Value) -> Option<String> {
    payload
        .get("events")
        .and_then(Value::as_array)
        .and_then(|events| events.first())
        .and_then(|event| event.get("markets"))
        .and_then(Value::as_array)
        .and_then(|markets| markets.first())
        .and_then(|market| market.get("market_id"))
        .and_then(value_to_string)
}

fn is_off_board_price(value: &Value) -> bool {
    match value {
        Value::String(value) => value.trim() == "0.0001",
        Value::Number(value) => value.to_string() == "0.0001",
        _ => false,
    }
}

fn raw_ref(
    source_channel: &SourceChannel,
    provider_event_id: Option<&str>,
    provider_message_id: Option<&str>,
    payload_hash: &str,
) -> String {
    let channel = match source_channel {
        SourceChannel::RestBootstrap => "rest_bootstrap",
        SourceChannel::RestDelta => "rest_delta",
        SourceChannel::RestDiscovery => "rest_discovery",
        SourceChannel::WsMarket => "ws_market",
        SourceChannel::WsUser => "ws_user",
        SourceChannel::RestGeoblock => "rest_geoblock",
        SourceChannel::RestTime => "rest_time",
        SourceChannel::RestClob => "rest_clob",
    };
    let event = provider_event_id.unwrap_or("unknown_event");
    let message = provider_message_id.unwrap_or("unknown_message");
    let hash = payload_hash.trim_start_matches("sha256:");
    format!("raw/therundown/{channel}/{event}/{message}/{hash}.json")
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

const REQUIRED_MARKET_PRICE_FIELDS: &[&str] = &[
    "id",
    "event_id",
    "affiliate_id",
    "market_id",
    "market_participant_id",
    "normalized_market_participant_id",
    "line",
    "price",
    "previous_price",
    "is_main_line",
    "sport_id",
    "updated_at",
];
