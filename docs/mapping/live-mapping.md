# Live Mapping Chain

This document describes the realtime TheRundown to Polymarket event matching CLI added for the Phase 6 mapping baseline.

## Command

Run the fixture regression suite:

```bash
make mapping-test
```

Run realtime discovery and mapping:

```bash
make live-mapping
```

`make live-mapping` calls TheRundown V2 current events and Polymarket public Gamma sports events. It does not use fixtures as primary input, does not call Polymarket CLOB order endpoints, does not read private keys, and does not create order intents.

## Data Flow

1. TheRundown V2 `/sports/{sportID}/events/{date}` is called with `market_ids=1`, `main_line=true`, configured affiliate filters, and `X-TheRundown-Key`.
2. Polymarket Gamma `/sports` is called to discover sport tag IDs, then `/events?active=true&closed=false&tag_id=...` is paginated for selected sports.
3. Provider events are normalized for sport, league, participants, market type, period, and start time.
4. Polymarket candidates must be concrete league game markets, such as `Team A vs. Team B`. Series winners, futures, season totals, props, awards, and championship markets are excluded before scoring.
5. Candidates are generated when sport aligns, the TheRundown event is inside the lookahead window, the Polymarket concrete game has the same event date when known, and the two team/player names are similar.
6. Candidates are scored by event date and team/player names, with market type and home/away retained as audit fields, then classified as `matched`, `needs_review`, or `rejected`.
6. Reports are written under `output/live-mapping/`.

## Confidence Formula

`confidence = 0.20 * event_date_score + 0.80 * participant_score`

Default decision thresholds:

- `confidence >= 0.95` and no review/reject reason: `matched`
- `0.80 <= confidence < 0.95`: `needs_review`
- `confidence < 0.80`: `rejected`

Candidate exclusion includes Polymarket series winners, futures, season totals, props, awards, championship markets, inactive markets, and closed markets. Hard rejects include sport mismatch, different event dates when both dates are known, and team/player name mismatch. Exact start-time delta and home/away status do not block event-level matching.

## Home/Away Handling

TheRundown exposes explicit home/away teams. Polymarket Gamma often exposes only a title and outcomes. When Polymarket does not provide explicit home/away fields, the mapping keeps `home_away_status=unknown`; the CLI never guesses home/away from title or outcome order.

If explicit Polymarket home/away fields are present:

- aligned: TheRundown home equals Polymarket home and TheRundown away equals Polymarket away.
- reversed: TheRundown home equals Polymarket away and TheRundown away equals Polymarket home.
- reversed mappings are never silently corrected. Under the simplified date/team-name matcher, reversed status is audit metadata rather than a matching blocker.

## Output

The realtime run writes:

- `latest.json`
- `latest.csv`
- `latest.md`
- `unmatched_therundown.json`
- `unmatched_polymarket.json`
- `needs_review.json`
- `rejected.json`
- `alias_candidates.json`
- `source_status.json`

`latest.md` is the operator-readable audit report. `latest.json` is the downstream baseline for later normalizer/canonical-mapper persistence.

## Phase Integration

Future Phase 6 work can move the Python DTOs into Rust `domain` mapping structs and persist mapping decisions. Future Phase 8 dry-run can add stricter live-readiness gates for market type, unknown home/away, reversed home/away, and exact start-time alignment without changing this event-level discovery baseline.
