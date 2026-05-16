use serde_json::Value;
use std::fmt;
use std::time::Duration;
use thiserror::Error;

use crate::polymarket::parser::ParserError;

#[derive(Debug, Error)]
pub enum PolymarketError {
    #[error("polymarket config error: {0}")]
    Config(String),
    #[error("polymarket transport error: {0}")]
    Transport(String),
    #[error("polymarket returned server status {status}")]
    Server { status: u16 },
    #[error("polymarket endpoint is rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("polymarket auth missing: {0}")]
    AuthMissing(String),
    #[error("polymarket auth failed")]
    AuthFailed,
    #[error("polymarket malformed json: {0}")]
    MalformedJson(String),
    #[error("polymarket schema error: {0}")]
    Schema(String),
    #[error("polymarket subscription error: {0}")]
    Subscription(String),
}

impl From<ParserError> for PolymarketError {
    fn from(value: ParserError) -> Self {
        Self::Schema(value.to_string())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct L2Credentials {
    api_key: String,
    secret: String,
    passphrase: String,
}

impl L2Credentials {
    pub fn new(
        api_key: impl Into<String>,
        secret: impl Into<String>,
        passphrase: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            secret: secret.into(),
            passphrase: passphrase.into(),
        }
    }

    pub fn from_env_names(
        api_key_env: &str,
        secret_env: &str,
        passphrase_env: &str,
    ) -> Result<Option<Self>, PolymarketError> {
        let api_key = match std::env::var(api_key_env) {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) | Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(PolymarketError::AuthMissing(format!(
                    "{api_key_env} contains non-unicode data"
                )))
            }
        };
        let secret = match std::env::var(secret_env) {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) | Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(PolymarketError::AuthMissing(format!(
                    "{secret_env} contains non-unicode data"
                )))
            }
        };
        let passphrase = match std::env::var(passphrase_env) {
            Ok(value) if !value.trim().is_empty() => value,
            Ok(_) | Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(PolymarketError::AuthMissing(format!(
                    "{passphrase_env} contains non-unicode data"
                )))
            }
        };
        Ok(Some(Self::new(api_key, secret, passphrase)))
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn passphrase(&self) -> &str {
        &self.passphrase
    }

    pub fn scrub(&self, text: &str) -> String {
        [self.api_key(), self.secret(), self.passphrase()]
            .iter()
            .fold(text.to_string(), |acc, secret| {
                scrub_secret_text(&acc, secret)
            })
    }
}

impl fmt::Debug for L2Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("L2Credentials")
            .field("api_key", &"<redacted>")
            .field("secret", &"<redacted>")
            .field("passphrase", &"<redacted>")
            .finish()
    }
}

impl fmt::Display for L2Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("L2Credentials(<redacted>)")
    }
}

pub fn scrub_secret_text(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "<redacted>")
}

pub fn redact_secret_json(value: &Value) -> Value {
    let mut value = value.clone();
    redact_secret_json_in_place(&mut value);
    value
}

pub fn redact_secret_json_in_place(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    *value = Value::String("<redacted>".to_string());
                } else {
                    redact_secret_json_in_place(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_secret_json_in_place(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "apikey"
            | "api_key"
            | "secret"
            | "passphrase"
            | "signature"
            | "privatekey"
            | "private_key"
            | "transaction_hash"
    )
}
