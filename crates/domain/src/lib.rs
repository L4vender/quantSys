use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    TheRundown,
    Polymarket,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChannel {
    RestBootstrap,
    RestDelta,
    WsMarket,
    WsUser,
    RestGeoblock,
    RestClob,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketType {
    Moneyline,
    Spread,
    Total,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    FullGame,
    FirstHalf,
    SecondHalf,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    RestBootstrap,
    RestDelta,
    LiveWs,
    Mock,
    PaperOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Ok,
    Degraded,
    Stale,
    RateLimited,
    Blocked,
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualityFlags {
    pub off_board: bool,
    pub delayed_source: bool,
    pub stale: bool,
    pub unknown_schema: bool,
    pub missing_required_field: bool,
}

impl QualityFlags {
    pub fn off_board() -> Self {
        Self {
            off_board: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl ErrorInfo {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RawMessage {
    pub raw_id: String,
    pub provider: Provider,
    pub source_channel: SourceChannel,
    pub provider_message_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub provider_market_id: Option<String>,
    pub received_at: DateTime<Utc>,
    pub received_mono_ns: u64,
    pub payload_hash: String,
    pub raw_ref: String,
    pub schema_version: String,
    pub trace_id: Uuid,
    pub payload: Value,
}

impl RawMessage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Provider,
        source_channel: SourceChannel,
        provider_message_id: Option<String>,
        provider_event_id: Option<String>,
        provider_market_id: Option<String>,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
        raw_ref: String,
        schema_version: String,
        payload: Value,
    ) -> Self {
        let payload_hash = sha256_json_hash(&payload);
        let raw_id = [
            provider_slug(&provider),
            channel_slug(&source_channel),
            provider_event_id.as_deref().unwrap_or("unknown_event"),
            provider_message_id.as_deref().unwrap_or("unknown_message"),
            payload_hash.trim_start_matches("sha256:"),
        ]
        .join(":");

        Self {
            raw_id,
            provider,
            source_channel,
            provider_message_id,
            provider_event_id,
            provider_market_id,
            received_at,
            received_mono_ns,
            payload_hash,
            raw_ref,
            schema_version,
            trace_id: Uuid::new_v4(),
            payload,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NormalizedQuote {
    pub quote_id: String,
    pub provider: Provider,
    pub canonical_market_key: Option<String>,
    pub canonical_event_id: Option<String>,
    pub provider_event_id: Option<String>,
    pub provider_market_id: Option<String>,
    pub provider_participant_id: Option<String>,
    pub normalized_participant_id: Option<String>,
    pub sport: Option<String>,
    pub market_type: MarketType,
    pub period: Period,
    pub side: Option<String>,
    pub line: Option<String>,
    pub raw_price: Option<String>,
    pub normalized_probability: Option<String>,
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub size: Option<String>,
    pub provider_ts: Option<DateTime<Utc>>,
    pub ingest_ts: DateTime<Utc>,
    pub ingest_mono_ns: u64,
    pub raw_ref: String,
    pub quality_flags: QualityFlags,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceState {
    pub source: String,
    pub mode: SourceMode,
    pub tier: Option<String>,
    pub data_delay_seconds: Option<u64>,
    pub websocket_access: Option<bool>,
    pub status: SourceStatus,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub stale_after_seconds: u64,
    pub rate_limited: bool,
    pub geoblocked: bool,
    pub error: Option<ErrorInfo>,
    pub live_signal_allowed: bool,
    pub live_execution_allowed: bool,
    pub block_reason: Option<String>,
}

fn sha256_json_hash(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).expect("serde_json::Value serialization cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", to_hex(&hasher.finalize()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn provider_slug(provider: &Provider) -> &'static str {
    match provider {
        Provider::TheRundown => "therundown",
        Provider::Polymarket => "polymarket",
    }
}

fn channel_slug(channel: &SourceChannel) -> &'static str {
    match channel {
        SourceChannel::RestBootstrap => "rest_bootstrap",
        SourceChannel::RestDelta => "rest_delta",
        SourceChannel::WsMarket => "ws_market",
        SourceChannel::WsUser => "ws_user",
        SourceChannel::RestGeoblock => "rest_geoblock",
        SourceChannel::RestClob => "rest_clob",
    }
}
