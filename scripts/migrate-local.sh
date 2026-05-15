#!/usr/bin/env bash
set -euo pipefail

COMPOSE_FILE="${COMPOSE_FILE:-deploy/docker-compose/docker-compose.yml}"
POSTGRES_DB="${POSTGRES_DB:-quantsys}"
POSTGRES_USER="${POSTGRES_USER:-quantsys}"
CLICKHOUSE_USER="${CLICKHOUSE_USER:-quantsys}"
CLICKHOUSE_PASSWORD="${CLICKHOUSE_PASSWORD:-quantsys}"

docker compose -f "${COMPOSE_FILE}" --profile local exec -T postgres \
  psql -v ON_ERROR_STOP=1 -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" \
  -f /docker-entrypoint-initdb.d/0001_init.sql

docker compose -f "${COMPOSE_FILE}" --profile local exec -T clickhouse \
  clickhouse-client --user "${CLICKHOUSE_USER}" --password "${CLICKHOUSE_PASSWORD}" \
  --multiquery < migrations/clickhouse/0001_init.sql
