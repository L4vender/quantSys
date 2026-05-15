.PHONY: contract-test test-contract therundown-test therundown-contract-test therundown-integration-test therundown-mock therundown-live-probe adapter-therundown mapping-test live-mapping fmt clippy test check compose-up compose-down migrate-local topic-init topic-init-dry-run

contract-test:
	python3 scripts/contract/check_external_api_contract.py

test-contract: contract-test

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

check: fmt clippy test contract-test therundown-test

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
