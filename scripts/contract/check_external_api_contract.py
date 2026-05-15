#!/usr/bin/env python3
"""Smoke checks for Phase 1 external API contract artifacts."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

THERUNDOWN_FIXTURES = [
    "events_bootstrap.json",
    "markets_delta.json",
    "ws_market_price.json",
    "ws_heartbeat.json",
    "rate_limit_headers.json",
    "off_board_price.json",
]

POLYMARKET_FIXTURES = [
    "market_subscribe.json",
    "market_book.json",
    "market_price_change.json",
    "market_best_bid_ask.json",
    "user_order_update.json",
    "geoblock_blocked.json",
    "create_order_response.json",
]

FIXTURE_PATHS = [
    Path("tests/fixtures/external/therundown") / name
    for name in THERUNDOWN_FIXTURES
] + [
    Path("tests/fixtures/external/polymarket") / name
    for name in POLYMARKET_FIXTURES
]

SECRET_ASSIGNMENT_PATTERNS = [
    re.compile(r"(?i)(api[_-]?key|secret|passphrase|private[_-]?key|signature)[\"']?\s*[:=]\s*[\"']?([^\"'\s,}]+)"),
]


class CheckFailure(Exception):
    pass


def read_json(path: Path) -> object:
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError as exc:
        raise CheckFailure(f"{path} is not valid JSON: {exc}") from exc


def assert_true(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def scan_text_for_secrets(path: Path) -> None:
    text = path.read_text(errors="replace")
    for pattern in SECRET_ASSIGNMENT_PATTERNS:
        for match in pattern.finditer(text):
            value = match.group(2)
            if value.startswith("<redacted"):
                continue
            if re.fullmatch(r"[A-Z][A-Z0-9_]{5,}", value):
                continue
            if re.fullmatch(r"official_doc_configured|mock|unknown", value):
                continue
            suspicious = (
                len(value) >= 20
                or value.startswith("0x")
                or "PRIVATE KEY" in value
            )
            if suspicious:
                raise CheckFailure(f"{path} contains a value matching a secret pattern")


def parse_manifest_fixture_paths(path: Path) -> list[str]:
    text = path.read_text()
    paths = re.findall(r"^\s*(?:-\s*)?fixture_path:\s*['\"]?([^'\"\n]+)['\"]?\s*$", text, re.MULTILINE)
    if not paths:
        raise CheckFailure(f"{path} does not list any fixture_path entries")
    return paths


def check_fixture_json() -> dict[Path, object]:
    loaded: dict[Path, object] = {}
    for rel_path in FIXTURE_PATHS:
        path = ROOT / rel_path
        assert_true(path.exists(), f"Missing fixture: {rel_path}")
        loaded[rel_path] = read_json(path)
        scan_text_for_secrets(path)
    return loaded


def check_manifest() -> None:
    manifest = ROOT / "tests/contract/external_api_contract_manifest.yaml"
    assert_true(manifest.exists(), "Missing contract manifest")
    scan_text_for_secrets(manifest)
    listed_paths = set(parse_manifest_fixture_paths(manifest))
    expected_paths = {str(path) for path in FIXTURE_PATHS}
    missing = sorted(expected_paths - listed_paths)
    extra = sorted(listed_paths - expected_paths)
    assert_true(not missing, f"Manifest missing fixture paths: {missing}")
    assert_true(not extra, f"Manifest contains unexpected fixture paths: {extra}")
    for item in listed_paths:
        assert_true((ROOT / item).exists(), f"Manifest fixture path does not exist: {item}")
    text = manifest.read_text()
    for required in ("provider:", "channel:", "message_type:", "source_type:", "sanitized_fields:", "blocking_level:"):
        assert_true(required in text, f"Manifest missing metadata key {required}")


def check_required_fixture_fields(loaded: dict[Path, object]) -> None:
    tr_ws = loaded[Path("tests/fixtures/external/therundown/ws_market_price.json")]
    assert_true(isinstance(tr_ws, dict), "TheRundown ws_market_price must be an object")
    assert_true(tr_ws.get("meta", {}).get("type") == "market_price", "TheRundown ws_market_price meta.type must be market_price")
    data = tr_ws.get("data", {})
    for field in (
        "id",
        "event_id",
        "affiliate_id",
        "market_id",
        "market_participant_id",
        "normalized_market_participant_id",
        "line",
        "price",
        "previous_price",
        "is_main_line",
        "sport_id",
        "updated_at",
    ):
        assert_true(field in data, f"TheRundown ws_market_price missing data.{field}")

    off_board = loaded[Path("tests/fixtures/external/therundown/off_board_price.json")]
    assert_true(off_board.get("data", {}).get("price") == "0.0001", "TheRundown off_board_price must use price=0.0001")
    assert_true("off-board" in json.dumps(off_board).lower(), "TheRundown off_board_price must describe off-board sentinel")

    market_subscribe = loaded[Path("tests/fixtures/external/polymarket/market_subscribe.json")]
    assert_true("assets_ids" in market_subscribe, "Polymarket market_subscribe must use assets_ids")
    assert_true("asset_ids" not in market_subscribe, "Polymarket market_subscribe must not use asset_ids")

    best_bid_ask = loaded[Path("tests/fixtures/external/polymarket/market_best_bid_ask.json")]
    assert_true(best_bid_ask.get("event_type") == "best_bid_ask", "Polymarket best_bid_ask fixture must use event_type=best_bid_ask")
    assert_true(best_bid_ask.get("custom_feature_enabled") is True, "Polymarket best_bid_ask must indicate custom_feature_enabled=true")

    geoblock = loaded[Path("tests/fixtures/external/polymarket/geoblock_blocked.json")]
    assert_true(geoblock.get("blocked") is True, "Polymarket geoblock_blocked must use blocked=true")


def check_config_examples() -> None:
    config_dir = ROOT / "configs/sources"
    expected = ["therundown.example.toml", "polymarket.example.toml"]
    for name in expected:
        path = config_dir / name
        assert_true(path.exists(), f"Missing config example: {path.relative_to(ROOT)}")
        scan_text_for_secrets(path)
        text = path.read_text()
        assert_true("<redacted" not in text, f"{path.relative_to(ROOT)} should contain env var names, not placeholder secret values")
    polymarket = (config_dir / "polymarket.example.toml").read_text()
    assert_true('execution_enabled = false' in polymarket, "Polymarket execution_enabled must default false")
    assert_true('geoblock_required = true' in polymarket, "Polymarket geoblock_required must default true")


def check_adapter_baseline() -> None:
    path = ROOT / "docs/adapters/api-contract-baseline.md"
    assert_true(path.exists(), "Missing docs/adapters/api-contract-baseline.md")
    scan_text_for_secrets(path)
    text = path.read_text()
    for section in ("RawMessage", "NormalizedQuote", "SourceState"):
        assert_true(section in text, f"Adapter baseline missing {section} section")
    for degradation in ("delayed source", "no websocket access", "stale source", "geoblocked", "unknown schema", "DLQ"):
        assert_true(degradation in text, f"Adapter baseline missing degradation rule: {degradation}")


def check_reports() -> None:
    path = ROOT / "docs/reports/external-api-contract-spike-2026-05-15.md"
    assert_true(path.exists(), "Missing Phase 1 contract report")
    scan_text_for_secrets(path)
    text = path.read_text()
    for section in ("Phase 1 Scope", "TheRundown Contract Baseline", "Polymarket Contract Baseline", "Secret / Compliance Baseline", "Probed / Unprobed / Unknown"):
        assert_true(section in text, f"Phase 1 report missing section: {section}")


def check_degradation_docs_or_fixtures() -> None:
    candidates = [
        ROOT / "docs/adapters/api-contract-baseline.md",
        ROOT / "docs/reports/external-api-contract-spike-2026-05-15.md",
        ROOT / "tests/fixtures/external/therundown/rate_limit_headers.json",
        ROOT / "tests/fixtures/external/polymarket/geoblock_blocked.json",
    ]
    combined = "\n".join(path.read_text(errors="replace") for path in candidates if path.exists())
    for term in ("429", "Retry-After", "stale", "geoblock"):
        assert_true(term.lower() in combined.lower(), f"Missing degradation evidence for {term}")


def main() -> int:
    checks = [
        ("fixture JSON", lambda: check_fixture_json()),
        ("manifest", check_manifest),
        ("fixture required fields", lambda: check_required_fixture_fields(check_fixture_json())),
        ("config examples", check_config_examples),
        ("adapter baseline", check_adapter_baseline),
        ("contract report", check_reports),
        ("degradation rules", check_degradation_docs_or_fixtures),
    ]
    failures: list[str] = []
    for name, fn in checks:
        try:
            fn()
            print(f"ok - {name}")
        except CheckFailure as exc:
            failures.append(f"FAIL - {name}: {exc}")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print("external API contract smoke checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
