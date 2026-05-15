use crate::therundown::headers::EntitlementHeaders;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use quantsys_domain::{ErrorInfo, SourceMode, SourceState, SourceStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheRundownStateMachine {
    stale_after_seconds: u64,
}

impl TheRundownStateMachine {
    pub fn new(stale_after_seconds: u64) -> Self {
        Self {
            stale_after_seconds,
        }
    }

    pub fn initial(&self, mode: SourceMode) -> SourceState {
        self.base(mode, SourceStatus::Unknown, false, None, None)
    }

    pub fn from_headers(&self, mode: SourceMode, headers: &EntitlementHeaders) -> SourceState {
        let mut state = self.base(mode, SourceStatus::Ok, true, None, None);
        state.tier = headers.tier.clone();
        state.data_delay_seconds = headers.data_delay_seconds;
        state.websocket_access = headers.websocket_access;

        if headers.data_delay_seconds.unwrap_or(u64::MAX) > 0 {
            state.status = SourceStatus::DataDelayDetected;
            state.live_signal_allowed = false;
            state.block_reason = Some("delayed_source".to_string());
            state.error = Some(ErrorInfo::new(
                "DATA_DELAY_DETECTED",
                "TheRundown data is delayed or delay header is unknown",
            ));
        } else if headers.websocket_access != Some(true) {
            state.status = SourceStatus::NoWebsocketAccess;
            state.live_signal_allowed = false;
            state.block_reason = Some("no_websocket_access".to_string());
            state.error = Some(ErrorInfo::new(
                "NO_WEBSOCKET_ACCESS",
                "TheRundown websocket access is false or unknown",
            ));
        } else if headers.datapoints_exhausted() {
            state.status = SourceStatus::DatapointsExhausted;
            state.live_signal_allowed = false;
            state.rate_limited = true;
            state.block_reason = Some("datapoints_exhausted".to_string());
            state.error = Some(ErrorInfo::new(
                "DATAPOINTS_EXHAUSTED",
                "TheRundown datapoints remaining is zero",
            ));
        }

        state
    }

    pub fn mark_auth_failed(&self, mode: SourceMode) -> SourceState {
        self.base(
            mode,
            SourceStatus::AuthFailed,
            false,
            Some("auth_failed"),
            Some(ErrorInfo::new(
                "AUTH_FAILED",
                "TheRundown authentication failed",
            )),
        )
    }

    pub fn mark_rate_limited(&self, mode: SourceMode) -> SourceState {
        let mut state = self.base(
            mode,
            SourceStatus::RateLimited,
            false,
            Some("rate_limited"),
            Some(ErrorInfo::new(
                "RATE_LIMITED",
                "TheRundown endpoint is rate limited",
            )),
        );
        state.rate_limited = true;
        state
    }

    pub fn mark_degraded(&self, mode: SourceMode, code: &str, message: &str) -> SourceState {
        self.base(
            mode,
            SourceStatus::Degraded,
            false,
            Some("source_degraded"),
            Some(ErrorInfo::new(code, message)),
        )
    }

    pub fn mark_stale(&self, mode: SourceMode) -> SourceState {
        self.base(
            mode,
            SourceStatus::Stale,
            false,
            Some("stale_source"),
            Some(ErrorInfo::new("SOURCE_STALE", "TheRundown source is stale")),
        )
    }

    pub fn mark_cursor_stale(&self, mode: SourceMode) -> SourceState {
        self.base(
            mode,
            SourceStatus::CursorStale,
            false,
            Some("cursor_stale"),
            Some(ErrorInfo::new(
                "CURSOR_STALE",
                "TheRundown delta cursor is stale",
            )),
        )
    }

    pub fn mark_schema_error(&self, mode: SourceMode, reason: &str) -> SourceState {
        self.base(
            mode,
            SourceStatus::SchemaError,
            false,
            Some(reason),
            Some(ErrorInfo::new(
                "SCHEMA_ERROR",
                "TheRundown payload did not match the expected schema",
            )),
        )
    }

    pub fn mark_ok_message(
        &self,
        mode: SourceMode,
        received_at: DateTime<Utc>,
        _heartbeat: bool,
    ) -> SourceState {
        let mut state = self.base(mode, SourceStatus::Ok, true, None, None);
        state.last_message_at = Some(received_at);
        state.last_heartbeat_at = Some(received_at);
        state
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

    fn base(
        &self,
        mode: SourceMode,
        status: SourceStatus,
        live_signal_allowed: bool,
        block_reason: Option<&str>,
        error: Option<ErrorInfo>,
    ) -> SourceState {
        SourceState {
            source: "therundown".to_string(),
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
            live_execution_allowed: false,
            block_reason: block_reason.map(str::to_string),
        }
    }
}
