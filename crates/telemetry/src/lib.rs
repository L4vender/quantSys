use prometheus::{Encoder, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder};
use thiserror::Error;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("prometheus metric error: {0}")]
    Prometheus(#[from] prometheus::Error),
    #[error("utf8 error while encoding metrics: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

#[derive(Clone)]
pub struct Metrics {
    registry: Registry,
    service_ready: IntGaugeVec,
    source_stale_total: IntCounterVec,
    service_name: String,
}

impl Metrics {
    pub fn new(service_name: impl Into<String>) -> Result<Self, TelemetryError> {
        let service_name = service_name.into();
        let registry = Registry::new();
        let service_ready = IntGaugeVec::new(
            Opts::new(
                "quantsys_service_ready",
                "Service readiness by service name.",
            ),
            &["service"],
        )?;
        let source_stale_total = IntCounterVec::new(
            Opts::new("quantsys_source_stale_total", "Source stale detections."),
            &["service", "source"],
        )?;

        registry.register(Box::new(service_ready.clone()))?;
        registry.register(Box::new(source_stale_total.clone()))?;

        Ok(Self {
            registry,
            service_ready,
            source_stale_total,
            service_name,
        })
    }

    pub fn set_service_ready(&self, ready: bool) {
        self.service_ready
            .with_label_values(&[&self.service_name])
            .set(i64::from(ready));
    }

    pub fn observe_source_stale(&self, source: &str) {
        self.source_stale_total
            .with_label_values(&[&self.service_name, source])
            .inc();
    }

    pub fn gather(&self) -> Result<String, TelemetryError> {
        let families = self.registry.gather();
        let encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}

pub fn init_json_logging(default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let subscriber = fmt()
        .json()
        .with_env_filter(filter)
        .with_current_span(true)
        .with_span_list(true)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub fn scrub_secrets(input: &str) -> String {
    let mut output = input.to_string();
    for key in [
        "api_key",
        "apikey",
        "secret",
        "passphrase",
        "private_key",
        "private-key",
        "signature",
    ] {
        output = redact_assignments(&output, key);
    }
    output
}

fn redact_assignments(input: &str, key: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut index = 0;
    let lower = input.to_ascii_lowercase();

    while let Some(relative_start) = lower[index..].find(key) {
        let start = index + relative_start;
        result.push_str(&input[index..start]);

        let mut cursor = start + key.len();
        while cursor < input.len() && input.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        if cursor >= input.len() || !matches!(input.as_bytes()[cursor], b'=' | b':') {
            result.push_str(&input[start..cursor]);
            index = cursor;
            continue;
        }

        cursor += 1;
        while cursor < input.len() && input.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        let value_start = cursor;
        while cursor < input.len()
            && !input.as_bytes()[cursor].is_ascii_whitespace()
            && !matches!(input.as_bytes()[cursor], b',' | b'}' | b']')
        {
            cursor += 1;
        }

        result.push_str(&input[start..value_start]);
        result.push_str("[REDACTED]");
        index = cursor;
    }

    result.push_str(&input[index..]);
    result
}
