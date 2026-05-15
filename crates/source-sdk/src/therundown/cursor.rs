use crate::therundown::error::TheRundownError;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaCursor {
    last_id: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    stale_after_minutes: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapCursorUpdate {
    complete: bool,
}

impl BootstrapCursorUpdate {
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

impl DeltaCursor {
    pub fn new(stale_after_minutes: i64) -> Self {
        Self {
            last_id: None,
            updated_at: None,
            stale_after_minutes,
        }
    }

    pub fn last_id(&self) -> Option<&str> {
        self.last_id.as_deref()
    }

    pub fn set_last_id(&mut self, last_id: impl Into<String>, now: DateTime<Utc>) {
        self.last_id = Some(last_id.into());
        self.updated_at = Some(now);
    }

    pub fn update_from_bootstrap(
        &mut self,
        payload: &Value,
        now: DateTime<Utc>,
    ) -> Result<BootstrapCursorUpdate, TheRundownError> {
        let delta_last_id = payload
            .pointer("/meta/delta_last_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let complete = delta_last_id.is_some();
        if let Some(delta_last_id) = delta_last_id {
            self.set_last_id(delta_last_id, now);
        }
        Ok(BootstrapCursorUpdate { complete })
    }

    pub fn update_from_delta(
        &mut self,
        payload: &Value,
        now: DateTime<Utc>,
    ) -> Result<(), TheRundownError> {
        if let Some(next_last_id) = payload
            .pointer("/meta/next_last_id")
            .or_else(|| payload.pointer("/meta/last_id"))
            .and_then(Value::as_str)
        {
            self.set_last_id(next_last_id.to_string(), now);
        }
        Ok(())
    }

    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        match self.updated_at {
            Some(updated_at) => {
                now.signed_duration_since(updated_at)
                    > ChronoDuration::minutes(self.stale_after_minutes)
            }
            None => true,
        }
    }

    pub fn should_rebootstrap(error: &TheRundownError) -> bool {
        matches!(error, TheRundownError::CursorStale)
    }
}
