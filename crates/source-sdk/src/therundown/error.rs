use std::fmt;
use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Eq, PartialEq)]
pub struct ApiKey {
    value: String,
}

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn from_env(name: &str) -> Result<Self, TheRundownError> {
        match std::env::var(name) {
            Ok(value) if !value.trim().is_empty() => Ok(Self::new(value)),
            Ok(_) | Err(std::env::VarError::NotPresent) => Err(TheRundownError::MissingApiKey {
                env: name.to_string(),
            }),
            Err(std::env::VarError::NotUnicode(_)) => Err(TheRundownError::Config(format!(
                "{name} contains non-unicode data"
            ))),
        }
    }

    pub fn header_name(&self) -> &'static str {
        "X-TheRundown-Key"
    }

    pub fn expose_for_transport(&self) -> &str {
        &self.value
    }

    pub fn scrub(&self, text: &str) -> String {
        scrub_secret_text(text, &self.value)
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Error)]
pub enum TheRundownError {
    #[error("missing TheRundown API key in env var {env}")]
    MissingApiKey { env: String },
    #[error("TheRundown authentication failed")]
    AuthFailed,
    #[error("TheRundown endpoint is rate limited; retry after {retry_after:?}")]
    RateLimited { retry_after: Option<Duration> },
    #[error("TheRundown server error status {status}")]
    Server { status: u16 },
    #[error("TheRundown cursor is stale")]
    CursorStale,
    #[error("TheRundown transport error: {0}")]
    Transport(String),
    #[error("TheRundown malformed JSON: {0}")]
    MalformedJson(String),
    #[error("TheRundown schema error: {0}")]
    Schema(String),
    #[error("TheRundown config error: {0}")]
    Config(String),
    #[error("TheRundown websocket error: {0}")]
    Websocket(String),
}

pub fn scrub_secret_text(text: &str, secret: &str) -> String {
    let mut scrubbed = if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "<redacted>")
    };

    scrubbed = scrub_query_value(&scrubbed, "key");
    scrubbed = scrub_header_value(&scrubbed, "X-TheRundown-Key");
    scrubbed = scrub_header_value(&scrubbed, "Authorization");
    scrubbed
}

fn scrub_query_value(text: &str, key: &str) -> String {
    let needle = format!("{key}=");
    let mut output = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(index) = remaining.find(&needle) {
        let (before, after_before) = remaining.split_at(index);
        output.push_str(before);
        output.push_str(&needle);
        output.push_str("<redacted>");
        let value_start = needle.len();
        let after = &after_before[value_start..];
        let end = after
            .find(['&', ' ', '\n', '\r', '\t'])
            .unwrap_or(after.len());
        remaining = &after[end..];
    }
    output.push_str(remaining);
    output
}

fn scrub_header_value(text: &str, header: &str) -> String {
    let needle = format!("{header}:");
    let lower = text.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(relative_index) = lower[cursor..].find(&lower_needle) {
        let index = cursor + relative_index;
        output.push_str(&text[cursor..index]);
        output.push_str(&text[index..index + needle.len()]);
        output.push(' ');
        output.push_str("<redacted>");
        let value_start = index + needle.len();
        let after = &text[value_start..];
        let end = after.find(['\n', '\r']).unwrap_or(after.len());
        cursor = value_start + end;
    }
    output.push_str(&text[cursor..]);
    output
}
