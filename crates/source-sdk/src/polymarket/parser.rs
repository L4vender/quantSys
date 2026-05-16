use crate::polymarket::discovery::{
    parse_discovery_payload, raw_from_fields, DiscoveryFilters, DiscoveryResult,
};
use crate::polymarket::error::redact_secret_json;
use chrono::{DateTime, Utc};
use quantsys_domain::{QualityFlags, RawMessage, SourceChannel};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedPolymarketKind {
    MarketBook,
    MarketPriceChange,
    MarketBestBidAsk,
    MarketLastTradePrice,
    MarketTickSizeChange,
    NewMarket,
    MarketResolved,
    UserOrder,
    UserFill,
    UserOrderUpdate,
    Unknown { event_type: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedPolymarketPayload {
    pub raw: RawMessage,
    pub kind: ParsedPolymarketKind,
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
            Self::MissingRequiredField { field } => write!(f, "missing required field {field}"),
            Self::InvalidPayload { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for ParserError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolymarketParser {
    schema_version: String,
}

impl PolymarketParser {
    pub fn new(schema_version: impl Into<String>) -> Self {
        Self {
            schema_version: schema_version.into(),
        }
    }

    pub fn parse_discovery_payload(
        &self,
        payload: Value,
        filters: &DiscoveryFilters,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<DiscoveryResult, ParserError> {
        parse_discovery_payload(
            &self.schema_version,
            payload,
            filters,
            received_at,
            received_mono_ns,
        )
    }

    pub fn parse_market_ws_payload(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<ParsedPolymarketPayload, ParserError> {
        let event_type = payload
            .get("event_type")
            .and_then(Value::as_str)
            .map(str::to_string);
        match event_type.as_deref() {
            Some("book") => self.parse_market_event(
                payload,
                received_at,
                received_mono_ns,
                ParsedPolymarketKind::MarketBook,
                &["market", "asset_id", "timestamp", "bids", "asks"],
            ),
            Some("price_change") => {
                require_field(&payload, "market")?;
                require_field(&payload, "timestamp")?;
                if payload.get("changes").is_none() && payload.get("price_changes").is_none() {
                    return Err(ParserError::MissingRequiredField {
                        field: "changes".to_string(),
                    });
                }
                Ok(self.parsed_market(
                    payload,
                    received_at,
                    received_mono_ns,
                    ParsedPolymarketKind::MarketPriceChange,
                    QualityFlags::default(),
                ))
            }
            Some("best_bid_ask") => self.parse_market_event(
                payload,
                received_at,
                received_mono_ns,
                ParsedPolymarketKind::MarketBestBidAsk,
                &["market", "asset_id", "timestamp", "best_bid", "best_ask"],
            ),
            Some("last_trade_price") => self.parse_market_event(
                payload,
                received_at,
                received_mono_ns,
                ParsedPolymarketKind::MarketLastTradePrice,
                &["market", "asset_id", "timestamp", "price"],
            ),
            Some("tick_size_change") => self.parse_market_event(
                payload,
                received_at,
                received_mono_ns,
                ParsedPolymarketKind::MarketTickSizeChange,
                &["market", "asset_id", "timestamp"],
            ),
            Some("new_market") => self.parse_market_event(
                payload,
                received_at,
                received_mono_ns,
                ParsedPolymarketKind::NewMarket,
                &["market", "timestamp"],
            ),
            Some("market_resolved") => self.parse_market_event(
                payload,
                received_at,
                received_mono_ns,
                ParsedPolymarketKind::MarketResolved,
                &["market", "timestamp"],
            ),
            _ => {
                let quality_flags = QualityFlags {
                    unknown_schema: true,
                    ..QualityFlags::default()
                };
                Ok(self.parsed_market(
                    payload,
                    received_at,
                    received_mono_ns,
                    ParsedPolymarketKind::Unknown { event_type },
                    quality_flags,
                ))
            }
        }
    }

    pub fn parse_user_ws_payload(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<ParsedPolymarketPayload, ParserError> {
        let sanitized = redact_secret_json(&payload);
        let event_type = sanitized
            .get("event_type")
            .or_else(|| sanitized.get("type"))
            .and_then(Value::as_str)
            .map(str::to_string);
        match event_type.as_deref() {
            Some("order") | Some("ORDER") => {
                require_user_market(&sanitized)?;
                Ok(self.parsed_user(
                    sanitized,
                    received_at,
                    received_mono_ns,
                    ParsedPolymarketKind::UserOrder,
                    QualityFlags::default(),
                ))
            }
            Some("order_update") | Some("ORDER_UPDATE") => {
                require_user_market(&sanitized)?;
                Ok(self.parsed_user(
                    sanitized,
                    received_at,
                    received_mono_ns,
                    ParsedPolymarketKind::UserOrderUpdate,
                    QualityFlags::default(),
                ))
            }
            Some("trade") | Some("TRADE") | Some("fill") | Some("FILL") => {
                require_user_market(&sanitized)?;
                Ok(self.parsed_user(
                    sanitized,
                    received_at,
                    received_mono_ns,
                    ParsedPolymarketKind::UserFill,
                    QualityFlags::default(),
                ))
            }
            _ => {
                let quality_flags = QualityFlags {
                    unknown_schema: true,
                    ..QualityFlags::default()
                };
                Ok(self.parsed_user(
                    sanitized,
                    received_at,
                    received_mono_ns,
                    ParsedPolymarketKind::Unknown { event_type },
                    quality_flags,
                ))
            }
        }
    }

    fn parse_market_event(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
        kind: ParsedPolymarketKind,
        required_fields: &[&str],
    ) -> Result<ParsedPolymarketPayload, ParserError> {
        for field in required_fields {
            require_field(&payload, field)?;
        }
        Ok(self.parsed_market(
            payload,
            received_at,
            received_mono_ns,
            kind,
            QualityFlags::default(),
        ))
    }

    fn parsed_market(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
        kind: ParsedPolymarketKind,
        quality_flags: QualityFlags,
    ) -> ParsedPolymarketPayload {
        let provider_event_id = payload.get("market").and_then(value_to_string);
        let provider_message_id = payload
            .get("hash")
            .and_then(value_to_string)
            .or_else(|| payload.get("timestamp").and_then(value_to_string))
            .or_else(|| payload.get("id").and_then(value_to_string));
        let provider_market_id = payload
            .get("asset_id")
            .and_then(value_to_string)
            .or_else(|| provider_event_id.clone());
        let raw = raw_from_fields(
            &self.schema_version,
            RawFields {
                source_channel: SourceChannel::WsMarket,
                provider_message_id,
                provider_event_id,
                provider_market_id,
                received_at,
                received_mono_ns,
                payload,
            },
        );
        ParsedPolymarketPayload {
            raw,
            kind,
            quality_flags,
        }
    }

    fn parsed_user(
        &self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
        kind: ParsedPolymarketKind,
        quality_flags: QualityFlags,
    ) -> ParsedPolymarketPayload {
        let provider_event_id = payload.get("market").and_then(value_to_string);
        let provider_message_id = payload
            .get("id")
            .and_then(value_to_string)
            .or_else(|| payload.pointer("/order/id").and_then(value_to_string))
            .or_else(|| payload.get("timestamp").and_then(value_to_string));
        let provider_market_id = payload
            .get("asset_id")
            .and_then(value_to_string)
            .or_else(|| provider_event_id.clone());
        let raw = raw_from_fields(
            &self.schema_version,
            RawFields {
                source_channel: SourceChannel::WsUser,
                provider_message_id,
                provider_event_id,
                provider_market_id,
                received_at,
                received_mono_ns,
                payload,
            },
        );
        ParsedPolymarketPayload {
            raw,
            kind,
            quality_flags,
        }
    }
}

pub(crate) struct RawFields {
    pub source_channel: SourceChannel,
    pub provider_message_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub provider_market_id: Option<String>,
    pub received_at: DateTime<Utc>,
    pub received_mono_ns: u64,
    pub payload: Value,
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

fn require_field(payload: &Value, field: &str) -> Result<(), ParserError> {
    if payload.get(field).is_none() {
        return Err(ParserError::MissingRequiredField {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn require_user_market(payload: &Value) -> Result<(), ParserError> {
    require_field(payload, "market")?;
    if payload.get("id").is_none() && payload.pointer("/order/id").is_none() {
        return Err(ParserError::MissingRequiredField {
            field: "id".to_string(),
        });
    }
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
