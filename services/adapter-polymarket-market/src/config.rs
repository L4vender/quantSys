use anyhow::Context;
use quantsys_config::PolymarketConfig;
use quantsys_source_sdk::polymarket::{
    DiscoveryFilters, PolymarketBackoff, PolymarketMarketAdapterConfig,
};
use quantsys_storage::LocalCsvSink;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn load_config(path: impl AsRef<Path>) -> anyhow::Result<PolymarketConfig> {
    PolymarketConfig::load_from_file(path.as_ref())
        .with_context(|| format!("loading Polymarket config {}", path.as_ref().display()))
}

pub fn adapter_config(config: &PolymarketConfig) -> PolymarketMarketAdapterConfig {
    PolymarketMarketAdapterConfig {
        gamma_api_base_url: config.gamma_api_base_url.clone(),
        geoblock_url: config.geoblock_url.clone(),
        server_time_url: config.server_time_url.clone(),
        schema_version: "polymarket.phase4.raw.v1".to_string(),
        discovery_limit: config.discovery_filters.limit,
        discovery_offset: config.discovery_filters.offset,
        discovery_game_tag_id: config.discovery_filters.game_tag_id,
        discovery_filters: DiscoveryFilters {
            sports_only: config.discovery_filters.sports_only,
            allowed_sports: config.sport_filters.allowed_sports.clone(),
            allowed_market_types: config.discovery_filters.allowed_market_types.clone(),
        },
        stale_after_seconds: config.stale_after_seconds,
        rest_timeout: Duration::from_millis(config.rest_timeout_ms),
        token_cache_ttl_seconds: config.token_cache_ttl_seconds,
        max_token_subscriptions: config.max_token_subscriptions,
        reconnect_backoff: PolymarketBackoff::new(
            config.reconnect_backoff.initial_ms,
            config.reconnect_backoff.max_ms,
            config.reconnect_backoff.jitter_ms,
        ),
    }
}

pub fn local_csv_sink(
    config: &PolymarketConfig,
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
