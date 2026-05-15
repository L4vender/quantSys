use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretRef {
    Env(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ReconnectBackoffConfig {
    pub initial_ms: u64,
    pub max_ms: u64,
    pub jitter_ms: u64,
    #[serde(default)]
    pub max_attempts_before_bootstrap: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct RateBudgetConfig {
    pub requests_per_second: u32,
    pub respect_retry_after: bool,
    pub rate_limited_disables_live_signal: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct DatapointBudgetConfig {
    pub monthly_limit: u64,
    pub min_remaining_for_live_signal: u64,
    pub exhausted_disables_live_signal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheRundownConfig {
    pub enabled: bool,
    pub api_base_url: String,
    pub ws_url: String,
    pub auth: SecretRef,
    pub sport_ids: Vec<u32>,
    pub market_ids: Vec<u32>,
    pub affiliate_ids: Vec<u32>,
    pub event_ids: Vec<String>,
    pub heartbeat_timeout_seconds: u64,
    pub stale_after_seconds: u64,
    pub real_time_required: bool,
    pub disable_live_signal_when_delayed: bool,
    pub reconnect_backoff: ReconnectBackoffConfig,
    pub rate_budget: RateBudgetConfig,
    pub datapoint_budget: DatapointBudgetConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketL2AuthConfig {
    pub api_key: SecretRef,
    pub secret: SecretRef,
    pub passphrase: SecretRef,
    pub private_key: SecretRef,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketChannelConfig {
    #[serde(rename = "type")]
    pub channel_type: String,
    #[serde(default)]
    pub assets_ids: Vec<String>,
    #[serde(default)]
    pub markets: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct EndpointRateBudgetConfig {
    pub scope: String,
    pub budget_kind: String,
    pub limit: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolymarketConfig {
    pub enabled: bool,
    pub clob_api_base_url: String,
    pub market_ws_url: String,
    pub user_ws_url: String,
    pub geoblock_url: String,
    pub heartbeat_interval_seconds: u64,
    pub stale_after_seconds: u64,
    pub custom_feature_enabled: bool,
    pub execution_enabled: bool,
    pub geoblock_required: bool,
    pub signing_mode: String,
    pub wallet_mode: String,
    pub live_order_type: String,
    pub l2_auth: PolymarketL2AuthConfig,
    pub market_channel: PolymarketChannelConfig,
    pub user_channel: PolymarketChannelConfig,
    pub reconnect_backoff: ReconnectBackoffConfig,
    pub rate_budgets_by_endpoint: std::collections::BTreeMap<String, EndpointRateBudgetConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceConfigs {
    pub therundown: TheRundownConfig,
    pub polymarket: PolymarketConfig,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl SourceConfigs {
    pub fn load_from_dir(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let therundown = read_therundown(path.join("therundown.example.toml"))?;
        let polymarket = read_polymarket(path.join("polymarket.example.toml"))?;
        let mut configs = Self {
            therundown,
            polymarket,
        };

        configs.apply_env_overrides()?;
        configs.validate()?;
        Ok(configs)
    }

    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        if let Some(value) = read_bool_env("QUANTSYS_THERUNDOWN_ENABLED")? {
            self.therundown.enabled = value;
        }
        if let Some(value) = read_bool_env("QUANTSYS_POLYMARKET_CUSTOM_FEATURE_ENABLED")? {
            self.polymarket.custom_feature_enabled = value;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.therundown.stale_after_seconds <= self.therundown.heartbeat_timeout_seconds {
            return Err(ConfigError::Invalid(
                "therundown stale_after_seconds must be greater than heartbeat_timeout_seconds"
                    .to_string(),
            ));
        }
        if self.polymarket.stale_after_seconds <= self.polymarket.heartbeat_interval_seconds {
            return Err(ConfigError::Invalid(
                "polymarket stale_after_seconds must be greater than heartbeat_interval_seconds"
                    .to_string(),
            ));
        }
        if self.polymarket.execution_enabled {
            return Err(ConfigError::Invalid(
                "polymarket execution_enabled must remain false in Phase 2 examples".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct RawTheRundownConfig {
    enabled: bool,
    api_base_url: String,
    ws_url: String,
    auth_env: String,
    sport_ids: Vec<u32>,
    market_ids: Vec<u32>,
    affiliate_ids: Vec<u32>,
    event_ids: Vec<String>,
    heartbeat_timeout_seconds: u64,
    stale_after_seconds: u64,
    real_time_required: bool,
    disable_live_signal_when_delayed: bool,
    reconnect_backoff: ReconnectBackoffConfig,
    rate_budget: RateBudgetConfig,
    datapoint_budget: DatapointBudgetConfig,
}

#[derive(Debug, Deserialize)]
struct RawPolymarketConfig {
    enabled: bool,
    clob_api_base_url: String,
    market_ws_url: String,
    user_ws_url: String,
    geoblock_url: String,
    heartbeat_interval_seconds: u64,
    stale_after_seconds: u64,
    custom_feature_enabled: bool,
    execution_enabled: bool,
    geoblock_required: bool,
    signing_mode: String,
    wallet_mode: String,
    live_order_type: String,
    l2_auth_env: RawPolymarketL2AuthConfig,
    market_channel: PolymarketChannelConfig,
    user_channel: PolymarketChannelConfig,
    reconnect_backoff: ReconnectBackoffConfig,
    rate_budgets_by_endpoint: std::collections::BTreeMap<String, EndpointRateBudgetConfig>,
}

#[derive(Debug, Deserialize)]
struct RawPolymarketL2AuthConfig {
    api_key: String,
    secret: String,
    passphrase: String,
    private_key: String,
}

impl<'de> Deserialize<'de> for SecretRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(SecretRef::Env(value))
    }
}

fn read_therundown(path: impl AsRef<Path>) -> Result<TheRundownConfig, ConfigError> {
    let path = path.as_ref();
    let raw: RawTheRundownConfig = read_toml(path)?;
    Ok(TheRundownConfig {
        enabled: raw.enabled,
        api_base_url: raw.api_base_url,
        ws_url: raw.ws_url,
        auth: SecretRef::Env(raw.auth_env),
        sport_ids: raw.sport_ids,
        market_ids: raw.market_ids,
        affiliate_ids: raw.affiliate_ids,
        event_ids: raw.event_ids,
        heartbeat_timeout_seconds: raw.heartbeat_timeout_seconds,
        stale_after_seconds: raw.stale_after_seconds,
        real_time_required: raw.real_time_required,
        disable_live_signal_when_delayed: raw.disable_live_signal_when_delayed,
        reconnect_backoff: raw.reconnect_backoff,
        rate_budget: raw.rate_budget,
        datapoint_budget: raw.datapoint_budget,
    })
}

fn read_polymarket(path: impl AsRef<Path>) -> Result<PolymarketConfig, ConfigError> {
    let path = path.as_ref();
    let raw: RawPolymarketConfig = read_toml(path)?;
    Ok(PolymarketConfig {
        enabled: raw.enabled,
        clob_api_base_url: raw.clob_api_base_url,
        market_ws_url: raw.market_ws_url,
        user_ws_url: raw.user_ws_url,
        geoblock_url: raw.geoblock_url,
        heartbeat_interval_seconds: raw.heartbeat_interval_seconds,
        stale_after_seconds: raw.stale_after_seconds,
        custom_feature_enabled: raw.custom_feature_enabled,
        execution_enabled: raw.execution_enabled,
        geoblock_required: raw.geoblock_required,
        signing_mode: raw.signing_mode,
        wallet_mode: raw.wallet_mode,
        live_order_type: raw.live_order_type,
        l2_auth: PolymarketL2AuthConfig {
            api_key: SecretRef::Env(raw.l2_auth_env.api_key),
            secret: SecretRef::Env(raw.l2_auth_env.secret),
            passphrase: SecretRef::Env(raw.l2_auth_env.passphrase),
            private_key: SecretRef::Env(raw.l2_auth_env.private_key),
        },
        market_channel: raw.market_channel,
        user_channel: raw.user_channel,
        reconnect_backoff: raw.reconnect_backoff,
        rate_budgets_by_endpoint: raw.rate_budgets_by_endpoint,
    })
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.display().to_string(),
        source,
    })
}

fn read_bool_env(name: &str) -> Result<Option<bool>, ConfigError> {
    match std::env::var(name) {
        Ok(value) => parse_bool(name, &value).map(Some),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(ConfigError::Invalid(format!(
            "{name} contains non-unicode data"
        ))),
    }
}

fn parse_bool(name: &str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::Invalid(format!(
            "{name} must be a boolean override"
        ))),
    }
}
