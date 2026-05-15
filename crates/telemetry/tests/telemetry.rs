use quantsys_telemetry::{scrub_secrets, Metrics};

#[test]
fn scrubber_removes_common_secret_values() {
    let text = "api_key=abc12345678901234567890 secret: shh123456789012345 private_key=0xabcdef passphrase=hunter2 signature=0xsig";
    let scrubbed = scrub_secrets(text);

    assert!(!scrubbed.contains("abc12345678901234567890"));
    assert!(!scrubbed.contains("shh123456789012345"));
    assert!(!scrubbed.contains("0xabcdef"));
    assert!(scrubbed.contains("[REDACTED]"));
}

#[test]
fn prometheus_metrics_skeleton_exports_service_and_source_metrics() {
    let metrics = Metrics::new("api-gateway").unwrap();
    metrics.set_service_ready(true);
    metrics.observe_source_stale("therundown");

    let encoded = metrics.gather().unwrap();

    assert!(encoded.contains("quantsys_service_ready"));
    assert!(encoded.contains("api-gateway"));
    assert!(encoded.contains("quantsys_source_stale_total"));
    assert!(encoded.contains("therundown"));
}
