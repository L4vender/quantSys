use chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EntitlementHeaders {
    pub tier: Option<String>,
    pub rate_limit: Option<u32>,
    pub data_delay_seconds: Option<u64>,
    pub websocket_access: Option<bool>,
    pub datapoints: Option<u64>,
    pub datapoints_breakdown: Option<String>,
    pub datapoints_limit: Option<u64>,
    pub datapoints_period: Option<String>,
    pub datapoints_remaining: Option<u64>,
    pub datapoints_reset: Option<String>,
    pub datapoints_used: Option<u64>,
    pub retry_after: Option<Duration>,
}

impl EntitlementHeaders {
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let now = Utc::now();
        Self::from_pairs_at(pairs, now)
    }

    pub fn from_pairs_at<I, K, V>(pairs: I, now: DateTime<Utc>) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut headers = Self::default();
        for (key, value) in pairs {
            headers.apply_pair(key.as_ref(), value.as_ref(), now);
        }
        headers
    }

    pub fn from_header_map(headers: &reqwest::header::HeaderMap) -> Self {
        let pairs = headers.iter().filter_map(|(key, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (key.as_str().to_string(), value.to_string()))
        });
        Self::from_pairs(pairs)
    }

    pub fn datapoints_exhausted(&self) -> bool {
        self.datapoints_remaining == Some(0)
    }

    fn apply_pair(&mut self, key: &str, value: &str, now: DateTime<Utc>) {
        match key.to_ascii_lowercase().as_str() {
            "x-tier" => self.tier = Some(value.to_string()),
            "x-rate-limit" => self.rate_limit = value.parse().ok(),
            "x-data-delay-seconds" => self.data_delay_seconds = value.parse().ok(),
            "x-websocket-access" => self.websocket_access = parse_bool(value),
            "x-datapoints" => self.datapoints = value.parse().ok(),
            "x-datapoints-breakdown" => self.datapoints_breakdown = Some(value.to_string()),
            "x-datapoints-limit" => self.datapoints_limit = value.parse().ok(),
            "x-datapoints-period" => self.datapoints_period = Some(value.to_string()),
            "x-datapoints-remaining" => self.datapoints_remaining = value.parse().ok(),
            "x-datapoints-reset" => self.datapoints_reset = Some(value.to_string()),
            "x-datapoints-used" => self.datapoints_used = value.parse().ok(),
            "retry-after" => self.retry_after = parse_retry_after(value, now).ok().flatten(),
            _ => {}
        }
    }
}

pub fn parse_retry_after(
    value: &str,
    now: DateTime<Utc>,
) -> Result<Option<Duration>, RetryAfterParseError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Ok(seconds) = trimmed.parse::<u64>() {
        return Ok(Some(Duration::from_secs(seconds)));
    }

    let parsed = DateTime::parse_from_rfc2822(trimmed)
        .or_else(|_| DateTime::parse_from_rfc3339(trimmed))
        .map_err(|_| RetryAfterParseError)?;
    let target = parsed.with_timezone(&Utc);
    let seconds = target.signed_duration_since(now).num_seconds().max(0) as u64;
    Ok(Some(Duration::from_secs(seconds)))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RetryAfterParseError;

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}
