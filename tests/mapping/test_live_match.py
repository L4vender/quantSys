import json
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

from scripts.mapping import live_match


ROOT = Path(__file__).resolve().parents[2]


class LiveMatchTests(unittest.TestCase):
    def setUp(self):
        self.aliases = live_match.load_aliases(ROOT / "configs/mapping/team_aliases.yaml")
        self.now = datetime(2026, 5, 15, 14, 0, tzinfo=timezone.utc)

    def test_name_normalization_and_alias_matching(self):
        cle, steps = live_match.normalize_name("CLE.", "mlb", self.aliases)
        self.assertEqual(cle, "cleveland guardians")
        self.assertIn("alias:cle->cleveland guardians", steps)
        score = live_match.name_similarity("NY Yankees", "New York Yankees", "mlb", self.aliases)
        self.assertGreaterEqual(score, 0.95)
        self.assertGreaterEqual(live_match.name_similarity("Oakland Athletics", "Athletics", "mlb", self.aliases), 0.95)
        self.assertGreaterEqual(live_match.name_similarity("Cavaliers", "Cleveland Cavaliers", "nba", self.aliases), 0.95)

    def test_home_away_reversed_is_audit_only_for_date_team_match(self):
        tr = live_match.ProviderEvent(
            provider="therundown",
            provider_event_id="tr_lal_bos",
            sport="nba",
            league="nba",
            event_name="Los Angeles at Boston",
            start_time_utc="2026-05-16T00:00:00Z",
            home_team_raw="Boston Celtics",
            away_team_raw="Los Angeles Lakers",
            participants_raw=["Los Angeles Lakers", "Boston Celtics"],
            market_type_raw="moneyline",
            outcome_names_raw=["Los Angeles Lakers", "Boston Celtics"],
            source_timestamp="2026-05-15T14:00:00Z",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_lal_bos",
            event_slug="nba-bos-lal-2026-05-16",
            condition_id="0xabc",
            market_id="1",
            token_ids=["yes", "no"],
            sport="nba",
            league="nba",
            event_title="Boston Celtics vs. Los Angeles Lakers",
            market_title="Boston Celtics vs. Los Angeles Lakers",
            start_time_utc="2026-05-16T00:00:00Z",
            participants_raw=["Boston Celtics", "Los Angeles Lakers"],
            outcome_names_raw=["Boston Celtics", "Los Angeles Lakers"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
            home_team_raw="Los Angeles Lakers",
            away_team_raw="Boston Celtics",
            home_away_status="explicit",
        )
        result = live_match.score_candidate(tr, pm, self.aliases, self.now, min_confidence=0.95)
        self.assertEqual(result["home_away_status"], "reversed")
        self.assertEqual(result["decision"], "matched")
        self.assertNotIn("home_away_reversed", result["review_reasons"])

    def test_series_market_is_excluded_even_when_date_and_teams_match(self):
        self.assertEqual(live_match.normalize_market_type("moneyline", ["A", "B"], "A vs B")[0], "moneyline")
        self.assertEqual(live_match.normalize_market_type("Who will win the series?", ["A", "B"], "series")[0], "series")
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_series_same_date",
            event_slug="mlb-cin-cle-series",
            condition_id="0xseries",
            market_id="series",
            token_ids=["cin", "cle"],
            sport="mlb",
            league="mlb",
            event_title="Cincinnati Reds vs. Cleveland Guardians",
            market_title="Who will win the series, Cincinnati Reds or Cleveland Guardians?",
            start_time_utc="2026-05-15T18:00:00Z",
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
            market_type_raw="series",
        )
        self.assertFalse(live_match.candidate_allowed(tr_events[0], pm, self.aliases, self.now, 24))

    def test_concrete_game_moneyline_is_allowed(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_game_same_date",
            event_slug="mlb-cin-cle-2026-05-15",
            condition_id="0xgame",
            market_id="game",
            token_ids=["cin", "cle"],
            sport="mlb",
            league="mlb",
            event_title="Cincinnati Reds vs. Cleveland Guardians",
            market_title="Cincinnati Reds vs. Cleveland Guardians",
            start_time_utc="2026-05-15T23:10:00Z",
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
            market_type_raw="moneyline",
        )
        self.assertTrue(live_match.candidate_allowed(tr_events[0], pm, self.aliases, self.now, 24))

    def test_total_market_is_not_a_concrete_game_match(self):
        self.assertEqual(live_match.normalize_market_type("Cincinnati Reds vs. Cleveland Guardians: O/U 8.5", ["Over", "Under"])[0], "total")

    def test_missing_start_time_needs_review_and_title_only_home_away_unknown(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_title_only",
            event_slug="mlb-cin-cle",
            condition_id="0xdef",
            market_id="2",
            token_ids=["cin", "cle"],
            sport="mlb",
            league="mlb",
            event_title="Cincinnati Reds vs. Cleveland Guardians",
            market_title="Cincinnati Reds vs. Cleveland Guardians",
            start_time_utc=None,
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
            home_team_raw=None,
            away_team_raw=None,
            home_away_status="unknown",
        )
        result = live_match.score_candidate(tr_events[0], pm, self.aliases, self.now, min_confidence=0.95)
        self.assertEqual(result["home_away_status"], "unknown")
        self.assertEqual(result["decision"], "needs_review")
        self.assertIn("missing_start_time", result["review_reasons"])

    def test_same_date_team_match_allows_large_time_delta_and_unknown_home_away(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_same_date",
            event_slug="mlb-cin-cle",
            condition_id="0xdate",
            market_id="2",
            token_ids=["cin", "cle"],
            sport="mlb",
            league="mlb",
            event_title="Cincinnati Reds vs. Cleveland Guardians",
            market_title="Cincinnati Reds vs. Cleveland Guardians",
            start_time_utc="2026-05-15T03:00:00Z",
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
            home_team_raw=None,
            away_team_raw=None,
            home_away_status="unknown",
        )
        result = live_match.score_candidate(tr_events[0], pm, self.aliases, self.now, min_confidence=0.95)
        self.assertEqual(result["decision"], "matched")
        self.assertEqual(result["event_date_match_status"], "same_date")
        self.assertNotIn("start_time_delta_large", result["review_reasons"])
        self.assertNotIn("polymarket_home_away_unknown", result["review_reasons"])

    def test_different_date_rejected_even_when_teams_match(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_next_day",
            event_slug="mlb-cin-cle-next-day",
            condition_id="0xnextday",
            market_id="3",
            token_ids=["cin", "cle"],
            sport="mlb",
            league="mlb",
            event_title="Cincinnati Reds vs. Cleveland Guardians",
            market_title="Cincinnati Reds vs. Cleveland Guardians",
            start_time_utc="2026-05-16T23:10:00Z",
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
        )
        result = live_match.score_candidate(tr_events[0], pm, self.aliases, self.now, min_confidence=0.95)
        self.assertEqual(result["decision"], "rejected")
        self.assertIn("event_date_mismatch", result["reject_reasons"])

    def test_slug_date_allows_utc_boundary_match(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_mil_min",
            event_slug="mlb-mil-min-2026-05-15",
            condition_id="0xmilmin",
            market_id="4",
            token_ids=["mil", "min"],
            sport="mlb",
            league="mlb",
            event_title="Cincinnati Reds vs. Cleveland Guardians",
            market_title="Cincinnati Reds vs. Cleveland Guardians",
            start_time_utc="2026-05-16T00:10:00Z",
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            active=True,
            closed=False,
            raw_ref="raw:pm",
            received_at="2026-05-15T14:00:00Z",
        )
        result = live_match.score_candidate(tr_events[0], pm, self.aliases, self.now, min_confidence=0.95)
        self.assertEqual(result["event_date_match_status"], "same_date")
        self.assertEqual(result["decision"], "matched")

    def test_time_window_filters_outside_lookahead(self):
        payload = json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text())
        payload["events"] = payload["events"][:1]
        payload["events"][0]["event_date"] = "2026-05-20T23:10:00Z"
        tr_events = live_match.parse_therundown_events(
            payload,
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        self.assertEqual(tr_events, [])

    def test_multiple_candidates_ambiguity(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm_events = live_match.parse_polymarket_events(
            json.loads((ROOT / "tests/fixtures/mapping/polymarket_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref_prefix="raw:pm",
            now=self.now,
        )
        duplicate = pm_events[0]
        duplicate = live_match.ProviderMarket(**{**duplicate.__dict__, "provider_event_id": "pm_duplicate", "condition_id": "0xdup"})
        decisions = live_match.match_events(tr_events[:1], [pm_events[0], duplicate], self.aliases, self.now, 0.95, True)
        self.assertEqual(len(decisions["matched"]), 2)
        self.assertFalse(decisions["needs_review"])

    def test_output_schema_and_secret_scrub(self):
        tr_events = live_match.parse_therundown_events(
            json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )
        pm_events = live_match.parse_polymarket_events(
            json.loads((ROOT / "tests/fixtures/mapping/polymarket_live_sample.json").read_text()),
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref_prefix="raw:pm",
            now=self.now,
        )
        decisions = live_match.match_events(tr_events, pm_events, self.aliases, self.now, 0.8, True)
        with tempfile.TemporaryDirectory() as tmp:
            live_match.write_outputs(
                output_dir=Path(tmp),
                run_id="run_fixture",
                run_started_at="2026-05-15T14:00:00Z",
                therundown_events=tr_events,
                polymarket_markets=pm_events,
                decisions=decisions,
                source_status={"therundown": {"status": "ok"}, "polymarket": {"status": "ok"}},
                alias_candidates=[],
                live_blocking_items=[],
            )
            latest = json.loads((Path(tmp) / "latest.json").read_text())
            self.assertIn("mapping_id", latest[0])
            self.assertIn("canonical_market_key", latest[0])
            self.assertTrue((Path(tmp) / "latest.csv").exists())
            self.assertTrue((Path(tmp) / "latest.md").exists())
            self.assertNotRegex((Path(tmp) / "latest.md").read_text(), r"api[_-]?key|private[_-]?key|passphrase")

    def test_no_order_call_guard(self):
        with self.assertRaises(live_match.SecurityError):
            live_match.assert_safe_url("https://clob.polymarket.com/order")


if __name__ == "__main__":
    unittest.main()
