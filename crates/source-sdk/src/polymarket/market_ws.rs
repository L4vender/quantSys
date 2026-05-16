use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolymarketBackoff {
    initial_ms: u64,
    max_ms: u64,
    jitter_ms: u64,
}

impl PolymarketBackoff {
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

    pub fn delay(&self, attempt: u32) -> Duration {
        Duration::from_millis(self.delay_ms(attempt))
    }

    pub fn delay_with_jitter_ms(&self, attempt: u32) -> u64 {
        self.delay_ms(attempt).saturating_add(self.jitter_ms)
    }

    pub fn jitter_ms(&self) -> u64 {
        self.jitter_ms
    }
}
