use crate::polymarket::parser::ParserError;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeProbe {
    pub server_unix_seconds: i64,
    pub local_unix_seconds: i64,
    pub offset_ms: i64,
    pub large_offset_warning: bool,
}

impl TimeProbe {
    pub fn parse_json(payload: Value, local_time: DateTime<Utc>) -> Result<Self, ParserError> {
        let seconds = payload
            .get("server_time")
            .or_else(|| payload.get("time"))
            .or_else(|| payload.get("serverTime"))
            .and_then(value_to_i64)
            .or_else(|| value_to_i64(&payload))
            .ok_or_else(|| ParserError::MissingRequiredField {
                field: "server_time".to_string(),
            })?;
        Ok(Self::from_server_unix_seconds(seconds, local_time))
    }

    pub fn from_server_unix_seconds(server_unix_seconds: i64, local_time: DateTime<Utc>) -> Self {
        let local_unix_seconds = local_time.timestamp();
        let offset_ms = (server_unix_seconds - local_unix_seconds) * 1_000;
        Self {
            server_unix_seconds,
            local_unix_seconds,
            offset_ms,
            large_offset_warning: offset_ms.abs() > 30_000,
        }
    }

    pub fn server_time(&self) -> Option<DateTime<Utc>> {
        Utc.timestamp_opt(self.server_unix_seconds, 0).single()
    }

    pub fn payload(&self) -> Value {
        serde_json::json!({
            "server_time": self.server_unix_seconds,
            "local_time": self.local_unix_seconds,
            "offset_ms": self.offset_ms,
            "large_offset_warning": self.large_offset_warning,
        })
    }
}

fn value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}
