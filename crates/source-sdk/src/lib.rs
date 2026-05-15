use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use quantsys_domain::SourceState;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("source unavailable: {0}")]
    Unavailable(String),
    #[error("source operation failed: {0}")]
    Operation(String),
}

#[async_trait]
pub trait SourceAdapter {
    async fn bootstrap(&mut self) -> Result<SourceState, SourceError>;
    async fn poll_once(&mut self) -> Result<Option<SourceEvent>, SourceError>;
    async fn state(&self) -> SourceState;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEvent {
    pub provider: String,
    pub channel: String,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartbeatStaleDetector {
    stale_after: Duration,
}

impl HeartbeatStaleDetector {
    pub fn new(stale_after: Duration) -> Self {
        Self { stale_after }
    }

    pub fn is_stale(&self, last_seen: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        match last_seen {
            Some(last_seen) => now.signed_duration_since(last_seen) > self.stale_after,
            None => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    consecutive_failures: u32,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32) -> Self {
        Self {
            failure_threshold,
            consecutive_failures: 0,
        }
    }

    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    pub fn is_open(&self) -> bool {
        self.consecutive_failures >= self.failure_threshold
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenBucket {
    capacity: u32,
    remaining: u32,
}

impl TokenBucket {
    pub fn new(capacity: u32) -> Self {
        Self {
            capacity,
            remaining: capacity,
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }

    pub fn refill(&mut self) {
        self.remaining = self.capacity;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectBackoff {
    initial_ms: u64,
    max_ms: u64,
    jitter_ms: u64,
}

impl ReconnectBackoff {
    pub fn new(initial_ms: u64, max_ms: u64, jitter_ms: u64) -> Self {
        Self {
            initial_ms,
            max_ms,
            jitter_ms,
        }
    }

    pub fn delay_ms(&self, attempt: u32) -> u64 {
        let multiplier = 2_u64.saturating_pow(attempt.min(31));
        self.initial_ms.saturating_mul(multiplier).min(self.max_ms)
    }

    pub fn jitter_ms(&self) -> u64 {
        self.jitter_ms
    }
}
