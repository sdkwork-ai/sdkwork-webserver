#!/usr/bin/env bash
# Configure WSL host-native external PostgreSQL and Redis for SDKWork Web Server
# Docker external mode deployment per sdkwork-specs ENVIRONMENT_SPEC §7.1.
#
# This script runs INSIDE the WSL Ubuntu host (not inside a container).
# It provisions:
#   - PostgreSQL workspace databases (sdkwork_ai_dev/test/prod) with canonical passwords
#   - Passwordless Redis for Docker bridge access via host.docker.internal
#
# Usage (inside WSL):
#   sudo bash deployments/docker/scripts/configure-wsl-external-deps.sh
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
# shellcheck source=ensure-host-redis.sh
source "$script_dir/ensure-host-redis.sh"

log() {
  echo "[configure-wsl-external-deps] $*"
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/configure-wsl-external-deps.sh"
    exit 1
  fi
}

verify_postgres_from_docker() {
  local db_user="$1"
  local db_password="$2"

  log "verifying PostgreSQL connectivity from Docker container..."

  if docker run --rm --add-host=host.docker.internal:host-gateway postgres:18-alpine \
    sh -c "PGPASSWORD='${db_password}' psql -h host.docker.internal -U ${db_user} -d ${db_user} -tAc 'SELECT 1;' 2>/dev/null" | grep -q 1; then
    log "PostgreSQL reachable from container at host.docker.internal:5432"
  else
    log "warning: could not verify PostgreSQL connectivity (may need port exposure)"
  fi
}

verify_redis_from_docker() {
  log "verifying Redis connectivity from Docker container..."

  if docker run --rm --add-host=host.docker.internal:host-gateway redis:8-alpine \
    sh -c "redis-cli -h host.docker.internal ping" 2>/dev/null | grep -q PONG; then
    log "Redis reachable from container at host.docker.internal:6379 (no password)"
  else
    log "warning: could not verify Redis connectivity (may need port exposure)"
  fi
}

main() {
  require_root

  log "============================================"
  log "SDKWork Web Server - WSL External Deps Config"
  log "============================================"

  echo ""

  log "--- Redis Configuration (passwordless) ---"
  ensure_host_redis

  echo ""

  log "--- PostgreSQL Configuration ---"
  bash "$script_dir/setup-host-external-deps.sh"

  echo ""

  log "--- Connectivity Verification ---"
  verify_postgres_from_docker "sdkwork_ai_dev" "sdkworkdev123"
  verify_redis_from_docker

  echo ""
  log "============================================"
  log "Configuration complete!"
  log ""
  log "PostgreSQL:"
  log "  Host: host.docker.internal (port 5432)"
  log "  Databases: sdkwork_ai_dev, sdkwork_ai_test, sdkwork_ai_prod"
  log "  Username: <dbname> (e.g., sdkwork_ai_dev)"
  log "  Passwords: sdkworkdev123 / sdkworktest123 / sdkworkprod123"
  log ""
  log "Redis:"
  log "  Host: host.docker.internal (port 6379)"
  log "  Password: (none)"
  log "  DB indices: 0 (dev), 1 (test), 2 (prod)"
  log "============================================"
}

main "$@"
