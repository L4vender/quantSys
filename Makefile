.PHONY: contract-test test-contract mapping-test live-mapping

contract-test:
	python3 scripts/contract/check_external_api_contract.py

test-contract: contract-test

mapping-test:
	python3 -m unittest tests.mapping.test_live_match -v

live-mapping:
	python3 scripts/mapping/live_match.py --sports nba,nfl,mlb,nhl,tennis --lookahead-hours 24 --dry-run true
