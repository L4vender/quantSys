import json
import copy
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

    def test_therundown_parser_keeps_moneyline_spread_and_total(self):
        payload = copy.deepcopy(json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()))
        payload["events"] = payload["events"][:1]
        payload["events"][0]["markets"].extend([
            {
                "market_id": 2,
                "period_id": 0,
                "name": "handicap",
                "participants": [
                    {"id": 31, "type": "TYPE_TEAM", "name": "Cincinnati Reds", "lines": [{"value": "+1.5", "prices": {"19": {"price": -110, "is_main_line": True}}}]},
                    {"id": 32, "type": "TYPE_TEAM", "name": "Cleveland Guardians", "lines": [{"value": "-1.5", "prices": {"19": {"price": -110, "is_main_line": True}}}]},
                ],
            },
            {
                "market_id": 3,
                "period_id": 0,
                "name": "totals",
                "participants": [
                    {"id": 0, "type": "TYPE_RESULT", "name": "Over", "lines": [{"value": "8.5", "prices": {"19": {"price": -115, "is_main_line": True}}}]},
                    {"id": 1, "type": "TYPE_RESULT", "name": "Under", "lines": [{"value": "8.5", "prices": {"19": {"price": -105, "is_main_line": True}}}]},
                ],
            },
        ])

        tr_events = live_match.parse_therundown_events(
            payload,
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )

        self.assertEqual([event.market_type_raw for event in tr_events], ["moneyline", "spread", "total"])
        self.assertEqual([event.line for event in tr_events], [None, 1.5, 8.5])
        self.assertEqual(tr_events[2].participants_raw, ["Cincinnati Reds", "Cleveland Guardians"])

    def test_therundown_parser_expands_multiple_distinct_spread_lines(self):
        payload = copy.deepcopy(json.loads((ROOT / "tests/fixtures/mapping/therundown_live_sample.json").read_text()))
        payload["events"] = payload["events"][:1]
        payload["events"][0]["markets"] = [{
            "market_id": 2,
            "period_id": 0,
            "name": "handicap",
            "participants": [
                {"id": 31, "type": "TYPE_TEAM", "name": "Cincinnati Reds", "lines": [
                    {"value": "+1.5", "prices": {"19": {"price": -110}}},
                    {"value": "+2.5", "prices": {"19": {"price": -125}}},
                ]},
                {"id": 32, "type": "TYPE_TEAM", "name": "Cleveland Guardians", "lines": [
                    {"value": "-1.5", "prices": {"19": {"price": -110}}},
                    {"value": "-2.5", "prices": {"19": {"price": 105}}},
                ]},
            ],
        }]

        tr_events = live_match.parse_therundown_events(
            payload,
            sport="mlb",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr",
            now=self.now,
            lookahead_hours=24,
        )

        self.assertEqual([event.line for event in tr_events], [1.5, 2.5])
        self.assertTrue(all(event.market_id == "2" for event in tr_events))

    def test_spread_and_total_match_by_team_and_date_without_home_away_gate(self):
        tr_events = [
            live_match.ProviderEvent(
                provider="therundown",
                provider_event_id="tr_cin_cle_spread",
                sport="mlb",
                league="mlb",
                event_name="Cincinnati at Cleveland",
                start_time_utc="2026-05-15T23:10:00Z",
                home_team_raw="Cleveland Guardians",
                away_team_raw="Cincinnati Reds",
                participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
                market_type_raw="spread",
                outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
                source_timestamp="2026-05-15T14:00:00Z",
                received_at="2026-05-15T14:00:00Z",
                raw_ref="raw:tr:spread",
                line=1.5,
            ),
            live_match.ProviderEvent(
                provider="therundown",
                provider_event_id="tr_cin_cle_total",
                sport="mlb",
                league="mlb",
                event_name="Cincinnati at Cleveland",
                start_time_utc="2026-05-15T23:10:00Z",
                home_team_raw="Cleveland Guardians",
                away_team_raw="Cincinnati Reds",
                participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
                market_type_raw="total",
                outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
                source_timestamp="2026-05-15T14:00:00Z",
                received_at="2026-05-15T14:00:00Z",
                raw_ref="raw:tr:total",
                line=8.5,
            ),
        ]
        pm_markets = [
            live_match.ProviderMarket(
                provider="polymarket",
                provider_event_id="pm_cin_cle_spread",
                event_slug="mlb-cin-cle-2026-05-15",
                condition_id="0xspread",
                market_id="pm_spread",
                token_ids=["spread_yes", "spread_no"],
                sport="mlb",
                league="mlb",
                event_title="Cleveland Guardians vs. Cincinnati Reds",
                market_title="Cincinnati Reds +1.5 vs Cleveland Guardians -1.5",
                start_time_utc="2026-05-15T03:00:00Z",
                participants_raw=["Cleveland Guardians", "Cincinnati Reds"],
                outcome_names_raw=["Cleveland Guardians", "Cincinnati Reds"],
                active=True,
                closed=False,
                raw_ref="raw:pm:spread",
                received_at="2026-05-15T14:00:00Z",
                home_team_raw=None,
                away_team_raw=None,
                home_away_status="unknown",
                market_type_raw="spread",
                line=1.5,
            ),
            live_match.ProviderMarket(
                provider="polymarket",
                provider_event_id="pm_cin_cle_total",
                event_slug="mlb-cin-cle-2026-05-15",
                condition_id="0xtotal",
                market_id="pm_total",
                token_ids=["over", "under"],
                sport="mlb",
                league="mlb",
                event_title="Cleveland Guardians vs. Cincinnati Reds",
                market_title="Cincinnati Reds vs Cleveland Guardians total 8.5",
                start_time_utc="2026-05-15T03:00:00Z",
                participants_raw=["Cleveland Guardians", "Cincinnati Reds"],
                outcome_names_raw=["Over", "Under"],
                active=True,
                closed=False,
                raw_ref="raw:pm:total",
                received_at="2026-05-15T14:00:00Z",
                home_team_raw=None,
                away_team_raw=None,
                home_away_status="unknown",
                market_type_raw="total",
                line=8.5,
            ),
        ]

        decisions = live_match.match_events(tr_events, pm_markets, self.aliases, self.now, 0.95, True)

        self.assertEqual({item["market_type"] for item in decisions["matched"]}, {"spread", "total"})
        self.assertEqual({item["polymarket_condition_id"] for item in decisions["matched"]}, {"0xspread", "0xtotal"})
        self.assertTrue(all(item["home_away_status"] == "unknown" for item in decisions["matched"]))

    def test_spread_and_total_candidates_require_same_line(self):
        tr = live_match.ProviderEvent(
            provider="therundown",
            provider_event_id="tr_cin_cle_spread",
            sport="mlb",
            league="mlb",
            event_name="Cincinnati at Cleveland",
            start_time_utc="2026-05-15T23:10:00Z",
            home_team_raw="Cleveland Guardians",
            away_team_raw="Cincinnati Reds",
            participants_raw=["Cincinnati Reds", "Cleveland Guardians"],
            market_type_raw="spread",
            outcome_names_raw=["Cincinnati Reds", "Cleveland Guardians"],
            source_timestamp="2026-05-15T14:00:00Z",
            received_at="2026-05-15T14:00:00Z",
            raw_ref="raw:tr:spread",
            line=1.5,
            market_id="2",
        )
        pm = live_match.ProviderMarket(
            provider="polymarket",
            provider_event_id="pm_cin_cle_spread",
            event_slug="mlb-cin-cle-2026-05-15",
            condition_id="0xspread",
            market_id="pm_spread",
            token_ids=["spread_yes", "spread_no"],
            sport="mlb",
            league="mlb",
            event_title="Cleveland Guardians vs. Cincinnati Reds",
            market_title="Cincinnati Reds +2.5 vs Cleveland Guardians -2.5",
            start_time_utc="2026-05-15T03:00:00Z",
            participants_raw=["Cleveland Guardians", "Cincinnati Reds"],
            outcome_names_raw=["Cleveland Guardians", "Cincinnati Reds"],
            active=True,
            closed=False,
            raw_ref="raw:pm:spread",
            received_at="2026-05-15T14:00:00Z",
            market_type_raw="spread",
            line=2.5,
        )

        self.assertFalse(live_match.candidate_allowed(tr, pm, self.aliases, self.now, 24))

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
            user_markets = json.loads((Path(tmp) / "polymarket_user_markets.json").read_text())
            self.assertEqual(user_markets["condition_ids"], ["0xfixturecincle", "0xfixturelalbos"])
            self.assertNotRegex((Path(tmp) / "latest.md").read_text(), r"(?i)api[_-]?key|private[_-]?key|passphrase")

    def test_ws_watchlist_selects_one_market_per_event_type_by_count_then_median_line(self):
        base = {
            "decision": "matched",
            "confidence": 0.99,
            "canonical_event_id": "nba:los_angeles_lakers_at_boston_celtics:2026-05-16",
            "sport": "nba",
            "league": "nba",
            "therundown_event_name": "Los Angeles Lakers vs Boston Celtics",
            "polymarket_event_title": "Los Angeles Lakers vs Boston Celtics",
            "start_time_therundown": "2026-05-16T00:00:00Z",
            "start_time_polymarket": "2026-05-16T00:00:00Z",
            "therundown_event_id": "tr_lal_bos",
            "polymarket_event_id": "pm_lal_bos",
        }
        decisions = {
            "matched": [
                {
                    **base,
                    "market_type": "moneyline",
                    "line": None,
                    "therundown_market_id": "1",
                    "polymarket_condition_id": "pm_moneyline",
                    "polymarket_market_id": "pm_market_moneyline",
                    "polymarket_asset_ids": ["pm_moneyline_yes", "pm_moneyline_no"],
                },
                {
                    **base,
                    "market_type": "spread",
                    "line": 1.5,
                    "therundown_market_id": "2",
                    "polymarket_condition_id": "pm_spread_1_5",
                    "polymarket_market_id": "pm_market_spread_1_5",
                    "polymarket_asset_ids": ["pm_spread_1_5_yes", "pm_spread_1_5_no"],
                },
                {
                    **base,
                    "market_type": "spread",
                    "line": 2.5,
                    "therundown_market_id": "2",
                    "polymarket_condition_id": "pm_spread_2_5_a",
                    "polymarket_market_id": "pm_market_spread_2_5_a",
                    "polymarket_asset_ids": ["pm_spread_2_5_a_yes", "pm_spread_2_5_a_no"],
                },
                {
                    **base,
                    "market_type": "spread",
                    "line": 2.5,
                    "therundown_market_id": "2",
                    "polymarket_condition_id": "pm_spread_2_5_b",
                    "polymarket_market_id": "pm_market_spread_2_5_b",
                    "polymarket_asset_ids": ["pm_spread_2_5_b_yes", "pm_spread_2_5_b_no"],
                },
                {
                    **base,
                    "market_type": "total",
                    "line": 219.5,
                    "therundown_market_id": "3",
                    "polymarket_condition_id": "pm_total_219_5",
                    "polymarket_market_id": "pm_market_total_219_5",
                    "polymarket_asset_ids": ["pm_total_219_5_over", "pm_total_219_5_under"],
                },
                {
                    **base,
                    "market_type": "total",
                    "line": 221.5,
                    "therundown_market_id": "3",
                    "polymarket_condition_id": "pm_total_221_5",
                    "polymarket_market_id": "pm_market_total_221_5",
                    "polymarket_asset_ids": ["pm_total_221_5_over", "pm_total_221_5_under"],
                },
                {
                    **base,
                    "market_type": "total",
                    "line": 223.5,
                    "therundown_market_id": "3",
                    "polymarket_condition_id": "pm_total_223_5",
                    "polymarket_market_id": "pm_market_total_223_5",
                    "polymarket_asset_ids": ["pm_total_223_5_over", "pm_total_223_5_under"],
                },
            ]
        }

        watchlist = live_match.build_ws_watchlist(decisions, "run_1", "2026-05-15T14:00:00Z")

        self.assertEqual(watchlist["schema_version"], "quantsys.ws_watchlist.v1")
        selected = {(item["market_type"], item["line"]): item for item in watchlist["items"]}
        self.assertEqual(set(selected), {("moneyline", None), ("spread", 2.5), ("total", 221.5)})
        self.assertEqual(selected[("spread", 2.5)]["selection_reason"], "max_market_count")
        self.assertEqual(selected[("total", 221.5)]["selection_reason"], "median_line_tie_break")
        self.assertEqual(watchlist["therundown"]["event_ids"], ["tr_lal_bos"])
        self.assertEqual(watchlist["therundown"]["market_ids"], [1, 2, 3])
        self.assertEqual(
            watchlist["polymarket"]["asset_ids"],
            [
                "pm_moneyline_yes",
                "pm_moneyline_no",
                "pm_spread_2_5_a_yes",
                "pm_spread_2_5_a_no",
                "pm_total_221_5_over",
                "pm_total_221_5_under",
            ],
        )
        self.assertEqual(selected[("spread", 2.5)]["matched_market_count"], 2)

    def test_no_order_call_guard(self):
        with self.assertRaises(live_match.SecurityError):
            live_match.assert_safe_url("https://clob.polymarket.com/order")


if __name__ == "__main__":
    unittest.main()
