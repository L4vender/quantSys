use crate::polymarket::error::PolymarketError;
use crate::polymarket::parser::{payload_hash, value_to_string, ParserError, RawFields};
use crate::polymarket::token_cache::DiscoveredMarket;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryFilters {
    pub sports_only: bool,
    pub allowed_sports: Vec<String>,
    pub allowed_market_types: Vec<String>,
}

impl DiscoveryFilters {
    pub fn sports_default() -> Self {
        Self {
            sports_only: true,
            allowed_sports: vec![
                "nba".to_string(),
                "nfl".to_string(),
                "mlb".to_string(),
                "nhl".to_string(),
                "atp".to_string(),
                "wta".to_string(),
                "tennis".to_string(),
                "soccer".to_string(),
                "sports".to_string(),
            ],
            allowed_market_types: vec![
                "moneyline".to_string(),
                "spread".to_string(),
                "total".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveryResult {
    pub raw: RawMessage,
    pub markets: Vec<DiscoveredMarket>,
    pub filtered_closed: usize,
    pub filtered_non_sports: usize,
    pub filtered_unsupported_market_types: usize,
    pub missing_token_ids: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MockHttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

impl MockHttpResponse {
    pub fn new<I, K, V>(status: u16, headers: I, body: Value) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        Self {
            status,
            headers: headers
                .into_iter()
                .map(|(key, value)| {
                    (
                        key.as_ref().to_ascii_lowercase(),
                        value.as_ref().to_string(),
                    )
                })
                .collect(),
            body,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        self.headers
            .get("retry-after")
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_secs)
    }
}

#[async_trait]
pub trait PolymarketRestTransport: Clone + Send + Sync + 'static {
    async fn get_json(
        &self,
        url: &str,
        timeout: Duration,
    ) -> Result<MockHttpResponse, PolymarketError>;
}

#[derive(Clone, Debug, Default)]
pub struct ReqwestPolymarketRestTransport {
    client: reqwest::Client,
}

impl ReqwestPolymarketRestTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl PolymarketRestTransport for ReqwestPolymarketRestTransport {
    async fn get_json(
        &self,
        url: &str,
        timeout: Duration,
    ) -> Result<MockHttpResponse, PolymarketError> {
        let response = self
            .client
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|err| PolymarketError::Transport(err.to_string()))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(key, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (key.as_str().to_ascii_lowercase(), value.to_string()))
            })
            .collect();
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| PolymarketError::MalformedJson(err.to_string()))?;
        Ok(MockHttpResponse {
            status,
            headers,
            body,
        })
    }
}

pub fn build_discovery_url(
    base_url: &str,
    limit: u32,
    offset: u32,
    game_tag_id: Option<u64>,
) -> Result<String, PolymarketError> {
    let mut url = Url::parse(&format!("{}/events", base_url.trim_end_matches('/')))
        .map_err(|err| PolymarketError::Config(err.to_string()))?;
    let mut pairs = url.query_pairs_mut();
    pairs
        .append_pair("active", "true")
        .append_pair("closed", "false")
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string());
    if let Some(game_tag_id) = game_tag_id {
        pairs.append_pair("tag_id", &game_tag_id.to_string());
    }
    drop(pairs);
    Ok(url.to_string())
}

pub fn interpret_response(response: MockHttpResponse) -> Result<MockHttpResponse, PolymarketError> {
    match response.status {
        200..=299 => Ok(response),
        401 | 403 => Err(PolymarketError::AuthFailed),
        429 => Err(PolymarketError::RateLimited {
            retry_after: response.retry_after(),
        }),
        status if status >= 500 => Err(PolymarketError::Server { status }),
        status => Err(PolymarketError::Transport(format!(
            "Polymarket returned unexpected status {status}"
        ))),
    }
}

pub fn parse_discovery_payload(
    schema_version: &str,
    payload: Value,
    filters: &DiscoveryFilters,
    received_at: DateTime<Utc>,
    received_mono_ns: u64,
) -> Result<DiscoveryResult, ParserError> {
    let events = event_items(&payload).ok_or_else(|| ParserError::InvalidPayload {
        message: "discovery payload must be an array or object with data/events".to_string(),
    })?;
    let mut markets = Vec::new();
    let mut filtered_closed = 0_usize;
    let mut filtered_non_sports = 0_usize;
    let mut filtered_unsupported_market_types = 0_usize;
    let missing_token_ids = 0_usize;

    for event in events {
        let event_active = bool_field(event, "active").unwrap_or(true);
        let event_closed = bool_field(event, "closed").unwrap_or(false);
        if !event_active || event_closed {
            filtered_closed = filtered_closed.saturating_add(1);
            continue;
        }
        if filters.sports_only && !is_sports_event(event, filters) {
            filtered_non_sports = filtered_non_sports.saturating_add(1);
            continue;
        }
        let event_id = string_field(event, &["id", "eventId", "event_id"]);
        let event_title = string_field(event, &["title", "question"]);
        let (sport, league) = sport_and_league_from_event(event);
        let event_start_time = string_field(
            event,
            &[
                "eventDate",
                "gameStartTime",
                "startTime",
                "startDate",
                "start_date",
                "startDateIso",
            ],
        );
        let event_markets = event
            .get("markets")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for market in event_markets {
            let active = bool_field(market, "active").unwrap_or(event_active);
            let closed = bool_field(market, "closed").unwrap_or(false);
            if !active || closed {
                filtered_closed = filtered_closed.saturating_add(1);
                continue;
            }
            let condition_id =
                string_field(market, &["conditionId", "condition_id"]).ok_or_else(|| {
                    ParserError::MissingRequiredField {
                        field: "conditionId".to_string(),
                    }
                })?;
            let token_ids =
                parse_string_array_field(market, &["clobTokenIds", "clob_token_ids", "tokenIds"]);
            if token_ids.is_empty() {
                return Err(ParserError::MissingRequiredField {
                    field: "clobTokenIds".to_string(),
                });
            }
            let outcome_names = parse_string_array_field(market, &["outcomes", "outcomeNames"]);
            let market_title = string_field(market, &["question", "title"])
                .unwrap_or_else(|| condition_id.clone());
            let slug = string_field(market, &["slug"])
                .or_else(|| string_field(event, &["slug"]))
                .unwrap_or_else(|| condition_id.clone());
            let start_time = string_field(
                market,
                &["gameStartTime", "startTime", "startDate", "startDateIso"],
            )
            .or_else(|| event_start_time.clone());
            let market_type = market_type_from_discovery(market);
            if !market_type_allowed(market_type.as_deref(), filters) {
                filtered_unsupported_market_types =
                    filtered_unsupported_market_types.saturating_add(1);
                continue;
            }
            let line = string_field(market, &["line"])
                .or_else(|| {
                    market
                        .pointer("/metadata/mainSpreadLine")
                        .and_then(value_to_string)
                })
                .or_else(|| {
                    market
                        .pointer("/metadata/mainTotalLine")
                        .and_then(value_to_string)
                });
            markets.push(DiscoveredMarket {
                event_id: event_id.clone(),
                event_title: event_title.clone(),
                market_title,
                slug,
                sport: sport.clone(),
                league: league.clone(),
                condition_id,
                token_ids,
                outcome_names,
                start_time,
                market_type,
                line,
                status: "active".to_string(),
            });
        }
    }

    let provider_event_id = markets.first().map(|market| market.condition_id.clone());
    let provider_market_id = markets
        .first()
        .and_then(|market| market.token_ids.first().cloned());
    let raw_ref = raw_ref(
        &SourceChannel::RestDiscovery,
        provider_event_id.as_deref(),
        None,
        &payload_hash(&payload),
    );
    let raw = RawMessage::new(
        Provider::Polymarket,
        SourceChannel::RestDiscovery,
        provider_event_id.clone(),
        provider_event_id,
        provider_market_id,
        received_at,
        received_mono_ns,
        raw_ref,
        schema_version.to_string(),
        payload,
    );

    Ok(DiscoveryResult {
        raw,
        markets,
        filtered_closed,
        filtered_non_sports,
        filtered_unsupported_market_types,
        missing_token_ids,
    })
}

pub(crate) fn raw_ref(
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
    format!("raw/polymarket/{channel}/{event}/{message}/{hash}.json")
}

pub(crate) fn raw_from_fields(schema_version: &str, fields: RawFields) -> RawMessage {
    let raw_ref = raw_ref(
        &fields.source_channel,
        fields.provider_event_id.as_deref(),
        fields.provider_message_id.as_deref(),
        &payload_hash(&fields.payload),
    );
    RawMessage::new(
        Provider::Polymarket,
        fields.source_channel,
        fields.provider_message_id,
        fields.provider_event_id,
        fields.provider_market_id,
        fields.received_at,
        fields.received_mono_ns,
        raw_ref,
        schema_version.to_string(),
        fields.payload,
    )
}

fn event_items(payload: &Value) -> Option<&[Value]> {
    payload
        .as_array()
        .map(Vec::as_slice)
        .or_else(|| {
            payload
                .get("data")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
        })
        .or_else(|| {
            payload
                .get("events")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
        })
}

fn bool_field(value: &Value, field: &str) -> Option<bool> {
    value.get(field).and_then(Value::as_bool)
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(value_to_string))
}

fn parse_string_array_field(value: &Value, fields: &[&str]) -> Vec<String> {
    let Some(value) = fields.iter().find_map(|field| value.get(*field)) else {
        return Vec::new();
    };
    match value {
        Value::Array(items) => items.iter().filter_map(value_to_string).collect(),
        Value::String(text) => serde_json::from_str::<Vec<String>>(text).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn is_sports_event(event: &Value, filters: &DiscoveryFilters) -> bool {
    let mut haystack = String::new();
    for field in ["title", "question", "slug", "category"] {
        if let Some(value) = event.get(field).and_then(Value::as_str) {
            haystack.push(' ');
            haystack.push_str(&value.to_ascii_lowercase());
        }
    }
    if let Some(tags) = event.get("tags").and_then(Value::as_array) {
        for tag in tags {
            for field in ["label", "name", "slug"] {
                if let Some(value) = tag.get(field).and_then(Value::as_str) {
                    haystack.push(' ');
                    haystack.push_str(&value.to_ascii_lowercase());
                }
            }
        }
    }
    filters
        .allowed_sports
        .iter()
        .map(|sport| sport.to_ascii_lowercase())
        .any(|sport| haystack.contains(&sport))
}

fn sport_and_league_from_event(event: &Value) -> (String, String) {
    let mut labels = Vec::new();
    if let Some(tags) = event.get("tags").and_then(Value::as_array) {
        for tag in tags {
            for field in ["slug", "label", "name"] {
                if let Some(value) = tag.get(field).and_then(Value::as_str) {
                    labels.push(value.to_ascii_lowercase());
                }
            }
        }
    }
    for field in ["category", "slug", "title"] {
        if let Some(value) = event.get(field).and_then(Value::as_str) {
            labels.push(value.to_ascii_lowercase());
        }
    }

    for league in [
        "nba", "nfl", "mlb", "nhl", "wnba", "atp", "wta", "tennis", "epl", "mls",
    ] {
        if labels
            .iter()
            .any(|value| value == league || value.contains(league))
        {
            return (league.to_string(), league.to_string());
        }
    }
    if labels
        .iter()
        .any(|value| value == "soccer" || value.contains("soccer") || value.contains("football"))
    {
        let league = labels
            .iter()
            .find(|value| {
                !matches!(
                    value.as_str(),
                    "sports" | "games" | "soccer" | "football" | "football-soccer"
                )
            })
            .cloned()
            .unwrap_or_else(|| "soccer".to_string());
        return ("soccer".to_string(), league);
    }
    ("unknown_sport".to_string(), "unknown_league".to_string())
}

fn market_type_from_discovery(market: &Value) -> Option<String> {
    let text = [
        string_field(
            market,
            &[
                "sportsMarketTypeV2",
                "sportsMarketType",
                "marketType",
                "question",
                "slug",
            ],
        ),
        market
            .pointer("/metadata/gameState/sportsMarketTypeV2")
            .and_then(value_to_string),
        market
            .pointer("/metadata/gameState/sportsMarketType")
            .and_then(value_to_string),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();

    if text.contains("moneyline") {
        Some("moneyline".to_string())
    } else if text.contains("spread") || text.contains("handicap") {
        Some("spread".to_string())
    } else if text.contains("total") || text.contains("over_under") {
        Some("total".to_string())
    } else {
        None
    }
}

fn market_type_allowed(market_type: Option<&str>, filters: &DiscoveryFilters) -> bool {
    let Some(market_type) = market_type else {
        return filters.allowed_market_types.is_empty();
    };
    filters.allowed_market_types.is_empty()
        || filters
            .allowed_market_types
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(market_type))
}
