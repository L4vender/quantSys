mod cursor;
mod error;
mod headers;
mod parser;
mod publisher;
mod rest;
mod state;
mod subscription;
mod ws;

use chrono::{DateTime, Utc};
use quantsys_domain::{RawMessage, SourceMode, SourceState};
use quantsys_eventbus::InMemoryEventProducer;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use cursor::{BootstrapCursorUpdate, DeltaCursor};
pub use error::{scrub_secret_text, ApiKey, TheRundownError};
pub use headers::{parse_retry_after, EntitlementHeaders, RetryAfterParseError};
pub use parser::{payload_hash, ParsedPayload, ParsedPayloadKind, ParserError, TheRundownParser};
pub use publisher::{
    DlqRecord, DlqSink, InMemoryDlqSink, RawPublisher, DLQ_EXTERNAL_TOPIC, RAW_THERUNDOWN_TOPIC,
};
pub use rest::{
    build_events_bootstrap_url, build_markets_delta_url, build_probe_url, interpret_response,
    MockRestResponse, ReqwestRestTransport, RestTransport, TheRundownRestClient,
};
pub use state::TheRundownStateMachine;
pub use subscription::{build_ws_url, redact_ws_url, SubscriptionFilters};
pub use ws::TheRundownBackoff;

#[derive(Clone, Debug)]
pub struct TheRundownAdapterConfig {
    pub api_base_url: String,
    pub schema_version: String,
    pub stale_after_seconds: u64,
    pub rest_timeout: Duration,
    pub reconnect_backoff: TheRundownBackoff,
}

#[derive(Clone, Debug)]
pub struct TheRundownAdapter<T>
where
    T: RestTransport,
{
    rest: TheRundownRestClient<T>,
    parser: TheRundownParser,
    state_machine: TheRundownStateMachine,
    cursor: DeltaCursor,
    state: SourceState,
    publisher: RawPublisher<InMemoryEventProducer>,
    dlq: InMemoryDlqSink,
    reconnect_backoff: TheRundownBackoff,
    retry_after: Option<Duration>,
    reconnect_attempt: u32,
    off_board_count: u64,
}

impl<T> TheRundownAdapter<T>
where
    T: RestTransport,
{
    pub fn new(
        config: TheRundownAdapterConfig,
        api_key: ApiKey,
        transport: T,
        producer: InMemoryEventProducer,
        dlq: InMemoryDlqSink,
    ) -> Self {
        let state_machine = TheRundownStateMachine::new(config.stale_after_seconds);
        let state = state_machine.initial(SourceMode::Mock);
        Self {
            rest: TheRundownRestClient::new(
                config.api_base_url,
                api_key,
                transport,
                config.rest_timeout,
            ),
            parser: TheRundownParser::new(config.schema_version),
            state_machine,
            cursor: DeltaCursor::new(30),
            state,
            publisher: RawPublisher::new(producer),
            dlq,
            reconnect_backoff: config.reconnect_backoff,
            retry_after: None,
            reconnect_attempt: 0,
            off_board_count: 0,
        }
    }

    pub fn publisher(&self) -> &InMemoryEventProducer {
        self.publisher.inner()
    }

    pub fn dlq(&self) -> &InMemoryDlqSink {
        &self.dlq
    }

    pub fn cursor(&self) -> &DeltaCursor {
        &self.cursor
    }

    pub fn state(&self) -> &SourceState {
        &self.state
    }

    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub fn off_board_count(&self) -> u64 {
        self.off_board_count
    }

    pub fn mark_cursor(&mut self, last_id: &str) {
        self.cursor.set_last_id(last_id.to_string(), Utc::now());
    }

    pub fn mark_message_at(&mut self, at: DateTime<Utc>) {
        self.state = self
            .state_machine
            .mark_ok_message(SourceMode::LiveWs, at, false);
    }

    pub fn detect_stale(&mut self, now: DateTime<Utc>) -> bool {
        let last_seen = self.state.last_message_at.or(self.state.last_heartbeat_at);
        if self.state_machine.is_stale(last_seen, now) {
            self.state = self.state_machine.mark_stale(SourceMode::LiveWs);
            self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn next_reconnect_delay(&self) -> Option<Duration> {
        if self.reconnect_attempt == 0 {
            None
        } else {
            Some(self.reconnect_backoff().delay(self.reconnect_attempt - 1))
        }
    }

    pub async fn probe(&mut self) -> Result<EntitlementHeaders, TheRundownError> {
        match self.rest.probe().await {
            Ok(response) => {
                self.state = self
                    .state_machine
                    .from_headers(SourceMode::RestBootstrap, &response.headers);
                self.reconnect_attempt = 0;
                Ok(response.headers)
            }
            Err(err) => {
                self.apply_rest_error(&err, SourceMode::RestBootstrap);
                Err(err)
            }
        }
    }

    pub async fn bootstrap_events(
        &mut self,
        sport_id: u32,
        date: &str,
    ) -> Result<RawMessage, TheRundownError> {
        match self.rest.events_bootstrap(sport_id, date).await {
            Ok(response) => {
                let received_at = Utc::now();
                let raw = self
                    .parser
                    .parse_rest_bootstrap(response.body.clone(), received_at, mono_ns())?
                    .raw;
                self.cursor
                    .update_from_bootstrap(&response.body, received_at)?;
                self.state = self
                    .state_machine
                    .from_headers(SourceMode::RestBootstrap, &response.headers);
                self.state.last_message_at = Some(received_at);
                self.reconnect_attempt = 0;
                self.publisher.publish_raw(&raw).await?;
                Ok(raw)
            }
            Err(err) => {
                self.apply_rest_error(&err, SourceMode::RestBootstrap);
                Err(err)
            }
        }
    }

    pub async fn markets_delta(&mut self, last_id: &str) -> Result<RawMessage, TheRundownError> {
        match self.rest.markets_delta(last_id).await {
            Ok(response) => {
                let received_at = Utc::now();
                let raw = self
                    .parser
                    .parse_rest_delta(response.body.clone(), received_at, mono_ns())?
                    .raw;
                self.cursor.update_from_delta(&response.body, received_at)?;
                self.state = self
                    .state_machine
                    .from_headers(SourceMode::RestDelta, &response.headers);
                self.state.last_message_at = Some(received_at);
                self.reconnect_attempt = 0;
                self.publisher.publish_raw(&raw).await?;
                Ok(raw)
            }
            Err(err) => {
                self.apply_rest_error(&err, SourceMode::RestDelta);
                Err(err)
            }
        }
    }

    pub async fn markets_delta_with_bootstrap_recovery(
        &mut self,
        last_id: &str,
        sport_id: u32,
        date: &str,
    ) -> Result<RawMessage, TheRundownError> {
        match self.markets_delta(last_id).await {
            Ok(raw) => Ok(raw),
            Err(err) if DeltaCursor::should_rebootstrap(&err) => {
                self.bootstrap_events(sport_id, date).await
            }
            Err(err) => Err(err),
        }
    }

    pub async fn handle_ws_json(
        &mut self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<RawMessage, TheRundownError> {
        match self
            .parser
            .parse_ws_payload(payload.clone(), received_at, received_mono_ns)
        {
            Ok(parsed) => {
                if parsed.quality_flags.off_board {
                    self.off_board_count = self.off_board_count.saturating_add(1);
                }
                match &parsed.kind {
                    ParsedPayloadKind::Heartbeat => {
                        self.state = self.state_machine.mark_ok_message(
                            SourceMode::LiveWs,
                            received_at,
                            true,
                        );
                    }
                    ParsedPayloadKind::MarketPrice => {
                        self.state = self.state_machine.mark_ok_message(
                            SourceMode::LiveWs,
                            received_at,
                            false,
                        );
                    }
                    ParsedPayloadKind::Unknown { .. } => {
                        self.state = self
                            .state_machine
                            .mark_schema_error(SourceMode::LiveWs, "unknown_schema");
                        self.state.last_message_at = Some(received_at);
                    }
                    ParsedPayloadKind::RestBootstrap { .. }
                    | ParsedPayloadKind::RestDelta { .. } => {}
                }
                self.publisher.publish_raw(&parsed.raw).await?;
                Ok(parsed.raw)
            }
            Err(err) => {
                let hash = payload_hash(&payload);
                let record = DlqRecord {
                    error_code: "SCHEMA_ERROR".to_string(),
                    error_message: err.to_string(),
                    provider: "therundown".to_string(),
                    source_channel: "ws_market".to_string(),
                    payload_hash: hash.clone(),
                    raw_ref: format!("dlq/therundown/{hash}.json"),
                    received_at,
                    schema_version: "therundown.v2.schema_error".to_string(),
                    trace_id: format!("dlq:{hash}"),
                };
                self.dlq.publish_dlq(record).await?;
                self.state = self
                    .state_machine
                    .mark_schema_error(SourceMode::LiveWs, "missing_required_field");
                Err(TheRundownError::Schema(err.to_string()))
            }
        }
    }

    fn apply_rest_error(&mut self, err: &TheRundownError, mode: SourceMode) {
        match err {
            TheRundownError::AuthFailed => {
                self.state = self.state_machine.mark_auth_failed(mode);
                self.retry_after = None;
            }
            TheRundownError::RateLimited { retry_after } => {
                self.state = self.state_machine.mark_rate_limited(mode);
                self.retry_after = *retry_after;
            }
            TheRundownError::CursorStale => {
                self.state = self.state_machine.mark_cursor_stale(mode);
                self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            }
            TheRundownError::Server { .. } | TheRundownError::Transport(_) => {
                self.state = self.state_machine.mark_degraded(
                    mode,
                    "SOURCE_DEGRADED",
                    "TheRundown request failed",
                );
                self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            }
            TheRundownError::MissingApiKey { .. }
            | TheRundownError::MalformedJson(_)
            | TheRundownError::Schema(_)
            | TheRundownError::Config(_)
            | TheRundownError::Websocket(_) => {
                self.state = self.state_machine.mark_degraded(
                    mode,
                    "SOURCE_DEGRADED",
                    "TheRundown request failed",
                );
            }
        }
    }

    fn reconnect_backoff(&self) -> &TheRundownBackoff {
        &self.reconnect_backoff
    }
}

impl<T> TheRundownAdapter<T>
where
    T: RestTransport,
{
    pub fn with_backoff(
        config: TheRundownAdapterConfig,
        api_key: ApiKey,
        transport: T,
        producer: InMemoryEventProducer,
        dlq: InMemoryDlqSink,
    ) -> Self {
        Self::new(config, api_key, transport, producer, dlq)
    }
}

fn mono_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}
