.PHONY: contract-test test-contract therundown-test therundown-contract-test therundown-integration-test therundown-mock therundown-live-probe therundown-csv-run therundown-watchlist-csv-run adapter-therundown polymarket-test polymarket-contract-test polymarket-integration-test polymarket-mock polymarket-csv-run polymarket-watchlist-csv-run adapter-polymarket-market adapter-polymarket-user polymarket-public-probe polymarket-geoblock-probe local-csv-test raw-archive-test source-health-test raw-archive-integration-test source-health-integration-test raw-archive-bench raw-archive source-health phase5-test phase5-integration-docker mapping-test live-mapping live-watchlist fmt clippy test check compose-up compose-down migrate-local topic-init topic-init-dry-run

contract-test:
	python3 scripts/contract/check_external_api_contract.py

test-contract: contract-test

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

check: contract-test fmt clippy test therundown-test polymarket-test local-csv-test phase5-test

therundown-test:
	cargo test -p quantsys-source-sdk --test therundown_unit
	cargo test -p quantsys-source-sdk --test therundown_integration
	cargo test -p adapter-therundown

therundown-contract-test: contract-test therundown-test

therundown-integration-test:
	cargo test -p quantsys-source-sdk --test therundown_integration

adapter-therundown:
	cargo build -p adapter-therundown

therundown-mock:
	cargo test -p quantsys-source-sdk --test therundown_integration mock_rest_bootstrap_publishes_raw_therundown_and_updates_cursor -- --nocapture

therundown-live-probe:
	@if [ -f .env ]; then set -a; . ./.env; set +a; fi; \
	if [ -z "$${THERUNDON_API_KEY:-}" ]; then \
		echo "THERUNDON_API_KEY is not set; skipping live TheRundown probe"; \
		exit 1; \
	fi; \
	cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode probe

therundown-csv-run:
	cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode ws --csv-output output/local-csv

therundown-watchlist-csv-run:
	cargo run -p adapter-therundown -- --config configs/sources/therundown.example.toml --mode ws --csv-output output/local-csv

polymarket-test:
	cargo test -p quantsys-source-sdk --test polymarket_unit
	cargo test -p quantsys-source-sdk --test polymarket_integration
	cargo test -p adapter-polymarket-market
	cargo test -p adapter-polymarket-user

polymarket-contract-test: contract-test polymarket-test

polymarket-integration-test:
	cargo test -p quantsys-source-sdk --test polymarket_integration

polymarket-mock:
	cargo test -p quantsys-source-sdk --test polymarket_integration market_ws_events_publish_raw_and_market_resolved_updates_source_state -- --nocapture

local-csv-test:
	cargo test -p quantsys-storage --test local_csv

raw-archive-test:
	cargo test -p quantsys-domain --test raw_phase5
	cargo test -p quantsys-storage --test raw_archive_phase5

source-health-test:
	cargo test -p source-health

raw-archive-integration-test:
	cargo test -p raw-archive --test raw_archive_flow

source-health-integration-test:
	cargo test -p source-health --test source_health_api

raw-archive-bench:
	cargo test -p raw-archive --test raw_archive_flow mock_archive_sustains_one_thousand_messages_per_second -- --nocapture

raw-archive:
	cargo run -p raw-archive

source-health:
	cargo run -p source-health

phase5-test: raw-archive-test source-health-test raw-archive-integration-test source-health-integration-test

phase5-integration-docker: compose-up migrate-local topic-init raw-archive-integration-test source-health-integration-test

polymarket-csv-run:
	cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode market-ws --csv-output output/local-csv

polymarket-watchlist-csv-run:
	cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode market-ws --csv-output output/local-csv

adapter-polymarket-market:
	cargo build -p adapter-polymarket-market

adapter-polymarket-user:
	cargo build -p adapter-polymarket-user

polymarket-public-probe:
	cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode discovery

polymarket-geoblock-probe:
	cargo run -p adapter-polymarket-market -- --config configs/sources/polymarket.example.toml --mode geoblock

compose-up:
	docker compose -f deploy/docker-compose/docker-compose.yml --profile local up -d

compose-down:
	docker compose -f deploy/docker-compose/docker-compose.yml --profile local down

migrate-local:
	bash scripts/migrate-local.sh

topic-init:
	python3 scripts/topic-init/topic_init.py

topic-init-dry-run:
	python3 scripts/topic-init/topic_init.py --dry-run

mapping-test:
	python3 -m unittest tests.mapping.test_live_match -v

live-mapping:
	python3 scripts/mapping/live_match.py --sports nba,nfl,mlb,nhl,tennis --lookahead-hours 24 --dry-run true

live-watchlist: live-mapping
