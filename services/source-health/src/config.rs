#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceHealthConfig {
    pub lagging_threshold: i64,
}

impl Default for SourceHealthConfig {
    fn default() -> Self {
        Self {
            lagging_threshold: 100,
        }
    }
}
