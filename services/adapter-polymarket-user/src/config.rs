use anyhow::Context;
use quantsys_config::PolymarketConfig;
use quantsys_source_sdk::polymarket::{
    L2Credentials, PolymarketBackoff, PolymarketError, PolymarketUserAdapterConfig,
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

pub fn load_config(path: impl AsRef<Path>) -> anyhow::Result<PolymarketConfig> {
    PolymarketConfig::load_from_file(path.as_ref())
        .with_context(|| format!("loading Polymarket config {}", path.as_ref().display()))
}

pub fn adapter_config(config: &PolymarketConfig) -> PolymarketUserAdapterConfig {
    PolymarketUserAdapterConfig {
        schema_version: "polymarket.phase4.user.raw.v1".to_string(),
        stale_after_seconds: config.stale_after_seconds,
        reconnect_backoff: PolymarketBackoff::new(
            config.reconnect_backoff.initial_ms,
            config.reconnect_backoff.max_ms,
            config.reconnect_backoff.jitter_ms,
        ),
    }
}

pub fn load_l2_credentials(
    config: &PolymarketConfig,
) -> Result<Option<L2Credentials>, PolymarketError> {
    let (api_key_env, secret_env, passphrase_env) = config.l2_auth_env_names();
    L2Credentials::from_env_names(api_key_env, secret_env, passphrase_env)
}

#[derive(Debug, Deserialize)]
struct MarketsFile {
    condition_ids: Vec<String>,
}

pub fn load_markets_file(path: impl AsRef<Path>) -> anyhow::Result<Vec<String>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file: MarketsFile = serde_json::from_str(&std::fs::read_to_string(path)?)
        .with_context(|| format!("loading Polymarket user markets file {}", path.display()))?;
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for id in file.condition_ids {
        let id = id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        ids.push(id.to_string());
    }
    Ok(ids)
}
