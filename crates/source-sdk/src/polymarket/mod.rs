mod discovery;
mod error;
mod geoblock;
mod market_ws;
mod parser;
mod publisher;
mod state;
mod subscription;
mod time_probe;
mod token_cache;

use chrono::{DateTime, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel, SourceMode, SourceState};
use quantsys_eventbus::InMemoryEventProducer;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use discovery::{
    build_discovery_url, interpret_response, DiscoveryFilters, DiscoveryResult, MockHttpResponse,
    PolymarketRestTransport, ReqwestPolymarketRestTransport,
};
pub use error::{
    redact_secret_json, redact_secret_json_in_place, scrub_secret_text, L2Credentials,
    PolymarketError,
};
pub use geoblock::GeoblockStatus;
pub use market_ws::PolymarketBackoff;
pub use parser::{
    payload_hash, ParsedPolymarketKind, ParsedPolymarketPayload, ParserError, PolymarketParser,
};
pub use publisher::{
    DlqRecord, DlqSink, InMemoryDlqSink, RawPublisher, DLQ_RAW_TOPIC, RAW_POLYMARKET_MARKET_TOPIC,
    RAW_POLYMARKET_USER_TOPIC,
};
pub use state::{EndpointBudget, PolymarketStateMachine};
pub use subscription::{
    build_market_subscription_payload, build_user_subscription_payload,
    validate_market_subscription_payload,
};
pub use time_probe::TimeProbe;
pub use token_cache::{DiscoveredMarket, TokenCache};

#[derive(Clone, Debug)]
pub struct PolymarketMarketAdapterConfig {
    pub gamma_api_base_url: String,
    pub geoblock_url: String,
    pub server_time_url: String,
    pub schema_version: String,
    pub discovery_limit: u32,
    pub discovery_offset: u32,
    pub discovery_game_tag_id: Option<u64>,
    pub discovery_filters: DiscoveryFilters,
    pub stale_after_seconds: u64,
    pub rest_timeout: Duration,
    pub token_cache_ttl_seconds: u64,
    pub max_token_subscriptions: usize,
    pub reconnect_backoff: PolymarketBackoff,
}

#[derive(Clone, Debug)]
pub struct PolymarketMarketAdapter<T>
where
    T: PolymarketRestTransport,
{
    config: PolymarketMarketAdapterConfig,
    transport: T,
    parser: PolymarketParser,
    state_machine: PolymarketStateMachine,
    token_cache: TokenCache,
    state: SourceState,
    publisher: RawPublisher<InMemoryEventProducer>,
    dlq: InMemoryDlqSink,
    budgets: BTreeMap<String, EndpointBudget>,
    reconnect_attempt: u32,
}

impl<T> PolymarketMarketAdapter<T>
where
    T: PolymarketRestTransport,
{
    pub fn new(
        config: PolymarketMarketAdapterConfig,
        transport: T,
        producer: InMemoryEventProducer,
        dlq: InMemoryDlqSink,
    ) -> Self {
        let parser = PolymarketParser::new(config.schema_version.clone());
        let state_machine = PolymarketStateMachine::new(config.stale_after_seconds);
        let state = state_machine.initial_market();
        let token_cache = TokenCache::new(config.token_cache_ttl_seconds);
        Self {
            config,
            transport,
            parser,
            state_machine,
            token_cache,
            state,
            publisher: RawPublisher::new(producer),
            dlq,
            budgets: BTreeMap::new(),
            reconnect_attempt: 0,
        }
    }

    pub fn publisher(&self) -> &InMemoryEventProducer {
        self.publisher.inner()
    }

    pub fn dlq(&self) -> &InMemoryDlqSink {
        &self.dlq
    }

    pub fn state(&self) -> &SourceState {
        &self.state
    }

    pub fn token_cache(&self) -> &TokenCache {
        &self.token_cache
    }

    pub fn endpoint_budget(&self, endpoint: &str) -> Option<&EndpointBudget> {
        self.budgets.get(endpoint)
    }

    pub async fn discover_markets(&mut self) -> Result<DiscoveryResult, PolymarketError> {
        let url = build_discovery_url(
            &self.config.gamma_api_base_url,
            self.config.discovery_limit,
            self.config.discovery_offset,
            self.config.discovery_game_tag_id,
        )?;
        let response = self
            .transport
            .get_json(&url, self.config.rest_timeout)
            .await
            .and_then(interpret_response);
        match response {
            Ok(response) => {
                let received_at = Utc::now();
                let body = response.body;
                let result = match self.parser.parse_discovery_payload(
                    body.clone(),
                    &self.config.discovery_filters,
                    received_at,
                    mono_ns(),
                ) {
                    Ok(result) => result,
                    Err(err) => {
                        self.publish_dlq(
                            "SCHEMA_ERROR",
                            &err.to_string(),
                            "rest_discovery",
                            &body,
                            received_at,
                        )
                        .await?;
                        self.state = self
                            .state_machine
                            .market_schema_error("missing_required_field");
                        return Err(PolymarketError::Schema(err.to_string()));
                    }
                };
                self.token_cache
                    .upsert_markets(result.markets.clone(), received_at);
                self.state = self.state_machine.market_ok(received_at);
                self.publisher.publish_raw(&result.raw).await?;
                self.reconnect_attempt = 0;
                Ok(result)
            }
            Err(err) => {
                self.apply_rest_error(&err, SourceMode::RestDiscovery);
                Err(err)
            }
        }
    }

    pub fn market_subscription_payload(
        &self,
        custom_feature_enabled: bool,
    ) -> Result<Value, PolymarketError> {
        let mut token_ids = self.token_cache.all_token_ids();
        if token_ids.len() > self.config.max_token_subscriptions {
            token_ids.truncate(self.config.max_token_subscriptions);
        }
        build_market_subscription_payload(&token_ids, custom_feature_enabled)
    }

    pub async fn handle_market_ws_json(
        &mut self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<RawMessage, PolymarketError> {
        match self
            .parser
            .parse_market_ws_payload(payload.clone(), received_at, received_mono_ns)
        {
            Ok(parsed) => {
                match &parsed.kind {
                    ParsedPolymarketKind::Unknown { .. } => {
                        self.state = self.state_machine.market_schema_error("unknown_schema");
                        self.state.last_message_at = Some(received_at);
                    }
                    ParsedPolymarketKind::MarketResolved => {
                        let condition_id = parsed
                            .raw
                            .provider_event_id
                            .as_deref()
                            .unwrap_or("unknown_market");
                        self.state = self.state_machine.market_resolved(condition_id);
                        self.state.last_message_at = Some(received_at);
                    }
                    ParsedPolymarketKind::MarketBook
                    | ParsedPolymarketKind::MarketPriceChange
                    | ParsedPolymarketKind::MarketBestBidAsk
                    | ParsedPolymarketKind::MarketLastTradePrice
                    | ParsedPolymarketKind::MarketTickSizeChange
                    | ParsedPolymarketKind::NewMarket => {
                        self.state = self.state_machine.market_ok(received_at);
                    }
                    ParsedPolymarketKind::UserOrder
                    | ParsedPolymarketKind::UserFill
                    | ParsedPolymarketKind::UserOrderUpdate => {}
                }
                self.publisher.publish_raw(&parsed.raw).await?;
                Ok(parsed.raw)
            }
            Err(err) => {
                self.publish_dlq(
                    "SCHEMA_ERROR",
                    &err.to_string(),
                    "ws_market",
                    &payload,
                    received_at,
                )
                .await?;
                self.state = self
                    .state_machine
                    .market_schema_error("missing_required_field");
                Err(PolymarketError::Schema(err.to_string()))
            }
        }
    }

    pub async fn probe_geoblock(&mut self) -> Result<GeoblockStatus, PolymarketError> {
        let response = self
            .transport
            .get_json(&self.config.geoblock_url, self.config.rest_timeout)
            .await
            .and_then(interpret_response);
        match response {
            Ok(response) => {
                let received_at = Utc::now();
                let status = GeoblockStatus::parse(response.body)
                    .map_err(|err| PolymarketError::Schema(err.to_string()))?;
                self.state = self
                    .state_machine
                    .geoblock_state(&status, SourceMode::RestGeoblock);
                let raw = self.rest_raw(
                    SourceChannel::RestGeoblock,
                    Some("geoblock".to_string()),
                    None,
                    status.sanitized_payload(),
                    received_at,
                );
                self.publisher.publish_raw(&raw).await?;
                Ok(status)
            }
            Err(err) => {
                self.state = self.state_machine.geoblock_unknown();
                Err(err)
            }
        }
    }

    pub async fn probe_time_at(
        &mut self,
        local_time: DateTime<Utc>,
    ) -> Result<TimeProbe, PolymarketError> {
        let response = self
            .transport
            .get_json(&self.config.server_time_url, self.config.rest_timeout)
            .await
            .and_then(interpret_response);
        match response {
            Ok(response) => {
                let received_at = Utc::now();
                let probe = TimeProbe::parse_json(response.body, local_time)
                    .map_err(|err| PolymarketError::Schema(err.to_string()))?;
                self.state = self.state_machine.time_ok(probe.large_offset_warning);
                let raw = self.rest_raw(
                    SourceChannel::RestTime,
                    Some("time".to_string()),
                    None,
                    probe.payload(),
                    received_at,
                );
                self.publisher.publish_raw(&raw).await?;
                Ok(probe)
            }
            Err(err) => {
                self.state = self.state_machine.time_ok(true);
                Err(err)
            }
        }
    }

    pub fn mark_pong(&mut self, at: DateTime<Utc>) {
        self.state = self.state_machine.market_ok(at);
    }

    pub fn detect_stale(&mut self, now: DateTime<Utc>) -> bool {
        let last_seen = self.state.last_message_at.or(self.state.last_heartbeat_at);
        if self.state_machine.is_stale(last_seen, now) {
            self.state = self.state_machine.market_stale();
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
            Some(
                self.config
                    .reconnect_backoff
                    .delay(self.reconnect_attempt - 1),
            )
        }
    }

    pub fn mark_endpoint_rate_limited(&mut self, endpoint: &str, retry_after: Option<Duration>) {
        self.budgets
            .entry(endpoint.to_string())
            .or_insert_with(|| EndpointBudget::new(endpoint))
            .mark_rate_limited(retry_after);
        self.state = self.state_machine.market_rate_limited();
    }

    async fn publish_dlq(
        &self,
        error_code: &str,
        error_message: &str,
        source_channel: &str,
        payload: &Value,
        received_at: DateTime<Utc>,
    ) -> Result<(), PolymarketError> {
        let hash = payload_hash(payload);
        self.dlq
            .publish_dlq(DlqRecord {
                error_code: error_code.to_string(),
                error_message: error_message.to_string(),
                provider: "polymarket".to_string(),
                source_channel: source_channel.to_string(),
                payload_hash: hash.clone(),
                raw_ref: format!("dlq/polymarket/{hash}.json"),
                received_at,
                schema_version: self.config.schema_version.clone(),
                trace_id: format!("dlq:{hash}"),
            })
            .await
    }

    fn rest_raw(
        &self,
        source_channel: SourceChannel,
        provider_message_id: Option<String>,
        provider_event_id: Option<String>,
        payload: Value,
        received_at: DateTime<Utc>,
    ) -> RawMessage {
        let raw_ref = crate::polymarket::discovery::raw_ref(
            &source_channel,
            provider_event_id.as_deref(),
            provider_message_id.as_deref(),
            &payload_hash(&payload),
        );
        RawMessage::new(
            Provider::Polymarket,
            source_channel,
            provider_message_id,
            provider_event_id,
            None,
            received_at,
            mono_ns(),
            raw_ref,
            self.config.schema_version.clone(),
            payload,
        )
    }

    fn apply_rest_error(&mut self, err: &PolymarketError, mode: SourceMode) {
        match err {
            PolymarketError::RateLimited { retry_after } => {
                let endpoint = match mode {
                    SourceMode::RestDiscovery => "discovery",
                    SourceMode::RestGeoblock => "geoblock",
                    SourceMode::RestTime => "time",
                    _ => "unknown",
                };
                self.mark_endpoint_rate_limited(endpoint, *retry_after);
            }
            PolymarketError::AuthFailed => {
                self.state = self.state_machine.user_auth_failed();
            }
            _ => {
                self.state = self.state_machine.market_schema_error("source_degraded");
                self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct PolymarketUserAdapterConfig {
    pub schema_version: String,
    pub stale_after_seconds: u64,
    pub reconnect_backoff: PolymarketBackoff,
}

#[derive(Clone, Debug)]
pub struct PolymarketUserAdapter {
    config: PolymarketUserAdapterConfig,
    parser: PolymarketParser,
    state_machine: PolymarketStateMachine,
    state: SourceState,
    publisher: RawPublisher<InMemoryEventProducer>,
    dlq: InMemoryDlqSink,
    reconnect_attempt: u32,
}

impl PolymarketUserAdapter {
    pub fn new(
        config: PolymarketUserAdapterConfig,
        producer: InMemoryEventProducer,
        dlq: InMemoryDlqSink,
    ) -> Self {
        let parser = PolymarketParser::new(config.schema_version.clone());
        let state_machine = PolymarketStateMachine::new(config.stale_after_seconds);
        let state = state_machine.user_disabled();
        Self {
            config,
            parser,
            state_machine,
            state,
            publisher: RawPublisher::new(producer),
            dlq,
            reconnect_attempt: 0,
        }
    }

    pub fn publisher(&self) -> &InMemoryEventProducer {
        self.publisher.inner()
    }

    pub fn dlq(&self) -> &InMemoryDlqSink {
        &self.dlq
    }

    pub fn state(&self) -> &SourceState {
        &self.state
    }

    pub async fn handle_user_ws_json(
        &mut self,
        payload: Value,
        received_at: DateTime<Utc>,
        received_mono_ns: u64,
    ) -> Result<RawMessage, PolymarketError> {
        match self
            .parser
            .parse_user_ws_payload(payload.clone(), received_at, received_mono_ns)
        {
            Ok(parsed) => {
                if matches!(parsed.kind, ParsedPolymarketKind::Unknown { .. }) {
                    self.state = self.state_machine.user_stale();
                } else {
                    self.state = self.state_machine.user_ok(received_at);
                }
                self.publisher.publish_raw(&parsed.raw).await?;
                Ok(parsed.raw)
            }
            Err(err) => {
                self.publish_dlq(&err.to_string(), &payload, received_at)
                    .await?;
                self.state = self.state_machine.user_stale();
                Err(PolymarketError::Schema(err.to_string()))
            }
        }
    }

    pub fn mark_auth_missing(&mut self) -> SourceState {
        self.state = self.state_machine.user_auth_missing();
        self.state.clone()
    }

    pub fn mark_pong(&mut self, at: DateTime<Utc>) {
        self.state = self.state_machine.user_ok(at);
    }

    pub fn detect_stale(&mut self, now: DateTime<Utc>) -> bool {
        let last_seen = self.state.last_message_at.or(self.state.last_heartbeat_at);
        if self.state_machine.is_stale(last_seen, now) {
            self.state = self.state_machine.user_stale();
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
            Some(
                self.config
                    .reconnect_backoff
                    .delay(self.reconnect_attempt - 1),
            )
        }
    }

    async fn publish_dlq(
        &self,
        error_message: &str,
        payload: &Value,
        received_at: DateTime<Utc>,
    ) -> Result<(), PolymarketError> {
        let sanitized = redact_secret_json(payload);
        let hash = payload_hash(&sanitized);
        self.dlq
            .publish_dlq(DlqRecord {
                error_code: "SCHEMA_ERROR".to_string(),
                error_message: error_message.to_string(),
                provider: "polymarket".to_string(),
                source_channel: "ws_user".to_string(),
                payload_hash: hash.clone(),
                raw_ref: format!("dlq/polymarket/{hash}.json"),
                received_at,
                schema_version: self.config.schema_version.clone(),
                trace_id: format!("dlq:{hash}"),
            })
            .await
    }
}

fn mono_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}
