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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LocalCsvConfig {
    pub enabled: bool,
    pub base_dir: String,
    pub flush_every_rows: u64,
    pub rotate_daily: bool,
    pub include_raw_refs: bool,
    pub write_single_provider_files: bool,
    pub write_comparison_files: bool,
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
    pub rest_timeout_ms: u64,
    pub ws_connect_timeout_ms: u64,
    pub max_reconnect_attempts: u32,
    pub subscription_filters_required: bool,
    pub real_time_required: bool,
    pub disable_live_signal_when_delayed: bool,
    pub reconnect_backoff: ReconnectBackoffConfig,
    pub rate_budget: RateBudgetConfig,
    pub datapoint_budget: DatapointBudgetConfig,
    pub local_csv: LocalCsvConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketL2AuthConfig {
    pub api_key: SecretRef,
    pub secret: SecretRef,
    pub passphrase: SecretRef,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketDiscoveryFiltersConfig {
    pub active: bool,
    pub closed: bool,
    pub limit: u32,
    pub offset: u32,
    pub sports_only: bool,
    #[serde(default = "default_polymarket_games_tag_id")]
    pub game_tag_id: Option<u64>,
    #[serde(default = "default_polymarket_market_types")]
    pub allowed_market_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PolymarketSportFiltersConfig {
    pub allowed_sports: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolymarketConfig {
    pub enabled: bool,
    pub clob_api_base_url: String,
    pub gamma_api_base_url: String,
    pub market_ws_url: String,
    pub user_ws_url: String,
    pub geoblock_url: String,
    pub server_time_url: String,
    pub heartbeat_interval_seconds: u64,
    pub stale_after_seconds: u64,
    pub rest_timeout_ms: u64,
    pub ws_connect_timeout_ms: u64,
    pub max_reconnect_attempts: u32,
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
    pub discovery_filters: PolymarketDiscoveryFiltersConfig,
    pub sport_filters: PolymarketSportFiltersConfig,
    pub max_token_subscriptions: usize,
    pub token_cache_ttl_seconds: u64,
    pub local_csv: LocalCsvConfig,
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
        validate_therundown(&self.therundown)?;
        validate_polymarket(&self.polymarket)?;
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
    #[serde(default = "default_rest_timeout_ms")]
    rest_timeout_ms: u64,
    #[serde(default = "default_ws_connect_timeout_ms")]
    ws_connect_timeout_ms: u64,
    #[serde(default = "default_max_reconnect_attempts")]
    max_reconnect_attempts: u32,
    #[serde(default = "default_subscription_filters_required")]
    subscription_filters_required: bool,
    real_time_required: bool,
    disable_live_signal_when_delayed: bool,
    reconnect_backoff: ReconnectBackoffConfig,
    rate_budget: RateBudgetConfig,
    datapoint_budget: DatapointBudgetConfig,
    #[serde(default = "default_local_csv_config")]
    local_csv: LocalCsvConfig,
}

#[derive(Debug, Deserialize)]
struct RawPolymarketConfig {
    enabled: bool,
    clob_api_base_url: String,
    #[serde(default = "default_gamma_api_base_url")]
    gamma_api_base_url: String,
    market_ws_url: String,
    user_ws_url: String,
    geoblock_url: String,
    #[serde(alias = "time_url", default = "default_polymarket_server_time_url")]
    server_time_url: String,
    heartbeat_interval_seconds: u64,
    stale_after_seconds: u64,
    #[serde(default = "default_rest_timeout_ms")]
    rest_timeout_ms: u64,
    #[serde(default = "default_ws_connect_timeout_ms")]
    ws_connect_timeout_ms: u64,
    #[serde(default = "default_max_reconnect_attempts")]
    max_reconnect_attempts: u32,
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
    #[serde(default = "default_polymarket_discovery_filters")]
    discovery_filters: PolymarketDiscoveryFiltersConfig,
    #[serde(default = "default_polymarket_sport_filters")]
    sport_filters: PolymarketSportFiltersConfig,
    #[serde(default = "default_max_token_subscriptions")]
    max_token_subscriptions: usize,
    #[serde(default = "default_token_cache_ttl_seconds")]
    token_cache_ttl_seconds: u64,
    #[serde(default = "default_local_csv_config")]
    local_csv: LocalCsvConfig,
}

#[derive(Debug, Deserialize)]
struct RawPolymarketL2AuthConfig {
    api_key: String,
    secret: String,
    passphrase: String,
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
        rest_timeout_ms: raw.rest_timeout_ms,
        ws_connect_timeout_ms: raw.ws_connect_timeout_ms,
        max_reconnect_attempts: raw.max_reconnect_attempts,
        subscription_filters_required: raw.subscription_filters_required,
        real_time_required: raw.real_time_required,
        disable_live_signal_when_delayed: raw.disable_live_signal_when_delayed,
        reconnect_backoff: raw.reconnect_backoff,
        rate_budget: raw.rate_budget,
        datapoint_budget: raw.datapoint_budget,
        local_csv: raw.local_csv,
    })
}

fn read_polymarket(path: impl AsRef<Path>) -> Result<PolymarketConfig, ConfigError> {
    let path = path.as_ref();
    let raw: RawPolymarketConfig = read_toml(path)?;
    Ok(PolymarketConfig {
        enabled: raw.enabled,
        clob_api_base_url: raw.clob_api_base_url,
        gamma_api_base_url: raw.gamma_api_base_url,
        market_ws_url: raw.market_ws_url,
        user_ws_url: raw.user_ws_url,
        geoblock_url: raw.geoblock_url,
        server_time_url: raw.server_time_url,
        heartbeat_interval_seconds: raw.heartbeat_interval_seconds,
        stale_after_seconds: raw.stale_after_seconds,
        rest_timeout_ms: raw.rest_timeout_ms,
        ws_connect_timeout_ms: raw.ws_connect_timeout_ms,
        max_reconnect_attempts: raw.max_reconnect_attempts,
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
        },
        market_channel: raw.market_channel,
        user_channel: raw.user_channel,
        reconnect_backoff: raw.reconnect_backoff,
        rate_budgets_by_endpoint: raw.rate_budgets_by_endpoint,
        discovery_filters: raw.discovery_filters,
        sport_filters: raw.sport_filters,
        max_token_subscriptions: raw.max_token_subscriptions,
        token_cache_ttl_seconds: raw.token_cache_ttl_seconds,
        local_csv: raw.local_csv,
    })
}

impl TheRundownConfig {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = read_therundown(path)?;
        validate_therundown(&config)?;
        Ok(config)
    }

    pub fn auth_env_name(&self) -> &str {
        match &self.auth {
            SecretRef::Env(name) => name,
        }
    }
}

impl PolymarketConfig {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = read_polymarket(path)?;
        validate_polymarket(&config)?;
        Ok(config)
    }

    pub fn l2_auth_env_names(&self) -> (&str, &str, &str) {
        (
            self.l2_auth.api_key.env_name(),
            self.l2_auth.secret.env_name(),
            self.l2_auth.passphrase.env_name(),
        )
    }
}

impl SecretRef {
    pub fn env_name(&self) -> &str {
        match self {
            SecretRef::Env(name) => name,
        }
    }
}

fn validate_therundown(config: &TheRundownConfig) -> Result<(), ConfigError> {
    if config.stale_after_seconds <= config.heartbeat_timeout_seconds {
        return Err(ConfigError::Invalid(
            "therundown stale_after_seconds must be greater than heartbeat_timeout_seconds"
                .to_string(),
        ));
    }
    if config.subscription_filters_required
        && config.sport_ids.is_empty()
        && config.market_ids.is_empty()
        && config.affiliate_ids.is_empty()
        && config.event_ids.is_empty()
    {
        return Err(ConfigError::Invalid(
            "therundown production subscriptions require sport_ids, market_ids, affiliate_ids, or event_ids"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_polymarket(config: &PolymarketConfig) -> Result<(), ConfigError> {
    if config.stale_after_seconds <= config.heartbeat_interval_seconds {
        return Err(ConfigError::Invalid(
            "polymarket stale_after_seconds must be greater than heartbeat_interval_seconds"
                .to_string(),
        ));
    }
    if config.execution_enabled {
        return Err(ConfigError::Invalid(
            "polymarket execution_enabled must remain false during Phase 4".to_string(),
        ));
    }
    if config.market_channel.channel_type != "market" {
        return Err(ConfigError::Invalid(
            "polymarket market_channel.type must be market".to_string(),
        ));
    }
    if config.user_channel.channel_type != "user" {
        return Err(ConfigError::Invalid(
            "polymarket user_channel.type must be user".to_string(),
        ));
    }
    if config.discovery_filters.allowed_market_types.is_empty() {
        return Err(ConfigError::Invalid(
            "polymarket discovery_filters.allowed_market_types cannot be empty".to_string(),
        ));
    }
    Ok(())
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

fn default_rest_timeout_ms() -> u64 {
    5_000
}

fn default_ws_connect_timeout_ms() -> u64 {
    5_000
}

fn default_max_reconnect_attempts() -> u32 {
    5
}

fn default_subscription_filters_required() -> bool {
    true
}

fn default_gamma_api_base_url() -> String {
    "https://gamma-api.polymarket.com".to_string()
}

fn default_polymarket_server_time_url() -> String {
    "https://clob.polymarket.com/time".to_string()
}

fn default_polymarket_discovery_filters() -> PolymarketDiscoveryFiltersConfig {
    PolymarketDiscoveryFiltersConfig {
        active: true,
        closed: false,
        limit: 100,
        offset: 0,
        sports_only: true,
        game_tag_id: default_polymarket_games_tag_id(),
        allowed_market_types: default_polymarket_market_types(),
    }
}

fn default_polymarket_games_tag_id() -> Option<u64> {
    Some(100_639)
}

fn default_polymarket_market_types() -> Vec<String> {
    vec![
        "moneyline".to_string(),
        "spread".to_string(),
        "total".to_string(),
    ]
}

fn default_polymarket_sport_filters() -> PolymarketSportFiltersConfig {
    PolymarketSportFiltersConfig {
        allowed_sports: vec![
            "nba".to_string(),
            "nfl".to_string(),
            "mlb".to_string(),
            "nhl".to_string(),
            "atp".to_string(),
            "wta".to_string(),
            "tennis".to_string(),
            "soccer".to_string(),
        ],
    }
}

fn default_max_token_subscriptions() -> usize {
    1_000
}

fn default_token_cache_ttl_seconds() -> u64 {
    300
}

fn default_local_csv_config() -> LocalCsvConfig {
    LocalCsvConfig {
        enabled: false,
        base_dir: "output/local-csv".to_string(),
        flush_every_rows: 1,
        rotate_daily: false,
        include_raw_refs: true,
        write_single_provider_files: true,
        write_comparison_files: false,
    }
}
