# Local CSV Output

## Purpose

Local CSV output is a lightweight observation aid for Phase 3 and Phase 4 ingestion. It saves TheRundown and Polymarket market data to local append-only CSV files so one game and one market can be watched over time.

This is not Phase 5 raw archive, not Phase 6 canonical mapping, and not strategy logic. The values are converted into Polymarket-style probability format only for visual comparison. They are not edge, signal, order intent, risk decision, or execution input.

## Directory Layout

Default base directory:

```text
output/local-csv/
├─ therundown/
│  ├─ nba/
│  │  ├─ draftkings/
│  │  └─ fanduel/
│  ├─ nfl/
│  ├─ mlb/
│  └─ nhl/
├─ polymarket/
│  ├─ nba/
│  ├─ nfl/
│  ├─ mlb/
│  └─ nhl/
└─ _index/
   ├─ markets_index.csv
   ├─ latest_files.json
   └─ latest/
```

`output/local-csv/` is ignored by git. Tests write to temporary directories.

## File Naming

Each file represents one game plus one market:

```text
{event_start_time}_{team_a}_vs_{team_b}_{market}.csv
```

Examples:

```text
2026-05-16T233000Z_lakers_vs_warriors_moneyline.csv
2026-05-16T233000Z_lakers_vs_warriors_spread_minus_5_5.csv
2026-05-16T233000Z_lakers_vs_warriors_total_221_5.csv
```

Names are lower case, path-safe, stable, sortable, and truncated with a hash suffix when team names are long. Missing start time becomes `unknown_time`. One market writes to one file; a different spread or total line writes to a different file.

## CSV Columns

Provider CSV files contain exactly:

```text
data_generated_at,data_fetched_at,bookmaker,affiliate_id,team_a_polymarket_format,team_b_polymarket_format
```

Meanings:

- `data_generated_at`: provider payload timestamp. TheRundown uses REST bootstrap or WS price `updated_at`, then `meta.timestamp` when available. Polymarket uses WS `timestamp`.
- `data_fetched_at`: local wall-clock time when this process reads the HTTP response or WebSocket text message, stored as `RawMessage.received_at`.
- `bookmaker`: provider/bookmaker display label. TheRundown maps affiliate ids through the official Sportsbook / Affiliate IDs table, for example `19=DraftKings`, `23=FanDuel`; Polymarket uses `Polymarket`.
- `affiliate_id`: TheRundown affiliate id when available. Empty for Polymarket.
- `team_a_polymarket_format`: team A price converted to Polymarket-style decimal probability, for example `0.541284`.
- `team_b_polymarket_format`: team B price converted to Polymarket-style decimal probability.

TheRundown American odds are converted to decimal implied probability. Polymarket prices are already decimal; when bid and ask are both present, the midpoint is used for the observation row. If only one side is present, the latest local snapshot for the other side is reused when available.

## How It Writes

`LocalCsvSink` lives in `crates/storage/src/local_csv.rs`.

1. It receives the existing Phase 3/4 `RawMessage` after raw publish succeeds.
2. It extracts one local observation record.
3. It writes TheRundown under `output/local-csv/therundown/{league}/{bookmaker_slug}/` and Polymarket under `output/local-csv/polymarket/{league}/`.
4. It appends to `{time}_{teams}_{market}.csv`.
5. It stores a small latest snapshot under `_index/latest/` so subsequent rows can contain both team columns.
6. It updates `_index/markets_index.csv` and `_index/latest_files.json`.

CSV headers are written once. Rows append and do not overwrite history.

TheRundown rows are written from the startup REST bootstrap snapshot and from later WS market-price updates. The bootstrap response can arrive in either the fixture-style `home_team`/`away_team` shape or the live `teams[] -> markets[].participants[].lines[].prices` shape; both are parsed for league, teams, event time, market type, line, affiliate price, and source update time. Each affiliate/bookmaker writes to a separate folder and keeps an independent team A/B snapshot, so prices from different companies are not merged into one row. WS market-price messages are enriched from that bootstrap event cache. Heartbeats and unenriched WS messages remain in the raw publish path but are skipped for local CSV so they do not create `unknown_league`, `unknown_time`, or `unknown_event` files.

The current built-in sportsbook mapping is:

| Affiliate ID | Bookmaker |
|---|---|
| 2 | Bovada |
| 3 | Pinnacle |
| 4 | Sportsbetting |
| 6 | BetOnline |
| 11 | LowVig |
| 12 | Bodog |
| 14 | Intertops |
| 16 | Matchbook |
| 18 | YouWager |
| 19 | DraftKings |
| 21 | Unibet |
| 22 | BetMGM |
| 23 | FanDuel |
| 24 | theScore Bet |
| 25 | Kalshi |
| 26 | Polymarket |

Unknown ids are still preserved as `Affiliate {id}` in the `bookmaker` column and `affiliate_{id}` in the folder so no data is dropped.

Polymarket rows require discovery metadata. The market WS payload itself often has only condition id, token/asset id, timestamp, and price fields. The Polymarket market adapter first discovers Games-tag markets, stores event/team/time/market metadata in the token cache, subscribes with `assets_ids`, and then enriches WS raw messages before local CSV write. WS messages that cannot be matched to discovery metadata are still available in the raw publish path but are skipped for local CSV to avoid misleading `unknown_sport` or `unknown_time` files.

## Enabling Output

Config default:

```toml
[local_csv]
enabled = false
base_dir = "output/local-csv"
flush_every_rows = 1
rotate_daily = false
include_raw_refs = true
write_single_provider_files = true
write_comparison_files = false
```

Manual CLI override:

```bash
cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode ws --csv-output output/local-csv
cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode market-ws --csv-output output/local-csv
```

Make targets:

```bash
make local-csv-test
make therundown-csv-run
make polymarket-csv-run
```

The run targets may require live credentials or network and are not part of CI. `local-csv-test` uses fixtures and temporary directories.

## Secret Safety

CSV output never writes auth headers, API keys, Polymarket secrets, passphrases, private keys, signatures, or user auth payloads. Values that contain secret-like labels are redacted before CSV/index writes. Raw payloads are not embedded in CSV files.

## Cleaning

```bash
rm -rf output/local-csv
```

## Phase Boundaries

The local CSV file key is only for manual observation. It must not be treated as the Phase 6 canonical mapping. This output must not generate strategy decisions, `signal.event`, `order.intent`, `risk.decision`, signed orders, or execution requests.
