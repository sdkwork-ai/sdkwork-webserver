#!/usr/bin/env bash
# Test Docker container connectivity to host-native external PostgreSQL and Redis.
set -euo pipefail

PG_USER="${WEBSERVER_POSTGRES_DEV_USER:-sdkwork_ai_dev}"
PG_PASSWORD="${WEBSERVER_POSTGRES_DEV_PASSWORD:-sdkworkdev123}"

echo "=== Testing PostgreSQL connectivity from Docker container ==="
docker run --rm -e "PGPASSWORD=${PG_PASSWORD}" --add-host=host.docker.internal:host-gateway postgres:18-alpine \
  psql -h host.docker.internal -U "${PG_USER}" -d "${PG_USER}" -tAc "SELECT 'postgres OK' AS result;"

echo ""
echo "=== Testing Redis connectivity from Docker container (passwordless) ==="
docker run --rm --add-host=host.docker.internal:host-gateway redis:8-alpine \
  redis-cli -h host.docker.internal ping

echo ""
echo "=== All connectivity tests passed ==="
