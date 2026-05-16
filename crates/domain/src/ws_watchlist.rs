use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WsWatchlist {
    pub schema_version: String,
    #[serde(default)]
    pub run_id: Option<String>,
    pub generated_at: String,
    #[serde(default)]
    pub selection_policy: Option<String>,
    #[serde(default)]
    pub items: Vec<WsWatchlistItem>,
    #[serde(default)]
    pub therundown: WatchlistTheRundown,
    #[serde(default)]
    pub polymarket: WatchlistPolymarket,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WatchlistTheRundown {
    #[serde(default)]
    pub event_ids: Vec<String>,
    #[serde(default)]
    pub market_ids: Vec<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WatchlistPolymarket {
    #[serde(default)]
    pub condition_ids: Vec<String>,
    #[serde(default)]
    pub asset_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WsWatchlistItem {
    pub canonical_event_id: Option<String>,
    pub canonical_market_key: Option<String>,
    pub sport: Option<String>,
    pub league: Option<String>,
    pub event_name: Option<String>,
    pub event_start_time_utc: Option<String>,
    pub market_type: String,
    #[serde(default = "default_full_game")]
    pub period: String,
    pub line: Option<f64>,
    pub therundown_event_id: String,
    #[serde(default)]
    pub therundown_market_id: Option<String>,
    pub polymarket_event_id: Option<String>,
    pub polymarket_condition_id: String,
    #[serde(default)]
    pub polymarket_market_id: Option<String>,
    #[serde(default)]
    pub polymarket_asset_ids: Vec<String>,
    #[serde(default)]
    pub selection_reason: Option<String>,
    #[serde(default)]
    pub matched_market_count: Option<u32>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

impl WsWatchlist {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|source| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, source.to_string())
        })
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn therundown_event_ids(&self) -> Vec<String> {
        unique_strings(
            self.therundown
                .event_ids
                .iter()
                .chain(self.items.iter().map(|item| &item.therundown_event_id)),
        )
    }

    pub fn therundown_market_ids(&self) -> Vec<u32> {
        let mut values = BTreeSet::new();
        values.extend(self.therundown.market_ids.iter().copied());
        for item in &self.items {
            if let Some(value) = item
                .therundown_market_id
                .as_deref()
                .and_then(|value| value.parse::<u32>().ok())
            {
                values.insert(value);
            }
        }
        values.into_iter().collect()
    }

    pub fn polymarket_asset_ids(&self) -> Vec<String> {
        unique_strings(
            self.polymarket.asset_ids.iter().chain(
                self.items
                    .iter()
                    .flat_map(|item| item.polymarket_asset_ids.iter()),
            ),
        )
    }

    pub fn allows_therundown_market_price(
        &self,
        event_id: &str,
        market_id: u32,
        line: Option<f64>,
    ) -> bool {
        self.items.iter().any(|item| {
            item.therundown_event_id == event_id
                && item
                    .therundown_market_id
                    .as_deref()
                    .and_then(|value| value.parse::<u32>().ok())
                    == Some(market_id)
                && watchlist_line_matches(item.line, line)
        })
    }
}

fn unique_strings<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        result.push(trimmed.to_string());
    }
    result
}

fn watchlist_line_matches(expected: Option<f64>, actual: Option<f64>) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected), Some(actual)) => (expected.abs() - actual.abs()).abs() <= 0.001,
        _ => false,
    }
}

fn default_full_game() -> String {
    "full_game".to_string()
}
