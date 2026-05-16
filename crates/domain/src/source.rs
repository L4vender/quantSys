use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    TheRundown,
    Polymarket,
}

impl Provider {
    pub fn slug(&self) -> &'static str {
        match self {
            Self::TheRundown => "therundown",
            Self::Polymarket => "polymarket",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChannel {
    RestBootstrap,
    RestDelta,
    RestDiscovery,
    WsMarket,
    WsUser,
    RestGeoblock,
    RestTime,
    RestClob,
}

impl SourceChannel {
    pub fn slug(&self) -> &'static str {
        match self {
            Self::RestBootstrap => "rest_bootstrap",
            Self::RestDelta => "rest_delta",
            Self::RestDiscovery => "rest_discovery",
            Self::WsMarket => "ws_market",
            Self::WsUser => "ws_user",
            Self::RestGeoblock => "rest_geoblock",
            Self::RestTime => "rest_time",
            Self::RestClob => "rest_clob",
        }
    }
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
    RestDiscovery,
    RestGeoblock,
    RestTime,
    LiveWs,
    Mock,
    PaperOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Ok,
    Degraded,
    Disabled,
    Stale,
    Delayed,
    NoWs,
    Geoblocked,
    RateLimited,
    AuthMissing,
    AuthFailed,
    DataDelayDetected,
    NoWebsocketAccess,
    DatapointsExhausted,
    CursorStale,
    SchemaError,
    DlqSpike,
    Lagging,
    Blocked,
    MarketResolved,
    Unknown,
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
