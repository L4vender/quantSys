.PHONY: contract-test test-contract mapping-test live-mapping fmt clippy test compose-up compose-down migrate-local topic-init topic-init-dry-run

contract-test:
	python3 scripts/contract/check_external_api_contract.py

test-contract: contract-test

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

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
