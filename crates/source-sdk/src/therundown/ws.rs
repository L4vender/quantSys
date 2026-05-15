use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TheRundownBackoff {
    initial_ms: u64,
    max_ms: u64,
    jitter_ms: u64,
    max_reconnect_attempts: u32,
}

impl TheRundownBackoff {
    pub fn new(initial_ms: u64, max_ms: u64, jitter_ms: u64, max_reconnect_attempts: u32) -> Self {
        Self {
            initial_ms,
            max_ms,
            jitter_ms,
            max_reconnect_attempts,
        }
    }

    pub fn delay_ms(&self, attempt: u32) -> u64 {
        let multiplier = 2_u64.saturating_pow(attempt.min(31));
        self.initial_ms.saturating_mul(multiplier).min(self.max_ms)
    }

    pub fn delay_with_jitter_ms(&self, attempt: u32) -> u64 {
        self.delay_ms(attempt)
            .saturating_add(self.deterministic_jitter(attempt))
            .min(self.max_ms.saturating_add(self.jitter_ms))
    }

    pub fn delay(&self, attempt: u32) -> Duration {
        Duration::from_millis(self.delay_with_jitter_ms(attempt))
    }

    pub fn should_rebootstrap_after_attempt(&self, attempt: u32) -> bool {
        attempt >= self.max_reconnect_attempts
    }

    fn deterministic_jitter(&self, attempt: u32) -> u64 {
        if self.jitter_ms == 0 {
            return 0;
        }
        let value = (attempt as u64)
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345);
        value % (self.jitter_ms + 1)
    }
}
