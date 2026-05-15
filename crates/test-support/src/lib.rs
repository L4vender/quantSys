use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct ExternalApiContractManifest {
    pub schema_version: String,
    pub generated_at: String,
    pub fixtures: Vec<FixtureManifestEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FixtureManifestEntry {
    pub fixture_path: String,
    pub provider: String,
    pub channel: String,
    pub message_type: String,
    pub source_type: String,
    pub source_url: String,
    pub captured_at: String,
    pub sanitized_fields: Vec<String>,
    pub schema_version: String,
    pub blocking_level: String,
    pub notes: String,
}

pub fn load_manifest() -> Result<ExternalApiContractManifest> {
    let path = workspace_root().join("tests/contract/external_api_contract_manifest.yaml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading manifest {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("parsing manifest {}", path.display()))
}

pub fn load_external_fixture(relative_path: &str) -> Result<Value> {
    let path = workspace_root()
        .join("tests/fixtures/external")
        .join(relative_path);
    load_json(path)
}

pub fn load_json(path: impl AsRef<Path>) -> Result<Value> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading JSON {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing JSON {}", path.display()))
}

pub fn canonical_json_hash(value: &Value) -> Result<String> {
    let canonical = canonical_json(value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(format!("sha256:{}", to_hex(&hasher.finalize())))
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => serde_json::to_string(value).expect("string serialization"),
        Value::Array(items) => {
            let inner = items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{inner}]")
        }
        Value::Object(map) => {
            let mut pairs = map.iter().collect::<Vec<_>>();
            pairs.sort_by(|left, right| left.0.cmp(right.0));
            let inner = pairs
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key serialization"),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate has parent")
        .parent()
        .expect("crates dir has parent")
        .to_path_buf()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
