use crate::polymarket::parser::ParserError;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeoblockStatus {
    pub blocked: bool,
    pub country: Option<String>,
    pub region: Option<String>,
    pub ip: Option<String>,
}

impl GeoblockStatus {
    pub fn parse(payload: Value) -> Result<Self, ParserError> {
        let blocked = payload
            .get("blocked")
            .and_then(Value::as_bool)
            .ok_or_else(|| ParserError::MissingRequiredField {
                field: "blocked".to_string(),
            })?;
        Ok(Self {
            blocked,
            country: payload
                .get("country")
                .and_then(Value::as_str)
                .map(str::to_string),
            region: payload
                .get("region")
                .and_then(Value::as_str)
                .map(str::to_string),
            ip: payload
                .get("ip")
                .and_then(Value::as_str)
                .map(|_| "<redacted-ip>".to_string()),
        })
    }

    pub fn sanitized_payload(&self) -> Value {
        serde_json::json!({
            "blocked": self.blocked,
            "country": self.country,
            "region": self.region,
            "ip": self.ip.clone().unwrap_or_else(|| "<redacted-ip>".to_string()),
        })
    }
}
