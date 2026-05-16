use quantsys_domain::DlqEvent;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct InMemoryDlqStore {
    events: Arc<Mutex<Vec<DlqEvent>>>,
}

impl InMemoryDlqStore {
    pub fn insert(&self, event: DlqEvent) {
        self.events
            .lock()
            .expect("dlq store mutex poisoned")
            .push(event);
    }

    pub fn list(&self) -> Vec<DlqEvent> {
        self.events
            .lock()
            .expect("dlq store mutex poisoned")
            .clone()
    }

    pub fn len(&self) -> usize {
        self.events.lock().expect("dlq store mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
