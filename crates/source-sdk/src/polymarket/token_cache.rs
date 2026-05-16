use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredMarket {
    pub event_id: Option<String>,
    pub event_title: Option<String>,
    pub market_title: String,
    pub slug: String,
    pub sport: String,
    pub league: String,
    pub condition_id: String,
    pub token_ids: Vec<String>,
    pub outcome_names: Vec<String>,
    pub start_time: Option<String>,
    pub market_type: Option<String>,
    pub line: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenCache {
    ttl_seconds: u64,
    version: u64,
    source: String,
    updated_at: Option<DateTime<Utc>>,
    markets_by_condition: BTreeMap<String, DiscoveredMarket>,
    condition_by_token: BTreeMap<String, String>,
    outcome_by_token: BTreeMap<String, String>,
    condition_by_slug: BTreeMap<String, String>,
    conditions_by_event: BTreeMap<String, Vec<String>>,
}

impl TokenCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            version: 0,
            source: "polymarket_discovery".to_string(),
            updated_at: None,
            markets_by_condition: BTreeMap::new(),
            condition_by_token: BTreeMap::new(),
            outcome_by_token: BTreeMap::new(),
            condition_by_slug: BTreeMap::new(),
            conditions_by_event: BTreeMap::new(),
        }
    }

    pub fn upsert_markets(&mut self, markets: Vec<DiscoveredMarket>, now: DateTime<Utc>) {
        for market in markets {
            self.condition_by_slug
                .insert(market.slug.clone(), market.condition_id.clone());
            if let Some(event_id) = market.event_id.as_ref() {
                let conditions = self
                    .conditions_by_event
                    .entry(event_id.clone())
                    .or_default();
                if !conditions.iter().any(|item| item == &market.condition_id) {
                    conditions.push(market.condition_id.clone());
                }
            }
            for (idx, token_id) in market.token_ids.iter().enumerate() {
                self.condition_by_token
                    .insert(token_id.clone(), market.condition_id.clone());
                if let Some(outcome) = market.outcome_names.get(idx) {
                    self.outcome_by_token
                        .insert(token_id.clone(), outcome.clone());
                }
            }
            self.markets_by_condition
                .insert(market.condition_id.clone(), market);
        }
        self.updated_at = Some(now);
        self.version = self.version.saturating_add(1);
    }

    pub fn token_ids_for_condition(&self, condition_id: &str) -> Option<Vec<String>> {
        self.markets_by_condition
            .get(condition_id)
            .map(|market| market.token_ids.clone())
    }

    pub fn market_for_condition(&self, condition_id: &str) -> Option<&DiscoveredMarket> {
        self.markets_by_condition.get(condition_id)
    }

    pub fn market_for_token(&self, token_id: &str) -> Option<&DiscoveredMarket> {
        self.condition_for_token(token_id)
            .and_then(|condition_id| self.market_for_condition(condition_id))
    }

    pub fn condition_for_token(&self, token_id: &str) -> Option<&str> {
        self.condition_by_token.get(token_id).map(String::as_str)
    }

    pub fn outcome_for_token(&self, token_id: &str) -> Option<&str> {
        self.outcome_by_token.get(token_id).map(String::as_str)
    }

    pub fn condition_for_slug(&self, slug: &str) -> Option<&str> {
        self.condition_by_slug.get(slug).map(String::as_str)
    }

    pub fn condition_ids_for_event(&self, event_id: &str) -> Vec<String> {
        self.conditions_by_event
            .get(event_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn all_token_ids(&self) -> Vec<String> {
        self.markets_by_condition
            .values()
            .flat_map(|market| market.token_ids.clone())
            .collect()
    }

    pub fn condition_ids(&self) -> Vec<String> {
        self.markets_by_condition.keys().cloned().collect()
    }

    pub fn market_count(&self) -> usize {
        self.markets_by_condition.len()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.updated_at
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.updated_at {
            Some(updated_at) => {
                now.signed_duration_since(updated_at)
                    > ChronoDuration::seconds(self.ttl_seconds as i64)
            }
            None => true,
        }
    }
}
