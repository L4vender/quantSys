use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use quantsys_domain::{Provider, RawMessage, SourceChannel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};
use thiserror::Error;

const LOCAL_CSV_SCHEMA_VERSION: &str = "local_csv.v1";
const PROVIDER_PAIR: &str = "therundown_polymarket";

const PROVIDER_HEADERS: &[&str] = &[
    "data_generated_at",
    "data_fetched_at",
    "bookmaker",
    "affiliate_id",
    "team_a_polymarket_format",
    "team_b_polymarket_format",
];

#[derive(Debug, Error)]
pub enum LocalCsvError {
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("json error at {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported raw message for local csv: {0}")]
    UnsupportedRaw(String),
    #[error("timed out waiting for local csv process lock at {path}")]
    LockTimeout { path: String },
}

type Result<T> = std::result::Result<T, LocalCsvError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvProvider {
    TheRundown,
    Polymarket,
}

impl CsvProvider {
    fn slug(self) -> &'static str {
        match self {
            Self::TheRundown => "therundown",
            Self::Polymarket => "polymarket",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum MarketLine {
    NoLine,
    Point(f64),
    Raw(String),
}

impl MarketLine {
    pub fn file_component(&self) -> String {
        match self {
            Self::NoLine => "no_line".to_string(),
            Self::Point(value) => line_value_to_file_component(*value),
            Self::Raw(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    "no_line".to_string()
                } else if let Ok(parsed) = trimmed.parse::<f64>() {
                    line_value_to_file_component(parsed)
                } else {
                    slugify(trimmed, 48)
                }
            }
        }
    }

    pub fn display_value(&self) -> String {
        match self {
            Self::NoLine => String::new(),
            Self::Point(value) => trim_float(*value),
            Self::Raw(value) => value.clone(),
        }
    }

    fn key_component(&self) -> String {
        self.file_component()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MarketFileKey {
    pub sport: String,
    pub league: String,
    pub event_name: String,
    pub event_name_slug: String,
    pub event_start_time_utc: Option<String>,
    pub market_type: String,
    pub period: String,
    pub line_key: String,
    pub line: String,
}

impl MarketFileKey {
    pub fn new(
        sport: impl Into<String>,
        league: impl Into<String>,
        event_name: impl Into<String>,
        event_start_time_utc: Option<&str>,
        market_type: impl Into<String>,
        period: impl Into<String>,
        line: MarketLine,
    ) -> Self {
        let event_name = event_name.into();
        let event_name_slug = slugify(&event_name, 72);
        Self {
            sport: slugify(&sport.into(), 32),
            league: slugify(&league.into(), 32),
            event_name,
            event_name_slug,
            event_start_time_utc: event_start_time_utc.map(str::to_string),
            market_type: slugify(&market_type.into(), 32),
            period: slugify(&period.into(), 32),
            line_key: line.key_component(),
            line: line.display_value(),
        }
    }

    pub fn market_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.sport,
            self.league,
            self.event_start_time_utc
                .as_deref()
                .unwrap_or("unknown_time"),
            self.event_name_slug,
            self.market_type,
            self.period,
            self.line_key
        )
    }

    pub fn file_name(&self, suffix: &str) -> String {
        let start = self
            .event_start_time_utc
            .as_deref()
            .map(start_time_file_component)
            .unwrap_or_else(|| "unknown_time".to_string());
        let _ = suffix;
        let market = if self.line_key == "no_line" {
            self.market_type.clone()
        } else {
            format!("{}_{}", self.market_type, self.line_key)
        };
        format!("{}_{}_{}.csv", start, self.event_name_slug, market)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CsvProviderRecord {
    pub provider: CsvProvider,
    pub market_key: MarketFileKey,
    pub row_id: String,
    pub schema_version: String,
    pub side: String,
    pub outcome_name: String,
    pub provider_generated_at: Option<String>,
    pub fetched_at: String,
    pub ingest_mono_ns: u64,
    pub event_id: Option<String>,
    pub market_id: Option<String>,
    pub market_participant_id: Option<String>,
    pub normalized_market_participant_id: Option<String>,
    pub affiliate_id: Option<String>,
    pub sport_id: Option<String>,
    pub price_raw: Option<String>,
    pub previous_price_raw: Option<String>,
    pub price_delta: Option<String>,
    pub is_main_line: Option<String>,
    pub event_type: Option<String>,
    pub condition_id: Option<String>,
    pub token_id: Option<String>,
    pub asset_id: Option<String>,
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub last_trade_price: Option<String>,
    pub mid_price: Option<String>,
    pub book_depth: Option<String>,
    pub updated_at: Option<String>,
    pub quality_flags: Vec<String>,
    pub raw_ref: String,
    pub payload_hash: String,
    pub trace_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCsvWriteResult {
    pub provider_file: PathBuf,
    pub comparison_file: Option<PathBuf>,
    pub comparison_status: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct CompactMarketSnapshot {
    team_a_polymarket_format: Option<String>,
    team_b_polymarket_format: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TeamSlot {
    TeamA,
    TeamB,
}

#[derive(Clone, Debug)]
pub struct LocalCsvSink {
    base_dir: PathBuf,
    guard: Arc<Mutex<()>>,
}

impl LocalCsvSink {
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self> {
        let sink = Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            guard: Arc::new(Mutex::new(())),
        };
        sink.ensure_directories()?;
        Ok(sink)
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn write_raw_message(&self, raw: &RawMessage) -> Result<Vec<LocalCsvWriteResult>> {
        records_from_raw(raw)?
            .iter()
            .map(|record| self.write_provider_record(record))
            .collect()
    }

    pub fn write_provider_record(&self, record: &CsvProviderRecord) -> Result<LocalCsvWriteResult> {
        let _lock = self.guard.lock().expect("local csv sink mutex poisoned");
        self.ensure_directories()?;
        let _process_lock =
            LocalCsvProcessLock::acquire(&self.base_dir.join("_index/.write.lock"))?;

        let mut provider_dir = self
            .base_dir
            .join(record.provider.slug())
            .join(&record.market_key.league);
        if record.provider == CsvProvider::TheRundown {
            provider_dir = provider_dir.join(record_bookmaker_dir(record));
        }
        let provider_file = provider_dir.join(record.market_key.file_name(record.provider.slug()));

        let snapshot_path = self.snapshot_path(record);
        let mut snapshot =
            read_json_file::<CompactMarketSnapshot>(&snapshot_path)?.unwrap_or_default();
        apply_record_to_snapshot(record, &mut snapshot);
        write_json_file(&snapshot_path, &snapshot)?;

        append_csv_row(
            &provider_file,
            PROVIDER_HEADERS,
            &provider_row(record, &snapshot),
        )?;

        self.update_index(record, &provider_file)?;

        Ok(LocalCsvWriteResult {
            provider_file,
            comparison_file: None,
            comparison_status: None,
        })
    }

    fn ensure_directories(&self) -> Result<()> {
        for dir in [
            self.base_dir.as_path(),
            &self.base_dir.join("therundown"),
            &self.base_dir.join("polymarket"),
            &self.base_dir.join("_index"),
            &self.base_dir.join("_index/latest/therundown"),
            &self.base_dir.join("_index/latest/polymarket"),
        ] {
            fs::create_dir_all(dir).map_err(|source| LocalCsvError::Io {
                path: dir.display().to_string(),
                source,
            })?;
        }
        Ok(())
    }

    fn snapshot_path(&self, record: &CsvProviderRecord) -> PathBuf {
        let name = format!("{}.json", slugify(&record.market_key.market_key(), 120));
        self.base_dir
            .join("_index/latest")
            .join(record.provider.slug())
            .join(record_bookmaker_dir(record))
            .join(name)
    }

    fn update_index(&self, record: &CsvProviderRecord, provider_file: &Path) -> Result<()> {
        let index_path = self.base_dir.join("_index/markets_index.csv");
        let mut row = BTreeMap::new();
        row.insert("market_key".to_string(), record.market_key.market_key());
        row.insert("provider_pair".to_string(), PROVIDER_PAIR.to_string());
        row.insert("provider".to_string(), record.provider.slug().to_string());
        row.insert("sport".to_string(), record.market_key.sport.clone());
        row.insert("league".to_string(), record.market_key.league.clone());
        row.insert(
            "event_start_time_utc".to_string(),
            record
                .market_key
                .event_start_time_utc
                .clone()
                .unwrap_or_default(),
        );
        row.insert(
            "event_name".to_string(),
            record.market_key.event_name.clone(),
        );
        row.insert(
            "market_type".to_string(),
            record.market_key.market_type.clone(),
        );
        row.insert("period".to_string(), record.market_key.period.clone());
        row.insert("line".to_string(), record.market_key.line.clone());
        row.insert(
            "provider_file".to_string(),
            provider_file.display().to_string(),
        );
        row.insert("latest_updated_at".to_string(), Utc::now().to_rfc3339());
        append_csv_row(
            &index_path,
            &[
                "market_key",
                "provider_pair",
                "provider",
                "sport",
                "league",
                "event_start_time_utc",
                "event_name",
                "market_type",
                "period",
                "line",
                "provider_file",
                "latest_updated_at",
            ],
            &row,
        )?;

        let latest_path = self.base_dir.join("_index/latest_files.json");
        let mut latest =
            read_json_file::<BTreeMap<String, Value>>(&latest_path)?.unwrap_or_default();
        latest.insert(
            record.market_key.market_key(),
            json!({
                "provider_pair": PROVIDER_PAIR,
                "latest_provider": record.provider.slug(),
                "provider_file": provider_file.display().to_string(),
                "updated_at": Utc::now().to_rfc3339(),
            }),
        );
        write_json_file(&latest_path, &latest)?;
        Ok(())
    }
}

struct LocalCsvProcessLock {
    path: PathBuf,
}

impl LocalCsvProcessLock {
    fn acquire(path: &Path) -> Result<Self> {
        let started = SystemTime::now();
        let timeout = Duration::from_secs(30);
        loop {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut file) => {
                    let _ = writeln!(
                        file,
                        "pid={} acquired_at={}",
                        std::process::id(),
                        Utc::now().to_rfc3339()
                    );
                    return Ok(Self {
                        path: path.to_path_buf(),
                    });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    remove_stale_lock(path)?;
                    if started.elapsed().unwrap_or_default() >= timeout {
                        return Err(LocalCsvError::LockTimeout {
                            path: path.display().to_string(),
                        });
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(source) => {
                    return Err(LocalCsvError::Io {
                        path: path.display().to_string(),
                        source,
                    });
                }
            }
        }
    }
}

impl Drop for LocalCsvProcessLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn remove_stale_lock(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    let Ok(modified) = metadata.modified() else {
        return Ok(());
    };
    if lock_owner_has_exited(path)
        || modified
            .elapsed()
            .unwrap_or_default()
            .gt(&Duration::from_secs(120))
    {
        fs::remove_file(path).map_err(|source| LocalCsvError::Io {
            path: path.display().to_string(),
            source,
        })?;
    }
    Ok(())
}

fn lock_owner_has_exited(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };
    let Some(owner_pid) = contents.split_whitespace().find_map(|part| {
        part.strip_prefix("pid=")
            .and_then(|value| value.parse::<u32>().ok())
    }) else {
        return false;
    };
    if owner_pid == std::process::id() {
        return false;
    }
    match Command::new("kill")
        .arg("-0")
        .arg(owner_pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => !status.success(),
        Err(_) => false,
    }
}

pub fn records_from_raw(raw: &RawMessage) -> Result<Vec<CsvProviderRecord>> {
    match (&raw.provider, &raw.source_channel) {
        (Provider::TheRundown, SourceChannel::RestBootstrap) => {
            therundown_bootstrap_records_from_raw(raw)
        }
        (Provider::TheRundown, SourceChannel::WsMarket) => therundown_records_from_raw(raw),
        (Provider::Polymarket, SourceChannel::WsMarket) => polymarket_records_from_raw(raw),
        _ => Ok(Vec::new()),
    }
}

fn therundown_bootstrap_records_from_raw(raw: &RawMessage) -> Result<Vec<CsvProviderRecord>> {
    let Some(events) = raw
        .payload
        .get("events")
        .and_then(Value::as_array)
        .or_else(|| raw.payload.get("data").and_then(Value::as_array))
    else {
        return Ok(Vec::new());
    };

    let mut records = Vec::new();
    for event in events {
        let event_id = string_field(event, &["event_id", "id"]);
        let sport_id = string_field(event, &["sport_id"]);
        let (sport, league) = therundown_sport_and_league(sport_id.as_deref());
        let (away_name, home_name) = therundown_event_team_names(event);
        let event_name = match (&away_name, &home_name) {
            (Some(away), Some(home)) => format!("{away} vs {home}"),
            _ => string_field(event, &["event_name", "name", "title"])
                .or_else(|| event_id.clone())
                .unwrap_or_else(|| "unknown_event".to_string()),
        };
        let event_start_time = string_field(event, &["event_date", "start_time", "start_date"]);
        let markets = event
            .get("markets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();

        for market in markets {
            let market_id = string_field(market, &["market_id", "id"]);
            let market_type = string_field(market, &["market_type", "type"])
                .unwrap_or_else(|| therundown_market_type(market_id.as_deref()).to_string());
            let period = string_field(market, &["period"]).unwrap_or_else(|| "full_game".into());
            let market_affiliate_id = string_field(market, &["affiliate_id"]);
            let participants = market
                .get("participants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();

            for (idx, participant) in participants.enumerate() {
                let outcome_name = participant_outcome_name(
                    participant,
                    home_name.as_deref(),
                    away_name.as_deref(),
                )
                .unwrap_or_else(|| "unknown".to_string());
                let mut push_price = |price: &Value,
                                      line_value: Option<&Value>,
                                      affiliate_id: Option<String>,
                                      row_suffix: String| {
                    let provider_generated_at = price
                        .get("updated_at")
                        .and_then(value_to_string)
                        .or_else(|| participant.get("updated_at").and_then(value_to_string))
                        .or_else(|| market.get("updated_at").and_then(value_to_string))
                        .or_else(|| event.get("updated_at").and_then(value_to_string))
                        .or_else(|| {
                            raw.payload
                                .pointer("/meta/timestamp")
                                .and_then(value_to_iso_time)
                        });
                    let mut quality_flags = Vec::new();
                    if provider_generated_at.is_none() {
                        quality_flags.push("missing_provider_generated_at".to_string());
                    }
                    if price.get("price").is_some_and(is_off_board_price) {
                        quality_flags.push("off_board".to_string());
                    }

                    let market_key = MarketFileKey::new(
                        sport,
                        league,
                        event_name.clone(),
                        event_start_time.as_deref(),
                        market_type.clone(),
                        period.clone(),
                        line_from_value(
                            line_value
                                .or_else(|| participant.get("line"))
                                .or_else(|| market.get("line")),
                        ),
                    );
                    let payload_hash = raw.payload_hash.clone();
                    records.push(CsvProviderRecord {
                        provider: CsvProvider::TheRundown,
                        row_id: deterministic_row_id(
                            "therundown",
                            &market_key,
                            &payload_hash,
                            &row_suffix,
                        ),
                        schema_version: LOCAL_CSV_SCHEMA_VERSION.to_string(),
                        side: outcome_name.clone(),
                        outcome_name: outcome_name.clone(),
                        provider_generated_at: provider_generated_at.clone(),
                        fetched_at: iso_time(raw.received_at),
                        ingest_mono_ns: raw.received_mono_ns,
                        event_id: event_id.clone(),
                        market_id: market_id.clone(),
                        market_participant_id: price.get("id").and_then(value_to_string).or_else(
                            || string_field(participant, &["market_participant_id", "id"]),
                        ),
                        normalized_market_participant_id: string_field(
                            participant,
                            &["normalized_market_participant_id", "id"],
                        ),
                        affiliate_id: affiliate_id.or_else(|| market_affiliate_id.clone()),
                        sport_id: sport_id.clone(),
                        price_raw: price.get("price").and_then(value_to_string),
                        previous_price_raw: price.get("previous_price").and_then(value_to_string),
                        price_delta: price.get("price_delta").and_then(value_to_string),
                        is_main_line: price.get("is_main_line").and_then(value_to_string),
                        event_type: Some("rest_bootstrap".to_string()),
                        condition_id: None,
                        token_id: None,
                        asset_id: None,
                        best_bid: None,
                        best_ask: None,
                        last_trade_price: None,
                        mid_price: None,
                        book_depth: None,
                        updated_at: provider_generated_at,
                        quality_flags,
                        raw_ref: raw.raw_ref.clone(),
                        payload_hash,
                        trace_id: raw.trace_id.to_string(),
                        market_key,
                    });
                };

                if participant.get("price").is_some() {
                    push_price(
                        participant,
                        participant.get("line"),
                        market_affiliate_id.clone(),
                        idx.to_string(),
                    );
                }

                if let Some(lines) = participant.get("lines").and_then(Value::as_array) {
                    for (line_idx, line) in lines.iter().enumerate() {
                        let line_value = line.get("value").or_else(|| line.get("line"));
                        if let Some(prices) = line.get("prices").and_then(Value::as_object) {
                            for (affiliate_id, price) in prices {
                                let price_id = price
                                    .get("id")
                                    .and_then(value_to_string)
                                    .unwrap_or_else(|| "unknown_price".to_string());
                                push_price(
                                    price,
                                    line_value,
                                    Some(affiliate_id.clone()),
                                    format!("{idx}:{line_idx}:{affiliate_id}:{price_id}"),
                                );
                            }
                        } else if line.get("price").is_some() {
                            push_price(
                                line,
                                line_value,
                                market_affiliate_id.clone(),
                                format!("{idx}:{line_idx}"),
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(records)
}

fn therundown_records_from_raw(raw: &RawMessage) -> Result<Vec<CsvProviderRecord>> {
    if raw.payload.pointer("/meta/type").and_then(Value::as_str) != Some("market_price") {
        return Ok(Vec::new());
    }
    if raw.payload.get("_local_csv").is_none() {
        return Ok(Vec::new());
    }
    Ok(vec![therundown_record_from_raw(raw)?])
}

fn therundown_record_from_raw(raw: &RawMessage) -> Result<CsvProviderRecord> {
    let data = raw.payload.get("data").ok_or_else(|| {
        LocalCsvError::UnsupportedRaw("therundown ws payload missing data".into())
    })?;
    let metadata = raw.payload.get("_local_csv");
    let market_id = data.get("market_id").and_then(value_to_string);
    let market_type = metadata
        .and_then(|metadata| metadata.get("market_type"))
        .and_then(value_to_string)
        .unwrap_or_else(|| therundown_market_type(market_id.as_deref()).to_string());
    let line = line_from_value(data.get("line"));
    let sport_id = data.get("sport_id").and_then(value_to_string);
    let (fallback_sport, fallback_league) = therundown_sport_and_league(sport_id.as_deref());
    let sport = metadata
        .and_then(|metadata| metadata.get("sport"))
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback_sport.to_string());
    let league = metadata
        .and_then(|metadata| metadata.get("league"))
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback_league.to_string());
    let event_id = data.get("event_id").and_then(value_to_string);
    let event_name = metadata
        .and_then(|metadata| metadata.get("event_name"))
        .and_then(value_to_string)
        .or_else(|| event_id.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown_event".to_string());
    let event_start_time = metadata
        .and_then(|metadata| metadata.get("event_start_time_utc"))
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty());
    let provider_generated_at = data
        .get("updated_at")
        .and_then(value_to_string)
        .or_else(|| {
            raw.payload
                .pointer("/meta/timestamp")
                .and_then(value_to_iso_time)
        });
    let mut quality_flags = Vec::new();
    if provider_generated_at.is_none() {
        quality_flags.push("missing_provider_generated_at".to_string());
    }
    if data.get("price").is_some_and(is_off_board_price) {
        quality_flags.push("off_board".to_string());
    }
    if raw
        .payload
        .pointer("/quality_flags/off_board")
        .and_then(Value::as_bool)
        == Some(true)
    {
        quality_flags.push("off_board".to_string());
    }
    quality_flags.sort();
    quality_flags.dedup();

    let market_key = MarketFileKey::new(
        sport,
        league,
        event_name.clone(),
        event_start_time.as_deref(),
        market_type,
        "full_game",
        line,
    );
    let payload_hash = raw.payload_hash.clone();
    Ok(CsvProviderRecord {
        provider: CsvProvider::TheRundown,
        row_id: deterministic_row_id("therundown", &market_key, &payload_hash, "0"),
        schema_version: LOCAL_CSV_SCHEMA_VERSION.to_string(),
        side: therundown_outcome_from_metadata(&raw.payload, data)
            .or_else(|| {
                data.get("market_participant_id")
                    .and_then(value_to_string)
                    .map(|id| format!("participant_{id}"))
            })
            .unwrap_or_else(|| "unknown".to_string()),
        outcome_name: therundown_outcome_from_metadata(&raw.payload, data)
            .or_else(|| {
                data.get("normalized_market_participant_id")
                    .and_then(value_to_string)
                    .map(|id| format!("normalized_participant_{id}"))
            })
            .or_else(|| data.get("market_participant_id").and_then(value_to_string))
            .unwrap_or_else(|| "unknown".to_string()),
        provider_generated_at: provider_generated_at.clone(),
        fetched_at: iso_time(raw.received_at),
        ingest_mono_ns: raw.received_mono_ns,
        event_id,
        market_id,
        market_participant_id: data.get("market_participant_id").and_then(value_to_string),
        normalized_market_participant_id: data
            .get("normalized_market_participant_id")
            .and_then(value_to_string),
        affiliate_id: data.get("affiliate_id").and_then(value_to_string),
        sport_id,
        price_raw: data.get("price").and_then(value_to_string),
        previous_price_raw: data.get("previous_price").and_then(value_to_string),
        price_delta: data.get("price_delta").and_then(value_to_string),
        is_main_line: data.get("is_main_line").and_then(value_to_string),
        event_type: raw.payload.pointer("/meta/type").and_then(value_to_string),
        condition_id: None,
        token_id: None,
        asset_id: None,
        best_bid: None,
        best_ask: None,
        last_trade_price: None,
        mid_price: None,
        book_depth: None,
        updated_at: provider_generated_at,
        quality_flags,
        raw_ref: raw.raw_ref.clone(),
        payload_hash,
        trace_id: raw.trace_id.to_string(),
        market_key,
    })
}

fn therundown_outcome_from_metadata(payload: &Value, data: &Value) -> Option<String> {
    let outcomes = payload.pointer("/_local_csv/outcomes_by_participant")?;
    for field in ["market_participant_id", "normalized_market_participant_id"] {
        if let Some(id) = data.get(field).and_then(value_to_string) {
            if let Some(outcome) = outcomes.get(&id).and_then(value_to_string) {
                return Some(outcome);
            }
        }
    }
    None
}

fn polymarket_records_from_raw(raw: &RawMessage) -> Result<Vec<CsvProviderRecord>> {
    let Some(metadata) = raw.payload.get("_local_csv") else {
        return Ok(Vec::new());
    };
    let event_type = raw.payload.get("event_type").and_then(value_to_string);
    let condition_id = raw.payload.get("market").and_then(value_to_string);
    let market_type = metadata
        .get("market_type")
        .and_then(value_to_string)
        .unwrap_or_else(|| {
            polymarket_market_type(condition_id.as_deref(), event_type.as_deref()).to_string()
        });
    let event_name = metadata
        .get("event_name")
        .and_then(value_to_string)
        .or_else(|| condition_id.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown_polymarket_market".to_string());
    let sport = metadata
        .get("sport")
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| infer_sport_from_text(&event_name).to_string());
    let league = metadata
        .get("league")
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| sport.clone());
    let event_start_time = metadata
        .get("event_start_time_utc")
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty());
    let line = metadata
        .get("line")
        .filter(|value| !value.is_null())
        .or_else(|| raw.payload.get("line"));
    let provider_generated_at = raw
        .payload
        .get("timestamp")
        .and_then(value_to_iso_time)
        .or_else(|| {
            raw.payload
                .pointer("/data/timestamp")
                .and_then(value_to_iso_time)
        });
    let mut quality_flags = Vec::new();
    if provider_generated_at.is_none() {
        quality_flags.push("missing_provider_generated_at".to_string());
    }
    let market_key = MarketFileKey::new(
        sport,
        league,
        event_name.clone(),
        event_start_time.as_deref(),
        market_type,
        "full_game",
        line_from_value(line),
    );
    let changes = raw
        .payload
        .get("changes")
        .or_else(|| raw.payload.get("price_changes"))
        .and_then(Value::as_array);
    if let Some(changes) = changes {
        return Ok(changes
            .iter()
            .enumerate()
            .map(|(idx, change)| polymarket_record(raw, &market_key, change, idx, &quality_flags))
            .collect());
    }
    Ok(vec![polymarket_record(
        raw,
        &market_key,
        &raw.payload,
        0,
        &quality_flags,
    )])
}

fn polymarket_record(
    raw: &RawMessage,
    market_key: &MarketFileKey,
    source: &Value,
    idx: usize,
    quality_flags: &[String],
) -> CsvProviderRecord {
    let best_bid = source
        .get("best_bid")
        .and_then(value_to_string)
        .or_else(|| top_price(&raw.payload, "bids"));
    let best_ask = source
        .get("best_ask")
        .and_then(value_to_string)
        .or_else(|| top_price(&raw.payload, "asks"));
    let mid_price =
        market_decimal_mid(best_bid.as_deref(), best_ask.as_deref()).map(float_to_string);
    let event_type = raw.payload.get("event_type").and_then(value_to_string);
    let condition_id = raw.payload.get("market").and_then(value_to_string);
    let polymarket_event_id = raw
        .payload
        .pointer("/_local_csv/event_id")
        .and_then(value_to_string);
    let asset_id = source
        .get("asset_id")
        .and_then(value_to_string)
        .or_else(|| raw.payload.get("asset_id").and_then(value_to_string));
    let token_id = source
        .get("token_id")
        .and_then(value_to_string)
        .or_else(|| asset_id.clone());
    let provider_generated_at = raw
        .payload
        .get("timestamp")
        .and_then(value_to_iso_time)
        .or_else(|| source.get("timestamp").and_then(value_to_iso_time));
    let payload_hash = raw.payload_hash.clone();

    CsvProviderRecord {
        provider: CsvProvider::Polymarket,
        row_id: deterministic_row_id(
            "polymarket",
            market_key,
            &payload_hash,
            &format!("{idx}:{}", asset_id.as_deref().unwrap_or("unknown_asset")),
        ),
        schema_version: LOCAL_CSV_SCHEMA_VERSION.to_string(),
        side: source
            .get("side")
            .and_then(value_to_string)
            .or_else(|| polymarket_outcome_from_metadata(&raw.payload, asset_id.as_deref()))
            .or_else(|| asset_id.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        outcome_name: source
            .get("outcome")
            .or_else(|| source.get("outcome_name"))
            .and_then(value_to_string)
            .or_else(|| polymarket_outcome_from_metadata(&raw.payload, asset_id.as_deref()))
            .or_else(|| asset_id.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        provider_generated_at: provider_generated_at.clone(),
        fetched_at: iso_time(raw.received_at),
        ingest_mono_ns: raw.received_mono_ns,
        event_id: polymarket_event_id.or_else(|| condition_id.clone()),
        market_id: condition_id.clone(),
        market_participant_id: None,
        normalized_market_participant_id: None,
        affiliate_id: None,
        sport_id: None,
        price_raw: source.get("price").and_then(value_to_string),
        previous_price_raw: None,
        price_delta: None,
        is_main_line: None,
        event_type,
        condition_id,
        token_id,
        asset_id,
        best_bid,
        best_ask,
        last_trade_price: raw.payload.get("price").and_then(value_to_string),
        mid_price,
        book_depth: book_depth(&raw.payload).map(|depth| depth.to_string()),
        updated_at: provider_generated_at,
        quality_flags: quality_flags.to_vec(),
        raw_ref: raw.raw_ref.clone(),
        payload_hash,
        trace_id: raw.trace_id.to_string(),
        market_key: market_key.clone(),
    }
}

fn polymarket_outcome_from_metadata(payload: &Value, asset_id: Option<&str>) -> Option<String> {
    let asset_id = asset_id?;
    payload
        .pointer("/_local_csv/outcomes_by_token")
        .and_then(|value| value.get(asset_id))
        .and_then(value_to_string)
}

fn apply_record_to_snapshot(record: &CsvProviderRecord, snapshot: &mut CompactMarketSnapshot) {
    let Some(value) = record_polymarket_format_value(record) else {
        return;
    };
    match team_slot_for_record(record, snapshot) {
        TeamSlot::TeamA => {
            snapshot.team_a_polymarket_format = Some(value.clone());
            if snapshot.team_b_polymarket_format.is_none() {
                snapshot.team_b_polymarket_format = complement_probability(&value);
            }
        }
        TeamSlot::TeamB => {
            snapshot.team_b_polymarket_format = Some(value.clone());
            if snapshot.team_a_polymarket_format.is_none() {
                snapshot.team_a_polymarket_format = complement_probability(&value);
            }
        }
    }
}

fn record_polymarket_format_value(record: &CsvProviderRecord) -> Option<String> {
    match record.provider {
        CsvProvider::TheRundown => record
            .price_raw
            .as_deref()
            .and_then(american_odds_to_implied_probability)
            .map(float_to_string),
        CsvProvider::Polymarket => record
            .mid_price
            .as_deref()
            .and_then(parse_probability)
            .or_else(|| {
                record
                    .last_trade_price
                    .as_deref()
                    .and_then(parse_probability)
            })
            .or_else(|| record.price_raw.as_deref().and_then(parse_probability))
            .or_else(|| market_decimal_mid(record.best_bid.as_deref(), record.best_ask.as_deref()))
            .or_else(|| record.best_ask.as_deref().and_then(parse_probability))
            .or_else(|| record.best_bid.as_deref().and_then(parse_probability))
            .map(float_to_string),
    }
}

fn team_slot_for_record(record: &CsvProviderRecord, snapshot: &CompactMarketSnapshot) -> TeamSlot {
    let (team_a, team_b) = teams_from_event_name(&record.market_key.event_name);
    let outcome = slugify(&record.outcome_name, 96);
    let side = slugify(&record.side, 96);
    if team_slug_matches(&outcome, team_a.as_deref()) || team_slug_matches(&side, team_a.as_deref())
    {
        return TeamSlot::TeamA;
    }
    if team_slug_matches(&outcome, team_b.as_deref()) || team_slug_matches(&side, team_b.as_deref())
    {
        return TeamSlot::TeamB;
    }
    if snapshot.team_a_polymarket_format.is_none() {
        TeamSlot::TeamA
    } else {
        TeamSlot::TeamB
    }
}

fn teams_from_event_name(event_name: &str) -> (Option<String>, Option<String>) {
    let normalized = event_name
        .replace(" at ", " vs ")
        .replace(" @ ", " vs ")
        .replace(" v. ", " vs ")
        .replace(" - ", " vs ");
    for delimiter in [" vs ", "_vs_"] {
        if let Some((left, right)) = normalized.split_once(delimiter) {
            return (Some(slugify(left, 96)), Some(slugify(right, 96)));
        }
    }
    (None, None)
}

fn team_slug_matches(value: &str, team: Option<&str>) -> bool {
    let Some(team) = team else {
        return false;
    };
    !team.is_empty() && (value.contains(team) || team.contains(value))
}

fn complement_probability(value: &str) -> Option<String> {
    let parsed = parse_probability(value)?;
    Some(float_to_string(1.0 - parsed))
}

fn provider_row(
    record: &CsvProviderRecord,
    snapshot: &CompactMarketSnapshot,
) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    row.insert(
        "data_generated_at".to_string(),
        record.provider_generated_at.clone().unwrap_or_default(),
    );
    row.insert("data_fetched_at".to_string(), record.fetched_at.clone());
    row.insert("bookmaker".to_string(), record_bookmaker_label(record));
    row.insert(
        "affiliate_id".to_string(),
        record.affiliate_id.clone().unwrap_or_default(),
    );
    row.insert(
        "team_a_polymarket_format".to_string(),
        snapshot
            .team_a_polymarket_format
            .clone()
            .unwrap_or_default(),
    );
    row.insert(
        "team_b_polymarket_format".to_string(),
        snapshot
            .team_b_polymarket_format
            .clone()
            .unwrap_or_default(),
    );
    row
}

fn record_bookmaker_dir(record: &CsvProviderRecord) -> String {
    match record.provider {
        CsvProvider::TheRundown => record
            .affiliate_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                therundown_affiliate_name(value)
                    .map(|name| slugify(name, 32))
                    .unwrap_or_else(|| format!("affiliate_{}", slugify(value, 32)))
            })
            .unwrap_or_else(|| "affiliate_unknown".to_string()),
        CsvProvider::Polymarket => "polymarket".to_string(),
    }
}

fn record_bookmaker_label(record: &CsvProviderRecord) -> String {
    match record.provider {
        CsvProvider::TheRundown => record
            .affiliate_id
            .as_deref()
            .and_then(therundown_affiliate_name)
            .map(str::to_string)
            .or_else(|| {
                record
                    .affiliate_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| format!("Affiliate {value}"))
            })
            .unwrap_or_else(|| "Unknown Affiliate".to_string()),
        CsvProvider::Polymarket => "Polymarket".to_string(),
    }
}

fn therundown_affiliate_name(affiliate_id: &str) -> Option<&'static str> {
    match affiliate_id.trim() {
        "2" => Some("Bovada"),
        "3" => Some("Pinnacle"),
        "4" => Some("Sportsbetting"),
        "6" => Some("BetOnline"),
        "11" => Some("LowVig"),
        "12" => Some("Bodog"),
        "14" => Some("Intertops"),
        "16" => Some("Matchbook"),
        "18" => Some("YouWager"),
        "19" => Some("DraftKings"),
        "21" => Some("Unibet"),
        "22" => Some("BetMGM"),
        "23" => Some("FanDuel"),
        "24" => Some("theScore Bet"),
        "25" => Some("Kalshi"),
        "26" => Some("Polymarket"),
        _ => None,
    }
}

pub fn american_odds_to_implied_probability(value: &str) -> Option<f64> {
    let odds = value.trim().trim_start_matches('+').parse::<f64>().ok()?;
    if odds == 0.0001 {
        return None;
    }
    if odds > 0.0 {
        Some(100.0 / (odds + 100.0))
    } else if odds < 0.0 {
        let abs = odds.abs();
        Some(abs / (abs + 100.0))
    } else {
        None
    }
}

pub fn market_decimal_mid(best_bid: Option<&str>, best_ask: Option<&str>) -> Option<f64> {
    let bid = best_bid.and_then(parse_probability)?;
    let ask = best_ask.and_then(parse_probability)?;
    Some((bid + ask) / 2.0)
}

fn append_csv_row(path: &Path, headers: &[&str], row: &BTreeMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| LocalCsvError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let needs_header = match fs::metadata(path) {
        Ok(metadata) => metadata.len() == 0,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(source) => {
            return Err(LocalCsvError::Io {
                path: path.display().to_string(),
                source,
            })
        }
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| LocalCsvError::Io {
            path: path.display().to_string(),
            source,
        })?;
    if needs_header {
        writeln!(file, "{}", headers.join(",")).map_err(|source| LocalCsvError::Io {
            path: path.display().to_string(),
            source,
        })?;
    }
    let values = headers
        .iter()
        .map(|header| {
            csv_escape(&redact_sensitive(
                row.get(*header).map(String::as_str).unwrap_or(""),
            ))
        })
        .collect::<Vec<_>>()
        .join(",");
    writeln!(file, "{values}").map_err(|source| LocalCsvError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn write_json_file<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| LocalCsvError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| LocalCsvError::Json {
        path: path.display().to_string(),
        source,
    })?;
    let tmp_path = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    {
        let mut file = File::create(&tmp_path).map_err(|source| LocalCsvError::Io {
            path: tmp_path.display().to_string(),
            source,
        })?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|source| LocalCsvError::Io {
                path: tmp_path.display().to_string(),
                source,
            })?;
    }
    fs::rename(&tmp_path, path).map_err(|source| LocalCsvError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn read_json_file<T>(path: &Path) -> Result<Option<T>>
where
    T: for<'de> Deserialize<'de>,
{
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.is_empty() {
                return Ok(None);
            }
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| LocalCsvError::Json {
                    path: path.display().to_string(),
                    source,
                })
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LocalCsvError::Io {
            path: path.display().to_string(),
            source,
        }),
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn slugify(input: &str, max_len: usize) -> String {
    if contains_sensitive_label(input) {
        return "redacted".to_string();
    }
    let mut output = String::new();
    let mut last_was_separator = false;
    for ch in input.to_ascii_lowercase().chars() {
        let next = if ch.is_ascii_alphanumeric() || ch == '-' {
            Some(ch)
        } else if ch == '_'
            || ch.is_ascii_whitespace()
            || matches!(
                ch,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '.'
            )
        {
            Some('_')
        } else {
            None
        };
        match next {
            Some('_') if !last_was_separator => {
                output.push('_');
                last_was_separator = true;
            }
            Some('_') => {}
            Some(ch) => {
                output.push(ch);
                last_was_separator = false;
            }
            None => {}
        }
    }
    let output = output.trim_matches('_').to_string();
    let output = if output.is_empty() {
        "unknown".to_string()
    } else {
        output
    };
    if output.len() <= max_len {
        output
    } else {
        let hash = short_hash(&output);
        let keep = max_len.saturating_sub(hash.len() + 2).max(16);
        format!(
            "{}_h{}",
            output
                .chars()
                .take(keep)
                .collect::<String>()
                .trim_end_matches('_'),
            hash
        )
    }
}

fn start_time_file_component(value: &str) -> String {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return parsed
            .with_timezone(&Utc)
            .format("%Y-%m-%dT%H%M%SZ")
            .to_string();
    }
    slugify(&value.replace(':', ""), 48)
}

fn line_from_value(value: Option<&Value>) -> MarketLine {
    match value {
        Some(Value::Number(number)) => number
            .as_f64()
            .map(MarketLine::Point)
            .unwrap_or_else(|| MarketLine::Raw(number.to_string())),
        Some(Value::String(value)) if value.trim().is_empty() => MarketLine::NoLine,
        Some(Value::String(value)) => value
            .trim()
            .parse::<f64>()
            .map(MarketLine::Point)
            .unwrap_or_else(|_| MarketLine::Raw(value.clone())),
        Some(Value::Null) | None => MarketLine::NoLine,
        Some(other) => MarketLine::Raw(other.to_string()),
    }
}

fn line_value_to_file_component(value: f64) -> String {
    let is_negative = value.is_sign_negative();
    let value = trim_float(value.abs()).replace('.', "_");
    if value == "0" {
        "0".to_string()
    } else if is_negative {
        format!("minus_{value}")
    } else {
        value
    }
}

fn trim_float(value: f64) -> String {
    let mut text = format!("{value:.6}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(value_to_string))
}

fn team_name_from_value(value: &Value) -> Option<String> {
    value
        .get("name")
        .and_then(value_to_string)
        .map(|name| {
            let mascot = value
                .get("mascot")
                .and_then(value_to_string)
                .unwrap_or_default();
            if mascot.trim().is_empty()
                || name
                    .to_ascii_lowercase()
                    .contains(&mascot.to_ascii_lowercase())
            {
                name
            } else {
                format!("{name} {mascot}")
            }
        })
        .or_else(|| value.as_str().map(str::to_string))
}

fn therundown_event_team_names(event: &Value) -> (Option<String>, Option<String>) {
    let away_from_legacy = event
        .get("away_team")
        .and_then(team_name_from_value)
        .filter(|value| !value.trim().is_empty());
    let home_from_legacy = event
        .get("home_team")
        .and_then(team_name_from_value)
        .filter(|value| !value.trim().is_empty());
    if away_from_legacy.is_some() || home_from_legacy.is_some() {
        return (away_from_legacy, home_from_legacy);
    }

    let mut away = None;
    let mut home = None;
    if let Some(teams) = event.get("teams").and_then(Value::as_array) {
        let away_id = event
            .pointer("/score/team_id_away")
            .and_then(value_to_string);
        let home_id = event
            .pointer("/score/team_id_home")
            .and_then(value_to_string);
        for team in teams {
            let name = team_name_from_value(team).filter(|value| !value.trim().is_empty());
            let team_id = string_field(team, &["team_id", "id"]);
            if team.get("is_away").and_then(Value::as_bool) == Some(true)
                || (away_id.is_some() && team_id == away_id)
            {
                away = name.clone();
            }
            if team.get("is_home").and_then(Value::as_bool) == Some(true)
                || (home_id.is_some() && team_id == home_id)
            {
                home = name;
            }
        }
    }
    (away, home)
}

fn participant_outcome_name(
    participant: &Value,
    home_name: Option<&str>,
    away_name: Option<&str>,
) -> Option<String> {
    participant
        .get("name")
        .and_then(value_to_string)
        .or_else(|| match participant.get("side").and_then(Value::as_str) {
            Some("home") => home_name.map(str::to_string),
            Some("away") => away_name.map(str::to_string),
            _ => None,
        })
        .or_else(|| {
            participant
                .get("normalized_market_participant_id")
                .and_then(value_to_string)
        })
        .or_else(|| {
            participant
                .get("market_participant_id")
                .and_then(value_to_string)
        })
}

fn value_to_iso_time(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => {
            if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
                Some(iso_time(parsed.with_timezone(&Utc)))
            } else if let Ok(number) = value.parse::<i64>() {
                unix_to_iso(number)
            } else {
                None
            }
        }
        Value::Number(number) => number.as_i64().and_then(unix_to_iso),
        _ => None,
    }
}

fn unix_to_iso(value: i64) -> Option<String> {
    let seconds = if value > 10_000_000_000 {
        value / 1_000
    } else {
        value
    };
    Utc.timestamp_opt(seconds, 0).single().map(iso_time)
}

fn iso_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn therundown_market_type(market_id: Option<&str>) -> &'static str {
    match market_id.and_then(|value| value.parse::<i64>().ok()) {
        Some(1 | 41) => "moneyline",
        Some(2 | 42) => "spread",
        Some(3 | 43) => "total",
        _ => "unknown",
    }
}

fn polymarket_market_type(condition_id: Option<&str>, event_type: Option<&str>) -> &'static str {
    let text = format!(
        "{} {}",
        condition_id.unwrap_or_default().to_ascii_lowercase(),
        event_type.unwrap_or_default().to_ascii_lowercase()
    );
    if text.contains("spread") {
        "spread"
    } else if text.contains("total") || text.contains("over_under") {
        "total"
    } else if text.contains("moneyline") || text.contains("_ml") {
        "moneyline"
    } else {
        "unknown"
    }
}

fn therundown_sport_and_league(sport_id: Option<&str>) -> (&'static str, &'static str) {
    match sport_id.and_then(|value| value.parse::<u32>().ok()) {
        Some(1) => ("nfl", "nfl"),
        Some(3) => ("mlb", "mlb"),
        Some(4) => ("nba", "nba"),
        Some(5) => ("nhl", "nhl"),
        _ => ("unknown_sport", "unknown_league"),
    }
}

fn infer_sport_from_text(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    for sport in ["nba", "nfl", "mlb", "nhl", "atp", "wta", "tennis"] {
        if lower.contains(sport) {
            return sport;
        }
    }
    "unknown_sport"
}

fn is_off_board_price(value: &Value) -> bool {
    value_to_string(value).is_some_and(|value| value.trim() == "0.0001")
}

fn top_price(payload: &Value, side: &str) -> Option<String> {
    payload
        .get(side)
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("price"))
        .and_then(value_to_string)
}

fn book_depth(payload: &Value) -> Option<usize> {
    let bids = payload.get("bids").and_then(Value::as_array).map(Vec::len);
    let asks = payload.get("asks").and_then(Value::as_array).map(Vec::len);
    match (bids, asks) {
        (Some(bids), Some(asks)) => Some(bids + asks),
        (Some(depth), None) | (None, Some(depth)) => Some(depth),
        (None, None) => None,
    }
}

fn parse_probability(value: &str) -> Option<f64> {
    let parsed = value.trim().parse::<f64>().ok()?;
    if (0.0..=1.0).contains(&parsed) {
        Some(parsed)
    } else {
        None
    }
}

fn float_to_string(value: f64) -> String {
    trim_float(value)
}

fn deterministic_row_id(
    provider: &str,
    key: &MarketFileKey,
    payload_hash: &str,
    suffix: &str,
) -> String {
    format!(
        "{}:{}:{}",
        provider,
        key.market_key(),
        short_hash(&format!("{payload_hash}:{suffix}"))
    )
}

fn short_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    to_hex(&hasher.finalize())[..10].to_string()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn redact_sensitive(value: &str) -> String {
    if contains_sensitive_label(value) {
        "<redacted>".to_string()
    } else {
        value.to_string()
    }
}

fn contains_sensitive_label(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("secret")
        || lower.contains("passphrase")
        || lower.contains("private_key")
        || lower.contains("authorization")
        || lower.contains("bearer ")
}
