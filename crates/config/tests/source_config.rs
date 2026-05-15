use quantsys_config::{SecretRef, SourceConfigs};
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn loads_phase1_example_toml_with_secret_references() {
    let configs = SourceConfigs::load_from_dir(workspace_root().join("configs/sources")).unwrap();

    assert_eq!(
        configs.therundown.auth,
        SecretRef::Env("THERUNDON_API_KEY".to_string())
    );
    assert_eq!(configs.therundown.stale_after_seconds, 30);
    assert_eq!(configs.therundown.rest_timeout_ms, 5000);
    assert_eq!(configs.therundown.ws_connect_timeout_ms, 5000);
    assert_eq!(configs.therundown.max_reconnect_attempts, 5);
    assert!(configs.therundown.subscription_filters_required);
    assert_eq!(
        configs.polymarket.l2_auth.api_key,
        SecretRef::Env("POLYMARKET_API_KEY".to_string())
    );
    assert!(!configs.polymarket.execution_enabled);
}

#[test]
fn therundown_production_config_requires_subscription_filters() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("therundown.example.toml"),
        r#"
enabled = true
api_base_url = "https://therundown.io/api/v2"
ws_url = "wss://therundown.io/api/v2/ws/markets"
auth_env = "THERUNDON_API_KEY"
sport_ids = []
market_ids = []
affiliate_ids = []
event_ids = []
heartbeat_timeout_seconds = 15
stale_after_seconds = 30
subscription_filters_required = true
real_time_required = true
disable_live_signal_when_delayed = true
[reconnect_backoff]
initial_ms = 500
max_ms = 30000
jitter_ms = 250
max_attempts_before_bootstrap = 3
[rate_budget]
requests_per_second = 10
respect_retry_after = true
rate_limited_disables_live_signal = true
[datapoint_budget]
monthly_limit = 40000000
min_remaining_for_live_signal = 100000
exhausted_disables_live_signal = true
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("polymarket.example.toml"),
        include_str!("../../../configs/sources/polymarket.example.toml"),
    )
    .unwrap();

    let err = SourceConfigs::load_from_dir(dir.path()).unwrap_err();

    assert!(err.to_string().contains("subscriptions require"));
}

#[test]
fn environment_overrides_non_secret_runtime_flags() {
    std::env::set_var("QUANTSYS_THERUNDOWN_ENABLED", "true");
    std::env::set_var("QUANTSYS_POLYMARKET_CUSTOM_FEATURE_ENABLED", "false");

    let configs = SourceConfigs::load_from_dir(workspace_root().join("configs/sources")).unwrap();

    std::env::remove_var("QUANTSYS_THERUNDOWN_ENABLED");
    std::env::remove_var("QUANTSYS_POLYMARKET_CUSTOM_FEATURE_ENABLED");

    assert!(configs.therundown.enabled);
    assert!(!configs.polymarket.custom_feature_enabled);
}

#[test]
fn invalid_stale_threshold_fails_fast() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("therundown.example.toml"),
        r#"
enabled = false
api_base_url = "https://therundown.io/api/v2"
ws_url = "wss://therundown.io/api/v2/ws/markets"
auth_env = "THERUNDON_API_KEY"
sport_ids = [4]
market_ids = [1]
affiliate_ids = [19]
event_ids = []
heartbeat_timeout_seconds = 30
stale_after_seconds = 15
real_time_required = true
disable_live_signal_when_delayed = true
[reconnect_backoff]
initial_ms = 500
max_ms = 30000
jitter_ms = 250
max_attempts_before_bootstrap = 3
[rate_budget]
requests_per_second = 10
respect_retry_after = true
rate_limited_disables_live_signal = true
[datapoint_budget]
monthly_limit = 40000000
min_remaining_for_live_signal = 100000
exhausted_disables_live_signal = true
"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("polymarket.example.toml"),
        include_str!("../../../configs/sources/polymarket.example.toml"),
    )
    .unwrap();

    let err = SourceConfigs::load_from_dir(dir.path()).unwrap_err();

    assert!(err.to_string().contains("stale_after_seconds"));
}
