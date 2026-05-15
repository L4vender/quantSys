use chrono::{Duration, TimeZone, Utc};
use quantsys_source_sdk::{CircuitBreaker, HeartbeatStaleDetector, ReconnectBackoff, TokenBucket};

#[test]
fn heartbeat_detector_marks_source_stale_after_threshold() {
    let detector = HeartbeatStaleDetector::new(Duration::seconds(30));
    let last_seen = Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap();
    let now = last_seen + Duration::seconds(31);

    assert!(detector.is_stale(Some(last_seen), now));
    assert!(detector.is_stale(None, now));
    assert!(!detector.is_stale(Some(now), now));
}

#[test]
fn circuit_breaker_opens_after_consecutive_failures_and_resets_on_success() {
    let mut breaker = CircuitBreaker::new(3);

    breaker.record_failure();
    breaker.record_failure();
    assert!(!breaker.is_open());
    breaker.record_failure();
    assert!(breaker.is_open());
    breaker.record_success();
    assert!(!breaker.is_open());
}

#[test]
fn token_bucket_refuses_when_budget_is_empty() {
    let mut limiter = TokenBucket::new(2);

    assert!(limiter.try_acquire());
    assert!(limiter.try_acquire());
    assert!(!limiter.try_acquire());
    limiter.refill();
    assert!(limiter.try_acquire());
}

#[test]
fn reconnect_backoff_caps_after_growth() {
    let backoff = ReconnectBackoff::new(500, 30_000, 250);

    assert_eq!(backoff.delay_ms(0), 500);
    assert_eq!(backoff.delay_ms(1), 1_000);
    assert_eq!(backoff.delay_ms(20), 30_000);
    assert_eq!(backoff.jitter_ms(), 250);
}
