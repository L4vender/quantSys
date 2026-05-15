#!/usr/bin/env python3
"""Live TheRundown <-> Polymarket event mapping.

This script performs public/entitled market discovery only. It never calls
Polymarket order, cancel, signing, or private-key paths.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass, field
from datetime import datetime, timedelta, timezone
from difflib import SequenceMatcher
from pathlib import Path
from typing import Any, Iterable

try:
    from zoneinfo import ZoneInfo
except Exception:  # pragma: no cover - Python without zoneinfo.
    ZoneInfo = None  # type: ignore


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT_DIR = ROOT / "output/live-mapping"
DEFAULT_ALIAS_PATH = ROOT / "configs/mapping/team_aliases.yaml"

EXIT_OK = 0
EXIT_CONFIG_MISSING = 2
EXIT_NETWORK_FAILURE = 3
EXIT_API_PERMISSION = 4
EXIT_DATA_EMPTY = 5
EXIT_RUNTIME_ERROR = 6

THERUNDOWN_SPORT_IDS = {
    "nfl": 2,
    "mlb": 3,
    "nba": 4,
    "nhl": 6,
}

THERUNDOWN_SPORT_LEAGUES = {
    2: ("nfl", "nfl"),
    3: ("mlb", "mlb"),
    4: ("nba", "nba"),
    6: ("nhl", "nhl"),
}

COMMON_POLYMARKET_SPORT_TAGS = {"1", "100639"}
GENERIC_YES_NO = {"yes", "no"}
HARD_ORDER_PATH_RE = re.compile(r"(?i)(/orders?|/cancel|/sign|/auth/api-key|/derive|/create-order)")
PUNCT_RE = re.compile(r"[^a-z0-9\s]")
SPACE_RE = re.compile(r"\s+")


class LiveMappingError(Exception):
    exit_code = EXIT_RUNTIME_ERROR


class ConfigMissingError(LiveMappingError):
    exit_code = EXIT_CONFIG_MISSING


class NetworkFailureError(LiveMappingError):
    exit_code = EXIT_NETWORK_FAILURE


class ApiPermissionError(LiveMappingError):
    exit_code = EXIT_API_PERMISSION


class DataEmptyError(LiveMappingError):
    exit_code = EXIT_DATA_EMPTY


class SecurityError(LiveMappingError):
    exit_code = EXIT_RUNTIME_ERROR


@dataclass
class ProviderEvent:
    provider: str
    provider_event_id: str
    sport: str
    league: str
    event_name: str
    start_time_utc: str | None
    home_team_raw: str | None
    away_team_raw: str | None
    participants_raw: list[str]
    market_type_raw: str | None
    outcome_names_raw: list[str]
    source_timestamp: str | None
    received_at: str
    raw_ref: str
    normalization_steps: list[str] = field(default_factory=list)


@dataclass
class ProviderMarket:
    provider: str
    provider_event_id: str
    event_slug: str | None
    condition_id: str | None
    market_id: str | None
    token_ids: list[str]
    sport: str
    league: str
    event_title: str
    market_title: str
    start_time_utc: str | None
    participants_raw: list[str]
    outcome_names_raw: list[str]
    active: bool
    closed: bool
    raw_ref: str
    received_at: str
    home_team_raw: str | None = None
    away_team_raw: str | None = None
    home_away_status: str = "unknown"
    market_type_raw: str | None = None
    period: str = "full_game"
    normalization_steps: list[str] = field(default_factory=list)


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def isoformat_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_bool(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    text = str(value).strip().lower()
    if text in {"1", "true", "yes", "y", "on"}:
        return True
    if text in {"0", "false", "no", "n", "off"}:
        return False
    raise argparse.ArgumentTypeError(f"invalid boolean value: {value}")


def parse_iso_datetime(value: Any) -> datetime | None:
    if value is None:
        return None
    text = str(value).strip()
    if not text:
        return None
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(text).astimezone(timezone.utc)
    except ValueError:
        return None


def stable_hash(value: Any) -> str:
    payload = json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":"))
    return hashlib.sha256(payload.encode()).hexdigest()[:16]


def raw_ref(provider: str, channel: str, payload: Any) -> str:
    return f"raw:{provider}:{channel}:{stable_hash(payload)}"


def load_dotenv(path: Path = ROOT / ".env") -> None:
    if not path.exists():
        return
    for line in path.read_text().splitlines():
        text = line.strip()
        if not text or text.startswith("#") or "=" not in text:
            continue
        key, value = text.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def read_simple_toml(path: Path) -> dict[str, Any]:
    data: dict[str, Any] = {}
    if not path.exists():
        return data
    current_prefix: list[str] = []
    for raw_line in path.read_text().splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            current_prefix = line[1:-1].split(".")
            continue
        if "=" not in line:
            continue
        key, value = [part.strip() for part in line.split("=", 1)]
        parsed: Any
        if value.startswith('"') and value.endswith('"'):
            parsed = value[1:-1]
        elif value.lower() in {"true", "false"}:
            parsed = value.lower() == "true"
        elif value.startswith("[") and value.endswith("]"):
            body = value[1:-1].strip()
            if not body:
                parsed = []
            else:
                parsed = [
                    item.strip().strip('"').strip("'")
                    for item in body.split(",")
                    if item.strip()
                ]
                parsed = [int(item) if re.fullmatch(r"-?\d+", str(item)) else item for item in parsed]
        elif re.fullmatch(r"-?\d+", value):
            parsed = int(value)
        else:
            parsed = value
        target = data
        for prefix in current_prefix:
            target = target.setdefault(prefix, {})
        target[key] = parsed
    return data


def load_aliases(path: Path = DEFAULT_ALIAS_PATH) -> dict[str, dict[str, str]]:
    aliases: dict[str, dict[str, str]] = {}
    if not path.exists():
        return aliases
    current_sport: str | None = None
    for raw_line in path.read_text().splitlines():
        if not raw_line.strip() or raw_line.lstrip().startswith("#") or raw_line.strip() == "aliases:":
            continue
        if raw_line.startswith("  ") and not raw_line.startswith("    "):
            current_sport = raw_line.strip().rstrip(":").lower()
            aliases.setdefault(current_sport, {})
            continue
        if current_sport and raw_line.startswith("    ") and ":" in raw_line:
            key, raw_values = raw_line.strip().split(":", 1)
            canonical = normalize_basic(key)
            aliases[current_sport][canonical] = canonical
            values = raw_values.strip()
            if values.startswith("[") and values.endswith("]"):
                values = values[1:-1]
            for value in [item.strip().strip('"').strip("'") for item in values.split(",") if item.strip()]:
                aliases[current_sport][normalize_basic(value)] = canonical
    return aliases


def normalize_basic(name: str) -> str:
    text = name.lower().strip()
    text = text.replace("&", " and ")
    text = PUNCT_RE.sub(" ", text)
    return SPACE_RE.sub(" ", text).strip()


def normalize_name(name: str | None, sport: str | None, aliases: dict[str, dict[str, str]]) -> tuple[str, list[str]]:
    if not name:
        return "", ["missing"]
    steps: list[str] = []
    basic = normalize_basic(name)
    steps.append(f"basic:{basic}")
    sport_key = (sport or "").lower()
    sport_aliases = aliases.get(sport_key, {})
    if sport_key in {"atp", "wta"}:
        sport_aliases = {**aliases.get("tennis", {}), **sport_aliases}
    if basic in sport_aliases:
        canonical = sport_aliases[basic]
        steps.append(f"alias:{basic}->{canonical}")
        return canonical, steps
    tokens = basic.split()
    if sport_key in {"atp", "wta", "tennis"} and len(tokens) == 2:
        reversed_name = " ".join(reversed(tokens))
        steps.append(f"tennis_name_variant:{reversed_name}")
    return basic, steps


def token_similarity(a: str, b: str) -> float:
    if not a or not b:
        return 0.0
    a_tokens = set(a.split())
    b_tokens = set(b.split())
    overlap = len(a_tokens & b_tokens) / max(1, len(a_tokens | b_tokens))
    seq = SequenceMatcher(None, a, b).ratio()
    sorted_seq = SequenceMatcher(None, " ".join(sorted(a_tokens)), " ".join(sorted(b_tokens))).ratio()
    return max(seq, sorted_seq, overlap)


def name_similarity(a: str | None, b: str | None, sport: str | None, aliases: dict[str, dict[str, str]]) -> float:
    norm_a, _ = normalize_name(a, sport, aliases)
    norm_b, _ = normalize_name(b, sport, aliases)
    score = token_similarity(norm_a, norm_b)
    if sport in {"atp", "wta", "tennis"}:
        rev_a = " ".join(reversed(norm_a.split()))
        rev_b = " ".join(reversed(norm_b.split()))
        score = max(score, token_similarity(rev_a, norm_b), token_similarity(norm_a, rev_b))
    return round(score, 4)


def parse_jsonish(value: Any) -> list[Any]:
    if value is None:
        return []
    if isinstance(value, list):
        return value
    if isinstance(value, tuple):
        return list(value)
    if isinstance(value, str):
        text = value.strip()
        if not text:
            return []
        try:
            loaded = json.loads(text)
            return loaded if isinstance(loaded, list) else [loaded]
        except json.JSONDecodeError:
            return [item.strip() for item in text.split(",") if item.strip()]
    return [value]


def combine_team_name(team: dict[str, Any]) -> str:
    name = str(team.get("name") or "").strip()
    mascot = str(team.get("mascot") or "").strip()
    if not mascot or mascot.lower() == name.lower():
        return name
    if name.lower() == "tbd" or mascot.lower() == "tbd":
        return "TBD"
    return f"{name} {mascot}".strip()


def normalize_market_type(raw: str | None, outcomes: list[str] | None, context: str | None = None) -> tuple[str, str, list[str]]:
    text = normalize_basic(" ".join([raw or "", context or ""]))
    outcome_norm = [normalize_basic(item) for item in (outcomes or [])]
    reasons: list[str] = []
    if "series" in text:
        return "series", "series", ["series_keyword"]
    if any(word in text for word in ["champion", "mvp", "award", "trophy", "winner", "retire", "cba"]):
        return "future_or_prop", "unknown", ["future_or_prop_keyword"]
    if "spread" in text:
        return "spread", "full_game", ["spread_keyword"]
    if (
        "total" in text
        or "over under" in text
        or " o u " in f" {text} "
        or set(outcome_norm) == {"over", "under"}
    ):
        return "total", "full_game", ["total_keyword"]
    if "moneyline" in text:
        return "moneyline", "full_game", ["moneyline_keyword"]
    if len(outcome_norm) == 2 and set(outcome_norm) != GENERIC_YES_NO and (
        " vs " in f" {text} " or " upcoming " in f" {text} " or " game " in f" {text} "
    ):
        return "moneyline", "full_game", ["two_team_game_outcomes"]
    if set(outcome_norm) == GENERIC_YES_NO:
        return "binary_prop", "unknown", ["yes_no_outcomes"]
    reasons.append("unknown_market_type")
    return "unknown", "unknown", reasons


def source_timestamp_from_event(event: dict[str, Any]) -> str | None:
    score = event.get("score") or {}
    return score.get("updated_at") or event.get("updatedAt") or event.get("event_date")


def parse_therundown_events(
    payload: dict[str, Any],
    sport: str,
    received_at: str,
    raw_ref: str,
    now: datetime,
    lookahead_hours: int,
) -> list[ProviderEvent]:
    results: list[ProviderEvent] = []
    deadline = now + timedelta(hours=lookahead_hours)
    for event in payload.get("events") or []:
        start_dt = parse_iso_datetime(event.get("event_date"))
        if start_dt and not (now - timedelta(hours=6) <= start_dt <= deadline):
            continue
        markets = event.get("markets") or []
        moneyline_markets = [
            market for market in markets
            if str(market.get("market_id")) == "1" and str(market.get("period_id", "0")) == "0"
        ]
        if not moneyline_markets:
            continue
        market = moneyline_markets[0]
        teams = event.get("teams") or []
        home_team = next((team for team in teams if team.get("is_home") is True), None)
        away_team = next((team for team in teams if team.get("is_away") is True), None)
        home_name = combine_team_name(home_team or {}) if home_team else None
        away_name = combine_team_name(away_team or {}) if away_team else None
        participants = [
            str(participant.get("name")).strip()
            for participant in market.get("participants") or []
            if participant.get("name")
        ]
        if not participants:
            participants = [name for name in [away_name, home_name] if name]
        sport_id = int(event.get("sport_id") or 0)
        sport_name, league = THERUNDOWN_SPORT_LEAGUES.get(sport_id, (sport.lower(), sport.lower()))
        schedule = event.get("schedule") or {}
        event_name = schedule.get("event_name") or " vs ".join(participants) or event.get("event_id")
        results.append(ProviderEvent(
            provider="therundown",
            provider_event_id=str(event.get("event_id") or event.get("event_uuid") or stable_hash(event)),
            sport=sport_name,
            league=league,
            event_name=str(event_name),
            start_time_utc=isoformat_utc(start_dt) if start_dt else None,
            home_team_raw=home_name,
            away_team_raw=away_name,
            participants_raw=participants,
            market_type_raw=str(market.get("name") or "moneyline"),
            outcome_names_raw=participants,
            source_timestamp=source_timestamp_from_event(event),
            received_at=received_at,
            raw_ref=raw_ref,
            normalization_steps=["therundown_v2_events", "market_id:1", "period_id:0"],
        ))
    return results


def infer_sport_from_text(text: str, default: str | None = None) -> str:
    norm = normalize_basic(text)
    for sport in ["nba", "nfl", "mlb", "nhl", "atp", "wta"]:
        if sport in norm.split() or f"{sport} " in norm:
            return sport
    if "basketball" in norm:
        return "nba"
    if "baseball" in norm or "world series" in norm:
        return "mlb"
    if "hockey" in norm or "stanley cup" in norm:
        return "nhl"
    if "tennis" in norm or "wimbledon" in norm or "french open" in norm:
        return default or "tennis"
    return default or "unknown"


def extract_vs_participants(*texts: str | None) -> list[str]:
    for text in texts:
        if not text:
            continue
        cleaned = re.sub(r"\s+", " ", str(text)).strip()
        match = re.search(r"(.+?)\s+(?:vs\.?|v\.?|versus)\s+(.+?)(?:$|,|\?| - )", cleaned, re.IGNORECASE)
        if match:
            left = match.group(1).split(":")[-1].strip(" -")
            right = match.group(2).strip(" -")
            if left and right:
                return [left, right]
    return []


def extract_slug_date(slug: str | None) -> str | None:
    if not slug:
        return None
    match = re.search(r"(20\d{2})-(\d{2})-(\d{2})", slug)
    if match:
        return "-".join(match.groups())
    return None


def parse_et_schedule(description: str | None, slug: str | None) -> datetime | None:
    if not description:
        return None
    match = re.search(
        r"scheduled for ([A-Za-z]+)\s+(\d{1,2})(?:,\s*(20\d{2}))?\s+at\s+(\d{1,2}):(\d{2})\s*(AM|PM)\s*ET",
        description,
        re.IGNORECASE,
    )
    if not match:
        return None
    month_name, day, year, hour, minute, ampm = match.groups()
    slug_date = extract_slug_date(slug)
    if not year and slug_date:
        year = slug_date[:4]
    if not year:
        year = str(utc_now().year)
    try:
        month = datetime.strptime(month_name[:3].title(), "%b").month
        hour_int = int(hour) % 12
        if ampm.lower() == "pm":
            hour_int += 12
        if ZoneInfo:
            eastern = ZoneInfo("America/New_York")
            local = datetime(int(year), month, int(day), hour_int, int(minute), tzinfo=eastern)
            return local.astimezone(timezone.utc)
        offset = timezone(timedelta(hours=-4))
        local = datetime(int(year), month, int(day), hour_int, int(minute), tzinfo=offset)
        return local.astimezone(timezone.utc)
    except Exception:
        return None


def looks_like_game_market(event: dict[str, Any], market: dict[str, Any], outcomes: list[str]) -> bool:
    context = " ".join(str(value or "") for value in [
        event.get("title"),
        event.get("slug"),
        event.get("description"),
        market.get("question"),
        market.get("slug"),
    ])
    norm = normalize_basic(context)
    if "series" in norm:
        return False
    if " upcoming " in f" {norm} " and " game " in f" {norm} ":
        return True
    if extract_vs_participants(event.get("title"), market.get("question")) and set(map(normalize_basic, outcomes)) != GENERIC_YES_NO:
        return True
    return False


def extract_polymarket_start(event: dict[str, Any], market: dict[str, Any], outcomes: list[str]) -> str | None:
    scheduled = parse_et_schedule(
        str(event.get("description") or market.get("description") or ""),
        str(event.get("slug") or market.get("slug") or ""),
    )
    if scheduled:
        return isoformat_utc(scheduled)
    for key in ["gameStartTime", "game_start_time", "scheduledStartTime", "scheduled_start_time"]:
        parsed = parse_iso_datetime(event.get(key) or market.get(key))
        if parsed:
            return isoformat_utc(parsed)
    creation = parse_iso_datetime(event.get("creationDate") or market.get("creationDate"))
    if creation and looks_like_game_market(event, market, outcomes):
        return isoformat_utc(creation)
    start = parse_iso_datetime(event.get("startDate") or market.get("startDate"))
    if start and looks_like_game_market(event, market, outcomes):
        return isoformat_utc(start)
    return None


def parse_polymarket_events(
    events: list[dict[str, Any]],
    sport: str,
    received_at: str,
    raw_ref_prefix: str,
    now: datetime,
) -> list[ProviderMarket]:
    results: list[ProviderMarket] = []
    for event in events:
        event_active = bool(event.get("active", True))
        event_closed = bool(event.get("closed", False))
        event_slug = event.get("slug")
        for market in event.get("markets") or []:
            active = event_active and bool(market.get("active", True))
            closed = event_closed or bool(market.get("closed", False))
            outcomes = [str(item) for item in parse_jsonish(market.get("outcomes"))]
            token_ids = [str(item) for item in parse_jsonish(market.get("clobTokenIds") or market.get("clobTokenIDs"))]
            title = str(event.get("title") or market.get("question") or "")
            question = str(market.get("question") or title)
            participants = [item for item in outcomes if normalize_basic(item) not in GENERIC_YES_NO]
            if not participants:
                participants = extract_vs_participants(title, question, event.get("description"))
            market_type, period, reasons = normalize_market_type(question, outcomes, " ".join([title, str(event.get("description") or "")]))
            start_time = extract_polymarket_start(event, market, outcomes)
            explicit_home = market.get("homeTeam") or market.get("home_team") or event.get("homeTeam") or event.get("home_team")
            explicit_away = market.get("awayTeam") or market.get("away_team") or event.get("awayTeam") or event.get("away_team")
            home_away_status = "explicit" if explicit_home and explicit_away else "unknown"
            event_id = str(event.get("id") or event_slug or stable_hash(event))
            results.append(ProviderMarket(
                provider="polymarket",
                provider_event_id=event_id,
                event_slug=str(event_slug) if event_slug else None,
                condition_id=str(market.get("conditionId") or market.get("condition_id") or ""),
                market_id=str(market.get("id") or ""),
                token_ids=token_ids,
                sport=infer_sport_from_text(" ".join([title, question, str(event_slug or "")]), sport),
                league=sport,
                event_title=title,
                market_title=question,
                start_time_utc=start_time,
                participants_raw=participants,
                outcome_names_raw=outcomes,
                active=active,
                closed=closed,
                raw_ref=f"{raw_ref_prefix}:{event_id}:{market.get('id') or stable_hash(market)}",
                received_at=received_at,
                home_team_raw=str(explicit_home) if explicit_home else None,
                away_team_raw=str(explicit_away) if explicit_away else None,
                home_away_status=home_away_status,
                market_type_raw=market_type,
                period=period,
                normalization_steps=["gamma_events", *reasons],
            ))
    return results


def pair_participant_scores(tr_names: list[str], pm_names: list[str], sport: str, aliases: dict[str, dict[str, str]]) -> tuple[float, float, str]:
    if len(tr_names) < 2 or len(pm_names) < 2:
        best = max(
            [name_similarity(a, b, sport, aliases) for a in tr_names for b in pm_names] or [0.0]
        )
        return round(best * 0.7, 4), round(best * 0.7, 4), "unknown"
    aligned = (
        name_similarity(tr_names[0], pm_names[0], sport, aliases)
        + name_similarity(tr_names[1], pm_names[1], sport, aliases)
    ) / 2
    reversed_score = (
        name_similarity(tr_names[0], pm_names[1], sport, aliases)
        + name_similarity(tr_names[1], pm_names[0], sport, aliases)
    ) / 2
    if reversed_score > aligned + 0.05:
        return round(reversed_score, 4), round(reversed_score, 4), "reversed"
    return round(aligned, 4), round(aligned, 4), "aligned"


def home_away_status(tr: ProviderEvent, pm: ProviderMarket, aliases: dict[str, dict[str, str]]) -> str:
    if pm.home_away_status != "explicit" or not pm.home_team_raw or not pm.away_team_raw:
        return "unknown"
    home_home = name_similarity(tr.home_team_raw, pm.home_team_raw, tr.sport, aliases)
    away_away = name_similarity(tr.away_team_raw, pm.away_team_raw, tr.sport, aliases)
    home_away = name_similarity(tr.home_team_raw, pm.away_team_raw, tr.sport, aliases)
    away_home = name_similarity(tr.away_team_raw, pm.home_team_raw, tr.sport, aliases)
    if home_home >= 0.9 and away_away >= 0.9:
        return "aligned"
    if home_away >= 0.9 and away_home >= 0.9:
        return "reversed"
    return "mismatch"


def score_time(tr_start: str | None, pm_start: str | None) -> tuple[float, int | None]:
    tr_dt = parse_iso_datetime(tr_start)
    pm_dt = parse_iso_datetime(pm_start)
    if not tr_dt or not pm_dt:
        return 0.55, None
    delta = abs(int((tr_dt - pm_dt).total_seconds()))
    if delta <= 15 * 60:
        return 1.0, delta
    if delta <= 60 * 60:
        return 0.9, delta
    if delta <= 4 * 60 * 60:
        return 0.72, delta
    if delta <= 24 * 60 * 60:
        return 0.35, delta
    return 0.05, delta


def event_date_candidates(start: str | None, slug: str | None = None) -> set[str]:
    candidates: set[str] = set()
    dt = parse_iso_datetime(start)
    if dt:
        candidates.add(dt.date().isoformat())
        if ZoneInfo:
            candidates.add(dt.astimezone(ZoneInfo("America/New_York")).date().isoformat())
    slug_date = extract_slug_date(slug)
    if slug_date:
        candidates.add(slug_date)
    return candidates


def event_date_match_status(tr_start: str | None, pm_start: str | None, pm_slug: str | None = None) -> tuple[str, float]:
    tr_dt = parse_iso_datetime(tr_start)
    pm_dt = parse_iso_datetime(pm_start)
    if not tr_dt or not pm_dt:
        return "missing_date", 0.55
    if event_date_candidates(tr_start) & event_date_candidates(pm_start, pm_slug):
        return "same_date", 1.0
    return "different_date", 0.0


def canonical_name(name: str | None, sport: str, aliases: dict[str, dict[str, str]]) -> str:
    return normalize_name(name, sport, aliases)[0].replace(" ", "_") or "unknown"


def is_concrete_polymarket_game(pm: ProviderMarket) -> bool:
    context = normalize_basic(" ".join([pm.event_title, pm.market_title, pm.event_slug or ""]))
    if pm.market_type_raw != "moneyline":
        return False
    if len(pm.participants_raw) < 2:
        return False
    if any(keyword in context for keyword in [
        "who will win series",
        "series",
        "champion",
        "championship",
        "regular season",
        "postseason",
        "division title",
        "conference finals",
        "finals",
        "mvp",
        "award",
        "next team",
        "play for",
        "draft",
    ]):
        return False
    if extract_vs_participants(pm.event_title, pm.market_title):
        return True
    return " vs " in f" {context} "


def score_candidate(
    tr: ProviderEvent,
    pm: ProviderMarket,
    aliases: dict[str, dict[str, str]],
    now: datetime,
    min_confidence: float,
) -> dict[str, Any]:
    reject_reasons: list[str] = []
    review_reasons: list[str] = []
    tr_market_type, tr_period, _ = normalize_market_type(tr.market_type_raw, tr.outcome_names_raw, tr.event_name)
    pm_market_type, pm_period, _ = normalize_market_type(pm.market_type_raw, pm.outcome_names_raw, " ".join([pm.event_title, pm.market_title]))
    if tr.sport != pm.sport and not ({tr.sport, pm.sport} <= {"tennis", "atp", "wta"}):
        reject_reasons.append("sport_mismatch")
    if not pm.active or pm.closed:
        reject_reasons.append("polymarket_not_active_or_closed")
    time_score, delta_seconds = score_time(tr.start_time_utc, pm.start_time_utc)
    date_status, date_score = event_date_match_status(tr.start_time_utc, pm.start_time_utc, pm.event_slug)
    if date_status == "missing_date":
        review_reasons.append("missing_start_time")
    elif date_status == "different_date":
        reject_reasons.append("event_date_mismatch")
    participant_score, outcome_set_score, pair_status = pair_participant_scores(
        tr.outcome_names_raw or tr.participants_raw,
        pm.outcome_names_raw or pm.participants_raw,
        tr.sport,
        aliases,
    )
    if not pm.participants_raw or not tr.participants_raw:
        review_reasons.append("missing_participant")
        participant_score *= 0.7
    elif participant_score < 0.86:
        reject_reasons.append("team_name_mismatch")
    name_score = max(
        name_similarity(tr.event_name, pm.event_title, tr.sport, aliases),
        participant_score,
    )
    invariant = home_away_status(tr, pm, aliases)
    if invariant == "reversed":
        home_away_score = 0.35
    elif invariant == "unknown":
        home_away_score = 0.72
    elif invariant == "mismatch":
        home_away_score = 0.45
    else:
        home_away_score = 1.0
    league_score = 1.0 if tr.league == pm.league or tr.sport == pm.sport else 0.5
    market_type_score = 1.0 if tr_market_type == pm_market_type and tr_period == pm_period else 0.0
    confidence = 0.20 * date_score + 0.80 * participant_score
    confidence = round(max(0.0, min(1.0, confidence)), 4)
    if reject_reasons:
        decision = "rejected"
    elif review_reasons or confidence < min_confidence:
        decision = "needs_review" if confidence >= 0.80 else "rejected"
        if confidence < min_confidence and confidence >= 0.80:
            review_reasons.append("confidence_below_auto_match")
        elif confidence < 0.80:
            reject_reasons.append("confidence_below_reject_threshold")
    else:
        decision = "matched"
    normalized_home, home_steps = normalize_name(tr.home_team_raw, tr.sport, aliases)
    normalized_away, away_steps = normalize_name(tr.away_team_raw, tr.sport, aliases)
    canonical_event_id = f"{tr.sport}:{canonical_name(tr.away_team_raw, tr.sport, aliases)}_at_{canonical_name(tr.home_team_raw, tr.sport, aliases)}:{(tr.start_time_utc or 'unknown')[:10]}"
    output_market_type = pm_market_type if pm_market_type != "unknown" else tr_market_type
    output_period = pm_period if pm_period != "unknown" else tr_period
    mapping_id = f"map_{stable_hash([tr.provider_event_id, pm.provider_event_id, pm.condition_id])}"
    return {
        "mapping_id": mapping_id,
        "run_id": "",
        "run_started_at": "",
        "therundown_event_id": tr.provider_event_id,
        "polymarket_event_id": pm.provider_event_id,
        "polymarket_condition_id": pm.condition_id,
        "canonical_event_id": canonical_event_id,
        "canonical_market_key": f"{canonical_event_id}:{output_period}:{output_market_type}",
        "sport": tr.sport,
        "league": tr.league,
        "therundown_event_name": tr.event_name,
        "polymarket_event_title": pm.event_title,
        "therundown_home_raw": tr.home_team_raw,
        "therundown_away_raw": tr.away_team_raw,
        "polymarket_home_raw": pm.home_team_raw,
        "polymarket_away_raw": pm.away_team_raw,
        "participants_normalized": {
            "home": normalized_home,
            "away": normalized_away,
            "normalization_steps": {
                "home": home_steps,
                "away": away_steps,
            },
        },
        "start_time_therundown": tr.start_time_utc,
        "start_time_polymarket": pm.start_time_utc,
        "event_time_delta_seconds": delta_seconds,
        "event_date_match_status": date_status,
        "market_type": output_market_type,
        "period": output_period,
        "outcome_match_status": pair_status,
        "home_away_status": invariant,
        "name_similarity_score": round(name_score, 4),
        "time_score": round(time_score, 4),
        "market_type_score": round(market_type_score, 4),
        "participant_score": round(participant_score, 4),
        "home_away_score": round(home_away_score, 4),
        "league_score": round(league_score, 4),
        "outcome_set_score": round(outcome_set_score, 4),
        "confidence": confidence,
        "decision": decision,
        "reject_reasons": sorted(set(reject_reasons)),
        "review_reasons": sorted(set(review_reasons)),
        "raw_refs": {
            "therundown": tr.raw_ref,
            "polymarket": pm.raw_ref,
        },
        "created_at": isoformat_utc(now),
    }


def candidate_allowed(
    tr: ProviderEvent,
    pm: ProviderMarket,
    aliases: dict[str, dict[str, str]],
    now: datetime,
    lookahead_hours: int,
) -> bool:
    if tr.sport != pm.sport and not ({tr.sport, pm.sport} <= {"tennis", "atp", "wta"}):
        return False
    if not is_concrete_polymarket_game(pm):
        return False
    tr_dt = parse_iso_datetime(tr.start_time_utc)
    if tr_dt and not (now - timedelta(hours=6) <= tr_dt <= now + timedelta(hours=lookahead_hours)):
        return False
    pm_dt = parse_iso_datetime(pm.start_time_utc)
    if tr_dt and pm_dt and not (event_date_candidates(tr.start_time_utc) & event_date_candidates(pm.start_time_utc, pm.event_slug)):
        return False
    participant_hits = [
        name_similarity(a, b, tr.sport, aliases)
        for a in (tr.participants_raw or tr.outcome_names_raw)
        for b in (pm.participants_raw or pm.outcome_names_raw)
    ]
    pair_score, _, _ = pair_participant_scores(
        tr.outcome_names_raw or tr.participants_raw,
        pm.outcome_names_raw or pm.participants_raw,
        tr.sport,
        aliases,
    )
    if pair_score >= 0.86:
        return True
    if participant_hits and max(participant_hits) >= 0.72:
        return True
    return False


def match_events(
    therundown_events: list[ProviderEvent],
    polymarket_markets: list[ProviderMarket],
    aliases: dict[str, dict[str, str]],
    now: datetime,
    min_confidence: float,
    include_needs_review: bool,
    lookahead_hours: int = 24,
) -> dict[str, list[dict[str, Any]]]:
    scored: list[dict[str, Any]] = []
    for tr in therundown_events:
        for pm in polymarket_markets:
            if candidate_allowed(tr, pm, aliases, now, lookahead_hours):
                scored.append(score_candidate(tr, pm, aliases, now, min_confidence))
    matched = [item for item in scored if item["decision"] == "matched"]
    needs_review = [item for item in scored if item["decision"] == "needs_review"]
    rejected = [item for item in scored if item["decision"] == "rejected"]
    mapped_tr = {item["therundown_event_id"] for item in matched + needs_review}
    mapped_pm = {str(item["polymarket_condition_id"]) for item in matched + needs_review}
    unmatched_tr = [asdict(item) for item in therundown_events if item.provider_event_id not in mapped_tr]
    unmatched_pm = [asdict(item) for item in polymarket_markets if str(item.condition_id) not in mapped_pm]
    latest = matched + (needs_review if include_needs_review else [])
    return {
        "latest": sorted(latest, key=lambda item: (-item["confidence"], item["therundown_event_id"])),
        "matched": sorted(matched, key=lambda item: -item["confidence"]),
        "needs_review": sorted(needs_review, key=lambda item: -item["confidence"]),
        "rejected": sorted(rejected, key=lambda item: -item["confidence"]),
        "unmatched_therundown": unmatched_tr,
        "unmatched_polymarket": unmatched_pm,
    }


def assert_safe_url(url: str) -> None:
    parsed = urllib.parse.urlparse(url)
    host = parsed.netloc.lower()
    path = parsed.path.lower()
    if "clob.polymarket.com" in host and HARD_ORDER_PATH_RE.search(path):
        raise SecurityError(f"Blocked order/signing URL: {host}{path}")
    if HARD_ORDER_PATH_RE.search(path) and "gamma-api.polymarket.com" not in host:
        raise SecurityError(f"Blocked unsafe URL path: {host}{path}")


def http_get_json(url: str, headers: dict[str, str] | None = None, timeout: int = 25) -> tuple[Any, dict[str, str], int]:
    assert_safe_url(url)
    request = urllib.request.Request(url, headers=headers or {})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            body = response.read().decode("utf-8")
            return json.loads(body), dict(response.headers.items()), int(response.status)
    except urllib.error.HTTPError as exc:
        if exc.code in {401, 403}:
            raise ApiPermissionError(f"API permission failure: HTTP {exc.code}") from exc
        raise NetworkFailureError(f"HTTP failure: {exc.code}") from exc
    except urllib.error.URLError as exc:
        raise NetworkFailureError(f"network failure: {exc.reason}") from exc
    except TimeoutError as exc:
        raise NetworkFailureError("network timeout") from exc
    except json.JSONDecodeError as exc:
        raise NetworkFailureError("invalid JSON response from live API") from exc


def load_therundown_config() -> dict[str, Any]:
    config = read_simple_toml(ROOT / "configs/sources/therundown.example.toml")
    config.setdefault("api_base_url", "https://therundown.io/api/v2")
    config.setdefault("auth_env", "THERUNDOWN_API_KEY")
    config.setdefault("market_ids", [1])
    config.setdefault("affiliate_ids", [19, 23])
    return config


def selected_sports(raw: str) -> list[str]:
    values = [item.strip().lower() for item in raw.split(",") if item.strip()]
    expanded: list[str] = []
    for item in values:
        if item == "tennis":
            expanded.extend(["atp", "wta"])
        else:
            expanded.append(item)
    return list(dict.fromkeys(expanded))


def fetch_therundown_current_events(
    sports: list[str],
    lookahead_hours: int,
    now: datetime,
) -> tuple[list[ProviderEvent], dict[str, Any]]:
    load_dotenv()
    config = load_therundown_config()
    env_name = str(config.get("auth_env") or "THERUNDOWN_API_KEY")
    key = os.environ.get(env_name) or os.environ.get("THERUNDOWN_API_KEY") or os.environ.get("THERUNDON_API_KEY")
    if not key:
        raise ConfigMissingError(f"TheRundown API key env var is missing: {env_name}")
    api_base = str(config["api_base_url"]).rstrip("/")
    market_ids = ",".join(str(item) for item in config.get("market_ids", [1]))
    affiliate_ids = ",".join(str(item) for item in config.get("affiliate_ids", [19, 23]))
    dates = sorted({(now + timedelta(hours=hours)).date().isoformat() for hours in range(0, lookahead_hours + 25, 24)})
    received_at = isoformat_utc(now)
    events: list[ProviderEvent] = []
    entitlement: dict[str, str] = {}
    unsupported: list[str] = []
    for sport in sports:
        if sport not in THERUNDOWN_SPORT_IDS:
            unsupported.append(sport)
            continue
        sport_id = THERUNDOWN_SPORT_IDS[sport]
        for date in dates:
            params = urllib.parse.urlencode({
                "market_ids": market_ids,
                "affiliate_ids": affiliate_ids,
                "main_line": "true",
                "offset": "300",
            })
            url = f"{api_base}/sports/{sport_id}/events/{date}?{params}"
            payload, headers, status = http_get_json(
                url,
                headers={
                    "X-TheRundown-Key": key,
                    "User-Agent": "quantSys-live-mapping/0.1",
                },
            )
            if status != 200:
                raise NetworkFailureError(f"TheRundown returned HTTP {status}")
            entitlement.update({
                header.lower(): value
                for header, value in headers.items()
                if header.lower().startswith(("x-tier", "x-data", "x-websocket", "x-rate", "x-datapoints"))
            })
            events.extend(parse_therundown_events(
                payload,
                sport=sport,
                received_at=received_at,
                raw_ref=raw_ref("therundown", f"events:{sport}:{date}", payload),
                now=now,
                lookahead_hours=lookahead_hours,
            ))
    source_status = {
        "status": "ok",
        "sports_requested": sports,
        "unsupported_sports": unsupported,
        "entitlement": entitlement,
        "event_count": len(events),
        "key_env": env_name,
    }
    return dedupe_events(events), source_status


def dedupe_events(events: list[ProviderEvent]) -> list[ProviderEvent]:
    seen: set[str] = set()
    result: list[ProviderEvent] = []
    for event in events:
        if event.provider_event_id in seen:
            continue
        seen.add(event.provider_event_id)
        result.append(event)
    return result


def load_polymarket_config() -> dict[str, Any]:
    config = read_simple_toml(ROOT / "configs/sources/polymarket.example.toml")
    config.setdefault("gamma_api_base_url", "https://gamma-api.polymarket.com")
    config.setdefault("geoblock_url", "https://polymarket.com/api/geoblock")
    return config


def polymarket_sport_tags(sports: list[str]) -> tuple[dict[str, str], dict[str, Any]]:
    sports_payload, _, _ = http_get_json(
        "https://gamma-api.polymarket.com/sports",
        headers={"User-Agent": "quantSys-live-mapping/0.1"},
    )
    tag_by_sport: dict[str, str] = {}
    for row in sports_payload:
        sport = str(row.get("sport") or "").lower()
        if sport not in sports:
            continue
        tags = [item.strip() for item in str(row.get("tags") or "").split(",") if item.strip()]
        specific = [tag for tag in tags if tag not in COMMON_POLYMARKET_SPORT_TAGS]
        if specific:
            tag_by_sport[sport] = specific[-1]
    return tag_by_sport, {"sports_rows": len(sports_payload)}


def fetch_polymarket_active_markets(
    sports: list[str],
    now: datetime,
    lookahead_hours: int,
) -> tuple[list[ProviderMarket], dict[str, Any]]:
    tag_by_sport, meta = polymarket_sport_tags(sports)
    received_at = isoformat_utc(now)
    all_markets: list[ProviderMarket] = []
    event_seen: set[str] = set()
    events_count = 0
    for sport, tag_id in tag_by_sport.items():
        for page in range(3):
            params = urllib.parse.urlencode({
                "active": "true",
                "closed": "false",
                "limit": "100",
                "offset": str(page * 100),
                "tag_id": tag_id,
            })
            url = f"https://gamma-api.polymarket.com/events?{params}"
            payload, _, status = http_get_json(url, headers={"User-Agent": "quantSys-live-mapping/0.1"})
            if status != 200:
                raise NetworkFailureError(f"Polymarket Gamma returned HTTP {status}")
            if not isinstance(payload, list):
                raise NetworkFailureError("Polymarket Gamma events response was not a list")
            events_count += len(payload)
            unique_events = []
            for event in payload:
                event_id = str(event.get("id") or event.get("slug") or stable_hash(event))
                if event_id in event_seen:
                    continue
                event_seen.add(event_id)
                unique_events.append(event)
            all_markets.extend(parse_polymarket_events(
                unique_events,
                sport=sport,
                received_at=received_at,
                raw_ref_prefix=f"raw:polymarket:gamma_events:{sport}:{tag_id}",
                now=now,
            ))
            if len(payload) < 100:
                break
    filtered = [
        market for market in all_markets
        if market.active and not market.closed and (market.token_ids or market.condition_id)
    ]
    source_status = {
        "status": "ok",
        "sports_requested": sports,
        "tag_by_sport": tag_by_sport,
        "events_fetched": events_count,
        "market_count": len(filtered),
        **meta,
    }
    return filtered, source_status


def fetch_polymarket_geoblock() -> dict[str, Any]:
    try:
        payload, _, status = http_get_json(
            "https://polymarket.com/api/geoblock",
            headers={"User-Agent": "quantSys-live-mapping/0.1"},
            timeout=15,
        )
        return {
            "status": "ok" if status == 200 else "unknown",
            "blocked": bool(payload.get("blocked")) if isinstance(payload, dict) else None,
            "country": payload.get("country") if isinstance(payload, dict) else None,
            "region": payload.get("region") if isinstance(payload, dict) else None,
            "ip": "<redacted-ip>" if isinstance(payload, dict) and payload.get("ip") else None,
        }
    except LiveMappingError as exc:
        return {"status": "unknown", "error": str(exc), "blocked": None}


def alias_candidates_from_unmatched(unmatched: list[dict[str, Any]], aliases: dict[str, dict[str, str]]) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    for item in unmatched:
        sport = item.get("sport") or item.get("league") or "unknown"
        for name in item.get("participants_raw") or []:
            normalized, _ = normalize_name(name, sport, aliases)
            if normalized and normalized not in aliases.get(str(sport), {}):
                candidates.append({"sport": sport, "raw_name": name, "normalized": normalized})
    return candidates[:200]


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n")


def write_outputs(
    output_dir: Path,
    run_id: str,
    run_started_at: str,
    therundown_events: list[ProviderEvent],
    polymarket_markets: list[ProviderMarket],
    decisions: dict[str, list[dict[str, Any]]],
    source_status: dict[str, Any],
    alias_candidates: list[dict[str, Any]],
    live_blocking_items: list[str],
) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for collection in ["latest", "matched", "needs_review", "rejected"]:
        for item in decisions.get(collection, []):
            item["run_id"] = run_id
            item["run_started_at"] = run_started_at
    write_json(output_dir / "latest.json", decisions.get("latest", []))
    write_json(output_dir / "unmatched_therundown.json", decisions.get("unmatched_therundown", []))
    write_json(output_dir / "unmatched_polymarket.json", decisions.get("unmatched_polymarket", []))
    write_json(output_dir / "needs_review.json", decisions.get("needs_review", []))
    write_json(output_dir / "rejected.json", decisions.get("rejected", []))
    write_json(output_dir / "alias_candidates.json", alias_candidates)
    write_json(output_dir / "source_status.json", source_status)
    csv_fields = [
        "mapping_id", "decision", "confidence", "sport", "league",
        "therundown_event_name", "polymarket_event_title",
        "start_time_therundown", "start_time_polymarket",
        "event_time_delta_seconds", "home_away_status",
        "reject_reasons", "review_reasons",
    ]
    with (output_dir / "latest.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=csv_fields)
        writer.writeheader()
        for item in decisions.get("latest", []):
            row = {field: item.get(field) for field in csv_fields}
            row["reject_reasons"] = ";".join(item.get("reject_reasons") or [])
            row["review_reasons"] = ";".join(item.get("review_reasons") or [])
            writer.writerow(row)
    write_markdown_report(
        output_dir / "latest.md",
        run_started_at,
        therundown_events,
        polymarket_markets,
        decisions,
        source_status,
        live_blocking_items,
    )


def summarize_status(status: dict[str, Any]) -> str:
    safe = dict(status)
    if "key_env" in safe:
        safe["key_env"] = str(safe["key_env"])
    return json.dumps(safe, ensure_ascii=False, sort_keys=True)


def write_markdown_report(
    path: Path,
    run_started_at: str,
    therundown_events: list[ProviderEvent],
    polymarket_markets: list[ProviderMarket],
    decisions: dict[str, list[dict[str, Any]]],
    source_status: dict[str, Any],
    live_blocking_items: list[str],
) -> None:
    matched = decisions.get("matched", [])
    needs_review = decisions.get("needs_review", [])
    rejected = decisions.get("rejected", [])
    unmatched_tr = decisions.get("unmatched_therundown", [])
    unmatched_pm = decisions.get("unmatched_polymarket", [])
    reversed_items = [item for item in needs_review + matched if item.get("home_away_status") == "reversed"]
    lines = [
        "# Live Mapping Report",
        "",
        f"- Run started at: `{run_started_at}`",
        f"- TheRundown events: `{len(therundown_events)}`",
        f"- Polymarket markets: `{len(polymarket_markets)}`",
        f"- matched: `{len(matched)}`",
        f"- needs_review: `{len(needs_review)}`",
        f"- rejected: `{len(rejected)}`",
        f"- unmatched TheRundown: `{len(unmatched_tr)}`",
        f"- unmatched Polymarket: `{len(unmatched_pm)}`",
        "",
        "## Data Source Status",
        "",
        f"- TheRundown: `{summarize_status(source_status.get('therundown', {}))}`",
        f"- Polymarket: `{summarize_status(source_status.get('polymarket', {}))}`",
        f"- Polymarket geoblock: `{summarize_status(source_status.get('polymarket_geoblock', {}))}`",
        "",
        "## Live Blocking Items",
        "",
    ]
    if live_blocking_items:
        lines.extend([f"- {item}" for item in live_blocking_items])
    else:
        lines.append("- None for mapping. Live execution remains disabled by task scope.")
    lines.extend(["", "## Matched", ""])
    if matched:
        lines.extend([f"- {item['therundown_event_name']} -> {item['polymarket_event_title']} (`{item['confidence']}`)" for item in matched])
    else:
        lines.append("- None")
    lines.extend(["", "## Needs Review", ""])
    if needs_review:
        for item in needs_review:
            lines.append(f"- {item['therundown_event_name']} -> {item['polymarket_event_title']} (`{item['confidence']}`): {', '.join(item.get('review_reasons') or [])}")
    else:
        lines.append("- None")
    lines.extend(["", "## Rejected", ""])
    if rejected:
        for item in rejected[:100]:
            lines.append(f"- {item['therundown_event_name']} -> {item['polymarket_event_title']} (`{item['confidence']}`): {', '.join(item.get('reject_reasons') or [])}")
        if len(rejected) > 100:
            lines.append(f"- ... {len(rejected) - 100} additional rejected candidates omitted from markdown; see rejected.json")
    else:
        lines.append("- None")
    lines.extend(["", "## Home/Away Reversed", ""])
    if reversed_items:
        for item in reversed_items:
            lines.append(f"- {item['therundown_event_name']} -> {item['polymarket_event_title']}: {', '.join(item.get('review_reasons') or [])}")
    else:
        lines.append("- None")
    lines.extend([
        "",
        "## Phase Integration Notes",
        "",
        "- Later normalizer integration should consume provider snapshots and preserve `raw_refs`.",
        "- Later canonical-mapper integration can persist `matched`, `needs_review`, and rejected audit reasons into mapping tables.",
        "- Phase 8 dry-run can add stricter live-readiness gates for market type, unknown home/away, reversed home/away, and exact start-time alignment.",
        "- This command does not create order intents, read private keys, sign orders, or call Polymarket order endpoints.",
        "",
    ])
    path.write_text("\n".join(lines))


def write_failure_report(output_dir: Path, run_started_at: str, message: str, code: int) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    text = "\n".join([
        "# Live Mapping Report",
        "",
        f"- Run started at: `{run_started_at}`",
        f"- Source unavailable: `{message}`",
        f"- Exit code: `{code}`",
        "",
        "No realtime mapping results were fabricated.",
        "",
    ])
    (output_dir / "latest.md").write_text(text)
    for name in ["latest.json", "unmatched_therundown.json", "unmatched_polymarket.json", "needs_review.json", "rejected.json"]:
        write_json(output_dir / name, [])
    (output_dir / "latest.csv").write_text("mapping_id,decision,confidence\n")


def build_live_blocking_items(source_status: dict[str, Any]) -> list[str]:
    items: list[str] = []
    tr = source_status.get("therundown", {})
    entitlement = tr.get("entitlement", {})
    delay = entitlement.get("x-data-delay-seconds")
    ws_access = entitlement.get("x-websocket-access")
    if delay not in {None, "0", 0}:
        items.append("therundown_delayed_source")
    if str(ws_access).lower() not in {"true", "none"} and ws_access is not None:
        items.append("therundown_no_websocket_access")
    geoblock = source_status.get("polymarket_geoblock", {})
    if geoblock.get("blocked") is True:
        items.append("polymarket_geoblocked")
    return items


def run_live_mapping(args: argparse.Namespace) -> tuple[int, dict[str, Any]]:
    now = utc_now()
    run_started_at = isoformat_utc(now)
    run_id = f"live_mapping_{now.strftime('%Y%m%dT%H%M%SZ')}_{int(time.time())}"
    output_dir = Path(args.output)
    aliases = load_aliases(DEFAULT_ALIAS_PATH)
    sports = selected_sports(args.sports)
    source_status: dict[str, Any] = {}
    therundown_events: list[ProviderEvent] = []
    polymarket_markets: list[ProviderMarket] = []
    if args.therundown_enabled:
        therundown_events, source_status["therundown"] = fetch_therundown_current_events(
            sports=sports,
            lookahead_hours=args.lookahead_hours,
            now=now,
        )
    else:
        source_status["therundown"] = {"status": "disabled"}
    if args.polymarket_enabled:
        polymarket_markets, source_status["polymarket"] = fetch_polymarket_active_markets(
            sports=sports,
            now=now,
            lookahead_hours=args.lookahead_hours,
        )
        source_status["polymarket_geoblock"] = fetch_polymarket_geoblock()
    else:
        source_status["polymarket"] = {"status": "disabled"}
    if args.therundown_enabled and not therundown_events:
        raise DataEmptyError("TheRundown returned zero current/lookahead moneyline events")
    if args.polymarket_enabled and not polymarket_markets:
        raise DataEmptyError("Polymarket returned zero active sports markets")
    decisions = match_events(
        therundown_events,
        polymarket_markets,
        aliases,
        now,
        args.min_confidence,
        args.include_needs_review,
        args.lookahead_hours,
    )
    alias_candidates = alias_candidates_from_unmatched(decisions["unmatched_therundown"], aliases)
    live_blocking_items = build_live_blocking_items(source_status)
    write_outputs(
        output_dir=output_dir,
        run_id=run_id,
        run_started_at=run_started_at,
        therundown_events=therundown_events,
        polymarket_markets=polymarket_markets,
        decisions=decisions,
        source_status=source_status,
        alias_candidates=alias_candidates,
        live_blocking_items=live_blocking_items,
    )
    return EXIT_OK, {
        "run_id": run_id,
        "therundown_events": len(therundown_events),
        "polymarket_markets": len(polymarket_markets),
        "matched": len(decisions["matched"]),
        "needs_review": len(decisions["needs_review"]),
        "rejected": len(decisions["rejected"]),
        "output": str(output_dir),
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run live TheRundown <-> Polymarket event mapping.")
    parser.add_argument("--sports", default="nba,nfl,mlb,nhl,tennis")
    parser.add_argument("--lookahead-hours", type=int, default=24)
    parser.add_argument("--therundown-enabled", type=parse_bool, default=True)
    parser.add_argument("--polymarket-enabled", type=parse_bool, default=True)
    parser.add_argument("--output", default=str(DEFAULT_OUTPUT_DIR))
    parser.add_argument("--min-confidence", type=float, default=0.95)
    parser.add_argument("--include-needs-review", type=parse_bool, default=True)
    parser.add_argument("--dry-run", type=parse_bool, default=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    run_started_at = isoformat_utc(utc_now())
    try:
        code, summary = run_live_mapping(args)
        print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
        return code
    except LiveMappingError as exc:
        code = exc.exit_code
        write_failure_report(Path(args.output), run_started_at, str(exc), code)
        print(f"live-mapping failed ({code}): {exc}", file=sys.stderr)
        return code
    except Exception as exc:
        write_failure_report(Path(args.output), run_started_at, f"unexpected error: {exc}", EXIT_RUNTIME_ERROR)
        print(f"live-mapping failed ({EXIT_RUNTIME_ERROR}): {exc}", file=sys.stderr)
        return EXIT_RUNTIME_ERROR


if __name__ == "__main__":
    raise SystemExit(main())
