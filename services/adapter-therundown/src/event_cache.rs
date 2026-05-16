use quantsys_domain::RawMessage;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TheRundownEventCache {
    events: BTreeMap<String, TheRundownEventMetadata>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TheRundownEventMetadata {
    sport: String,
    league: String,
    event_name: String,
    event_start_time_utc: Option<String>,
    outcomes_by_participant: BTreeMap<String, String>,
}

impl TheRundownEventCache {
    pub fn upsert_bootstrap_payload(&mut self, payload: &Value) {
        let Some(events) = payload
            .get("events")
            .and_then(Value::as_array)
            .or_else(|| payload.get("data").and_then(Value::as_array))
        else {
            return;
        };

        for event in events {
            let Some(event_id) = string_field(event, &["event_id", "id"]) else {
                continue;
            };
            let sport_id = string_field(event, &["sport_id"]);
            let (sport, league) = sport_and_league(sport_id.as_deref());
            let (away, home) = event_team_infos(event);
            let event_name = match (&away.name, &home.name) {
                (Some(away), Some(home)) => format!("{away} vs {home}"),
                _ => string_field(event, &["event_name", "name", "title"])
                    .unwrap_or_else(|| event_id.clone()),
            };
            let event_start_time_utc =
                string_field(event, &["event_date", "start_time", "start_date"]);
            let mut outcomes_by_participant = BTreeMap::new();

            insert_team_ids(&mut outcomes_by_participant, &home);
            insert_team_ids(&mut outcomes_by_participant, &away);
            if let Some(markets) = event.get("markets").and_then(Value::as_array) {
                for market in markets {
                    if let Some(participants) = market.get("participants").and_then(Value::as_array)
                    {
                        for participant in participants {
                            let team_name = participant
                                .get("name")
                                .and_then(value_to_string)
                                .or_else(|| {
                                    match participant.get("side").and_then(Value::as_str) {
                                        Some("home") => home.name.clone(),
                                        Some("away") => away.name.clone(),
                                        _ => participant
                                            .get("normalized_market_participant_id")
                                            .and_then(value_to_string)
                                            .and_then(|id| {
                                                outcomes_by_participant.get(&id).cloned()
                                            }),
                                    }
                                });
                            if let Some(team_name) = team_name {
                                for field in [
                                    "id",
                                    "market_participant_id",
                                    "normalized_market_participant_id",
                                ] {
                                    if let Some(id) =
                                        participant.get(field).and_then(value_to_string)
                                    {
                                        outcomes_by_participant.insert(id, team_name.clone());
                                    }
                                }
                                if let Some(lines) =
                                    participant.get("lines").and_then(Value::as_array)
                                {
                                    for line in lines {
                                        if let Some(prices) =
                                            line.get("prices").and_then(Value::as_object)
                                        {
                                            for price in prices.values() {
                                                if let Some(id) =
                                                    price.get("id").and_then(value_to_string)
                                                {
                                                    outcomes_by_participant
                                                        .insert(id, team_name.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.events.insert(
                event_id,
                TheRundownEventMetadata {
                    sport: sport.to_string(),
                    league: league.to_string(),
                    event_name,
                    event_start_time_utc,
                    outcomes_by_participant,
                },
            );
        }
    }

    pub fn enrich_raw_for_local_csv(&self, raw: &RawMessage) -> RawMessage {
        if raw.payload.pointer("/meta/type").and_then(Value::as_str) != Some("market_price") {
            return raw.clone();
        }
        let Some(event_id) = raw
            .payload
            .pointer("/data/event_id")
            .and_then(value_to_string)
        else {
            return raw.clone();
        };
        let Some(metadata) = self.events.get(&event_id) else {
            return raw.clone();
        };

        let mut enriched = raw.clone();
        let outcomes_by_participant = metadata
            .outcomes_by_participant
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect::<Map<_, _>>();
        let line = enriched
            .payload
            .pointer("/data/line")
            .cloned()
            .unwrap_or(Value::Null);
        let market_type = enriched
            .payload
            .pointer("/data/market_id")
            .and_then(value_to_string)
            .map(|value| market_type(&value))
            .unwrap_or("unknown");

        enriched.payload["_local_csv"] = json!({
            "sport": metadata.sport,
            "league": metadata.league,
            "event_name": metadata.event_name,
            "event_start_time_utc": metadata.event_start_time_utc,
            "market_type": market_type,
            "line": line,
            "event_id": event_id,
            "outcomes_by_participant": outcomes_by_participant,
        });
        enriched
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct TeamInfo {
    name: Option<String>,
    ids: Vec<String>,
}

fn event_team_infos(event: &Value) -> (TeamInfo, TeamInfo) {
    let legacy_home = team_info(event.get("home_team"));
    let legacy_away = team_info(event.get("away_team"));
    if legacy_home.name.is_some() || legacy_away.name.is_some() {
        return (legacy_away, legacy_home);
    }

    let mut away = TeamInfo::default();
    let mut home = TeamInfo::default();
    if let Some(teams) = event.get("teams").and_then(Value::as_array) {
        let away_id = event
            .pointer("/score/team_id_away")
            .and_then(value_to_string);
        let home_id = event
            .pointer("/score/team_id_home")
            .and_then(value_to_string);
        for team in teams {
            let info = team_info(Some(team));
            let team_id = string_field(team, &["team_id", "id"]);
            if team.get("is_away").and_then(Value::as_bool) == Some(true)
                || (away_id.is_some() && team_id == away_id)
            {
                away = info.clone();
            }
            if team.get("is_home").and_then(Value::as_bool) == Some(true)
                || (home_id.is_some() && team_id == home_id)
            {
                home = info;
            }
        }
    }
    (away, home)
}

fn team_info(value: Option<&Value>) -> TeamInfo {
    let Some(value) = value else {
        return TeamInfo::default();
    };
    let mut ids = Vec::new();
    for field in ["team_id", "normalized_team_id", "id"] {
        if let Some(id) = value.get(field).and_then(value_to_string) {
            ids.push(id);
        }
    }
    TeamInfo {
        name: value
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
            .or_else(|| value.as_str().map(str::to_string)),
        ids,
    }
}

fn insert_team_ids(outcomes: &mut BTreeMap<String, String>, team: &TeamInfo) {
    let Some(name) = team.name.as_ref() else {
        return;
    };
    for id in &team.ids {
        outcomes.insert(id.clone(), name.clone());
    }
}

fn sport_and_league(sport_id: Option<&str>) -> (&'static str, &'static str) {
    match sport_id.and_then(|value| value.parse::<u32>().ok()) {
        Some(1) => ("nfl", "nfl"),
        Some(3) => ("mlb", "mlb"),
        Some(4) => ("nba", "nba"),
        Some(5) => ("nhl", "nhl"),
        _ => ("unknown_sport", "unknown_league"),
    }
}

fn market_type(market_id: &str) -> &'static str {
    match market_id.parse::<i64>().ok() {
        Some(1 | 41) => "moneyline",
        Some(2 | 42) => "spread",
        Some(3 | 43) => "total",
        _ => "unknown",
    }
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(value_to_string))
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}
