use quantsys_storage::{
    InMemoryConsumerLagStore, InMemoryDlqStore, InMemoryRateBudgetStore, InMemoryRawArchiveIndex,
    InMemorySourceStateStore,
};

#[derive(Clone)]
pub struct SourceHealthAppState {
    pub source_states: InMemorySourceStateStore,
    pub rate_budgets: InMemoryRateBudgetStore,
    pub consumer_lag: InMemoryConsumerLagStore,
    pub dlq: InMemoryDlqStore,
    pub raw_index: InMemoryRawArchiveIndex,
}

impl SourceHealthAppState {
    pub fn new(
        source_states: InMemorySourceStateStore,
        rate_budgets: InMemoryRateBudgetStore,
        consumer_lag: InMemoryConsumerLagStore,
        dlq: InMemoryDlqStore,
        raw_index: InMemoryRawArchiveIndex,
    ) -> Self {
        Self {
            source_states,
            rate_budgets,
            consumer_lag,
            dlq,
            raw_index,
        }
    }
}
