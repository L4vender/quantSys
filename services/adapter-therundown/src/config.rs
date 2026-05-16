use anyhow::Context;
use quantsys_config::TheRundownConfig;
use quantsys_source_sdk::therundown::{
    ApiKey, SubscriptionFilters, TheRundownAdapterConfig, TheRundownBackoff,
};
use quantsys_storage::LocalCsvSink;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn load_config(path: impl AsRef<Path>) -> anyhow::Result<TheRundownConfig> {
    TheRundownConfig::load_from_file(path.as_ref())
        .with_context(|| format!("loading TheRundown config {}", path.as_ref().display()))
}

pub fn load_api_key(config: &TheRundownConfig) -> anyhow::Result<ApiKey> {
    ApiKey::from_env(config.auth_env_name()).with_context(|| {
        format!(
            "TheRundown API key is required; set env var {}",
            config.auth_env_name()
        )
    })
}

pub fn adapter_config(config: &TheRundownConfig) -> TheRundownAdapterConfig {
    TheRundownAdapterConfig {
        api_base_url: config.api_base_url.clone(),
        schema_version: "therundown.v2.phase3.raw.v1".to_string(),
        stale_after_seconds: config.stale_after_seconds,
        rest_timeout: Duration::from_millis(config.rest_timeout_ms),
        reconnect_backoff: TheRundownBackoff::new(
            config.reconnect_backoff.initial_ms,
            config.reconnect_backoff.max_ms,
            config.reconnect_backoff.jitter_ms,
            config.max_reconnect_attempts,
        ),
    }
}

pub fn subscription_filters(config: &TheRundownConfig) -> anyhow::Result<SubscriptionFilters> {
    let filters = SubscriptionFilters {
        sport_ids: config.sport_ids.clone(),
        market_ids: config.market_ids.clone(),
        affiliate_ids: config.affiliate_ids.clone(),
        event_ids: config.event_ids.clone(),
    };
    filters
        .validate(config.subscription_filters_required)
        .map_err(anyhow::Error::from)?;
    Ok(filters)
}

pub fn local_csv_sink(
    config: &TheRundownConfig,
    override_dir: Option<PathBuf>,
) -> anyhow::Result<Option<LocalCsvSink>> {
    let Some(base_dir) = override_dir.or_else(|| {
        config
            .local_csv
            .enabled
            .then(|| PathBuf::from(&config.local_csv.base_dir))
    }) else {
        return Ok(None);
    };
    LocalCsvSink::new(&base_dir)
        .map(Some)
        .with_context(|| format!("initializing local CSV sink at {}", base_dir.display()))
}
