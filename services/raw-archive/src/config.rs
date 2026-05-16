#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawArchiveProcessorConfig {
    pub object_key_prefix: String,
    pub consumer_group: String,
    pub stale_after_seconds: u64,
}

impl Default for RawArchiveProcessorConfig {
    fn default() -> Self {
        Self {
            object_key_prefix: String::new(),
            consumer_group: "raw-archive".to_string(),
            stale_after_seconds: 30,
        }
    }
}
