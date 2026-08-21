#!/usr/bin/env bash
# Test Docker container connectivity to host services
set -euo pipefail

echo "=== Testing PostgreSQL connectivity from Docker container ==="
docker run --rm -e PGPASSWORD=Helloworld --add-host=host.docker.internal:host-gateway postgres:18-alpine \
  psql -h host.docker.internal -U sdkwork_ai_dev -d sdkwork_ai_dev -tAc "SELECT 'postgres OK' AS result;"

echo ""
echo "=== Testing Redis connectivity from Docker container ==="
docker run --rm --add-host=host.docker.internal:host-gateway redis:8-alpine \
  redis-cli -h host.docker.internal ping

echo ""
echo "=== All connectivity tests passed ==="
