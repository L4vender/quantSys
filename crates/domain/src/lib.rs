use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod dlq;
pub mod raw;
pub mod source;

mod ws_watchlist;

pub use dlq::{sanitize_error_message, DlqErrorCategory, DlqErrorCode, DlqEvent};
pub use raw::{
    compute_payload_hash, compute_raw_id, scan_json_for_secrets, QualityFlags, RawArchiveStatus,
    RawMessage, SecretScanError,
};
pub use source::{
    ErrorInfo, MarketType, Period, Provider, SourceChannel, SourceMode, SourceState, SourceStatus,
};
pub use ws_watchlist::{WatchlistPolymarket, WatchlistTheRundown, WsWatchlist, WsWatchlistItem};

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
