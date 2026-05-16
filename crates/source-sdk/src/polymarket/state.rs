use crate::polymarket::geoblock::GeoblockStatus;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use quantsys_domain::{ErrorInfo, SourceMode, SourceState, SourceStatus};
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointBudget {
    pub endpoint: String,
    pub retry_after: Option<Duration>,
    pub rate_limited: bool,
}

impl EndpointBudget {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            retry_after: None,
            rate_limited: false,
        }
    }

    pub fn mark_rate_limited(&mut self, retry_after: Option<Duration>) {
        self.rate_limited = true;
        self.retry_after = retry_after;
    }

    pub fn is_limited(&self) -> bool {
        self.rate_limited
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolymarketStateMachine {
    stale_after_seconds: u64,
}

impl PolymarketStateMachine {
    pub fn new(stale_after_seconds: u64) -> Self {
        Self {
            stale_after_seconds,
        }
    }

    pub fn initial_market(&self) -> SourceState {
        self.base(
            "polymarket_market",
            SourceMode::Mock,
            SourceStatus::Unknown,
            false,
            false,
            None,
            None,
        )
    }

    pub fn market_ok(&self, at: DateTime<Utc>) -> SourceState {
        let mut state = self.base(
            "polymarket_market",
            SourceMode::LiveWs,
            SourceStatus::Ok,
            true,
            false,
            None,
            None,
        );
        state.last_message_at = Some(at);
        state.last_heartbeat_at = Some(at);
        state
    }

    pub fn market_stale(&self) -> SourceState {
        self.base(
            "polymarket_market",
            SourceMode::LiveWs,
            SourceStatus::Stale,
            false,
            false,
            Some("stale_source"),
            Some(ErrorInfo::new(
                "SOURCE_STALE",
                "Polymarket market websocket is stale",
            )),
        )
    }

    pub fn market_schema_error(&self, reason: &str) -> SourceState {
        self.base(
            "polymarket_market",
            SourceMode::LiveWs,
            SourceStatus::SchemaError,
            false,
            false,
            Some(reason),
            Some(ErrorInfo::new(
                "SCHEMA_ERROR",
                "Polymarket market payload did not match expected schema",
            )),
        )
    }

    pub fn market_rate_limited(&self) -> SourceState {
        let mut state = self.base(
            "polymarket_market",
            SourceMode::LiveWs,
            SourceStatus::RateLimited,
            false,
            false,
            Some("rate_limited"),
            Some(ErrorInfo::new(
                "RATE_LIMITED",
                "Polymarket endpoint is rate limited",
            )),
        );
        state.rate_limited = true;
        state
    }

    pub fn market_resolved(&self, condition_id: &str) -> SourceState {
        self.base(
            "polymarket_market",
            SourceMode::LiveWs,
            SourceStatus::MarketResolved,
            false,
            false,
            Some("market_resolved"),
            Some(ErrorInfo::new(
                "MARKET_RESOLVED",
                format!("Polymarket market {condition_id} is resolved"),
            )),
        )
    }

    pub fn user_ok(&self, at: DateTime<Utc>) -> SourceState {
        let mut state = self.base(
            "polymarket_user",
            SourceMode::LiveWs,
            SourceStatus::Ok,
            false,
            false,
            None,
            None,
        );
        state.websocket_access = Some(true);
        state.last_message_at = Some(at);
        state.last_heartbeat_at = Some(at);
        state
    }

    pub fn user_disabled(&self) -> SourceState {
        self.base(
            "polymarket_user",
            SourceMode::LiveWs,
            SourceStatus::Disabled,
            false,
            false,
            Some("user_ws_disabled"),
            Some(ErrorInfo::new(
                "USER_WS_DISABLED",
                "Polymarket user websocket is disabled",
            )),
        )
    }

    pub fn user_auth_missing(&self) -> SourceState {
        self.base(
            "polymarket_user",
            SourceMode::LiveWs,
            SourceStatus::AuthMissing,
            false,
            false,
            Some("auth_missing"),
            Some(ErrorInfo::new(
                "AUTH_MISSING",
                "Polymarket user websocket credentials are missing",
            )),
        )
    }

    pub fn user_auth_failed(&self) -> SourceState {
        self.base(
            "polymarket_user",
            SourceMode::LiveWs,
            SourceStatus::AuthFailed,
            false,
            false,
            Some("auth_failed"),
            Some(ErrorInfo::new(
                "AUTH_FAILED",
                "Polymarket user websocket authentication failed",
            )),
        )
    }

    pub fn user_stale(&self) -> SourceState {
        self.base(
            "polymarket_user",
            SourceMode::LiveWs,
            SourceStatus::Stale,
            false,
            false,
            Some("stale_source"),
            Some(ErrorInfo::new(
                "SOURCE_STALE",
                "Polymarket user websocket is stale",
            )),
        )
    }

    pub fn geoblock_state(&self, status: &GeoblockStatus, mode: SourceMode) -> SourceState {
        if status.blocked {
            let mut state = self.base(
                "polymarket_geoblock",
                mode,
                SourceStatus::Blocked,
                false,
                false,
                Some("geoblocked"),
                Some(ErrorInfo::new(
                    "GEOBLOCKED",
                    "Polymarket geoblock probe returned blocked=true",
                )),
            );
            state.geoblocked = true;
            state
        } else {
            self.base(
                "polymarket_geoblock",
                mode,
                SourceStatus::Ok,
                false,
                false,
                None,
                None,
            )
        }
    }

    pub fn geoblock_unknown(&self) -> SourceState {
        self.base(
            "polymarket_geoblock",
            SourceMode::RestGeoblock,
            SourceStatus::Unknown,
            false,
            false,
            Some("geoblock_unknown"),
            Some(ErrorInfo::new(
                "GEOBLOCK_UNKNOWN",
                "Polymarket geoblock status is unknown; live execution fails closed",
            )),
        )
    }

    pub fn time_ok(&self, large_offset_warning: bool) -> SourceState {
        if large_offset_warning {
            self.base(
                "polymarket_time",
                SourceMode::RestTime,
                SourceStatus::Degraded,
                false,
                false,
                Some("large_time_offset"),
                Some(ErrorInfo::new(
                    "LARGE_TIME_OFFSET",
                    "Polymarket server time offset exceeds warning threshold",
                )),
            )
        } else {
            self.base(
                "polymarket_time",
                SourceMode::RestTime,
                SourceStatus::Ok,
                false,
                false,
                None,
                None,
            )
        }
    }

    pub fn is_stale(&self, last_seen: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        match last_seen {
            Some(last_seen) => {
                now.signed_duration_since(last_seen)
                    > ChronoDuration::seconds(self.stale_after_seconds as i64)
            }
            None => true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn base(
        &self,
        source: &str,
        mode: SourceMode,
        status: SourceStatus,
        live_signal_allowed: bool,
        live_execution_allowed: bool,
        block_reason: Option<&str>,
        error: Option<ErrorInfo>,
    ) -> SourceState {
        SourceState {
            source: source.to_string(),
            mode,
            tier: None,
            data_delay_seconds: None,
            websocket_access: None,
            status,
            last_message_at: None,
            last_heartbeat_at: None,
            stale_after_seconds: self.stale_after_seconds,
            rate_limited: false,
            geoblocked: false,
            error,
            live_signal_allowed,
            live_execution_allowed,
            block_reason: block_reason.map(str::to_string),
        }
    }
}
