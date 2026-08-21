#!/usr/bin/env bash
# Configure WSL host-native external PostgreSQL and Redis for SDKWork Web Server
# Docker external mode deployment per sdkwork-specs DEPLOYLOYMENT_SPEC.md.
#
# This script runs INSIDE the WSL Ubuntu 22.04 host (not inside a container).
# It provisions:
#   - PostgreSQL workspace databases (sdkwork_ai_dev/test/prod) with password
#   - Redis with password (Helloworld)
#   - Docker network access for both services
#
# Usage (inside WSL):
#   sudo bash deployments/docker/scripts/configure-wsl-external-deps.sh
set -euo pipefail

log() {
  echo "[configure-wsl-external-deps] $*"
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/configure-wsl-external-deps.sh"
    exit 1
  fi
}

# PostgreSQL: provision workspace identity per sdkwork-specs ENVIRONMENT_SPEC
# Each identity: database name = schema name = username
provision_postgres_identity() {
  local db_name="$1"
  local db_password="$2"

  log "provisioning PostgreSQL identity: ${db_name}"

  # Create role with password (or update existing)
  su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"CREATE ROLE ${db_name} LOGIN PASSWORD '${db_password}';\"" 2>/dev/null || \
  su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"ALTER ROLE ${db_name} WITH LOGIN PASSWORD '${db_password}';\""

  # Create database if not exists
  if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_database WHERE datname='${db_name}';\"" | grep -q 1; then
    su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"CREATE DATABASE ${db_name} OWNER ${db_name};\""
    log "  created database: ${db_name}"
  else
    log "  database already exists: ${db_name}"
  fi

  # Grant privileges on database
  su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"GRANT ALL PRIVILEGES ON DATABASE ${db_name} TO ${db_name};\""

  # Create schema and grant privileges
  su - postgres -c "psql -v ON_ERROR_STOP=1 -d ${db_name} -c \"CREATE SCHEMA IF NOT EXISTS ${db_name} AUTHORIZATION ${db_name};\""
  su - postgres -c "psql -v ON_ERROR_STOP=1 -d ${db_name} -c \"GRANT ALL ON SCHEMA ${db_name} TO ${db_name};\""
  su - postgres -c "psql -v ON_ERROR_STOP=1 -d ${db_name} -c \"GRANT CREATE ON SCHEMA ${db_name} TO ${db_name};\""
  su - postgres -c "psql -v ON_ERROR_STOP=1 -d ${db_name} -c \"ALTER ROLE ${db_name} SET search_path TO ${db_name};\""

  log "  provisioned: db=${db_name} schema=${db_name}"
}

# PostgreSQL: ensure pg_hba.conf allows Docker bridge networks
ensure_pg_hba_docker_access() {
  local hba_file
  hba_file="$(su - postgres -c "psql -tAc \"SHOW hba_file;\"" | tr -d '[:space:]')"

  if [ ! -f "${hba_file}" ]; then
    log "warning: could not resolve pg_hba.conf (${hba_file})"
    return 0
  fi

  if grep -Eq '^host\s+all\s+all\s+172\.(1[6-9]|2[0-9]|3[0-1])\.' "${hba_file}"; then
    log "pg_hba already allows docker bridge networks"
    return 0
  fi

  {
    echo ""
    echo "# sdkwork-webserver docker external mode (host.docker.internal)"
    echo "host    all             all             172.16.0.0/12           scram-sha-256"
    echo "host    all             all             127.0.0.1/32            scram-sha-256"
  } >> "${hba_file}"

  systemctl reload postgresql || service postgresql reload
  log "updated ${hba_file} for docker bridge access"
}

# Redis: configure password and listening
configure_redis() {
  local redis_password="$1"
  local redis_conf="/etc/redis/redis.conf"

  log "configuring Redis..."

  # Ensure Redis listens on all interfaces
  if grep -q "^bind " "${redis_conf}"; then
    sed -i 's/^bind .*/bind * -::*/' "${redis_conf}"
  else
    echo "bind * -::*" >> "${redis_conf}"
  fi

  # Set password
  if grep -q "^requirepass " "${redis_conf}"; then
    sed -i "s/^requirepass .*/requirepass ${redis_password}/" "${redis_conf}"
  else
    echo "requirepass ${redis_password}" >> "${redis_conf}"
  fi

  # Disable protected mode for Docker bridge access
  if grep -q "^protected-mode " "${redis_conf}"; then
    sed -i 's/^protected-mode .*/protected-mode no/' "${redis_conf}"
  else
    echo "protected-mode no" >> "${redis_conf}"
  fi

  # Restart Redis to apply configuration
  systemctl restart redis-server
  sleep 2

  # Verify Redis is responding with password
  if redis-cli -a "${redis_password}" ping 2>/dev/null | grep -q PONG; then
    log "Redis configured successfully with password auth"
  else
    log "warning: Redis CLI check inconclusive, checking service..."
    systemctl is-active redis-server
    log "Redis service is active"
  fi
}

# Verify PostgreSQL connectivity from Docker
verify_postgres_from_docker() {
  local db_password="$1"

  log "verifying PostgreSQL connectivity from Docker container..."

  # Install postgres client in temp container and test connection
  if docker run --rm --add-host=host.docker.internal:host-gateway postgres:18-alpine \
    sh -c "PGPASSWORD='${db_password}' psql -h host.docker.internal -U sdkwork_ai_dev -d sdkwork_ai_dev -tAc 'SELECT 1;' 2>/dev/null" | grep -q 1; then
    log "PostgreSQL reachable from container at host.docker.internal:5432"
  else
    log "warning: could not verify PostgreSQL connectivity (may need port exposure)"
  fi
}

# Verify Redis connectivity from Docker
verify_redis_from_docker() {
  local redis_password="$1"

  log "verifying Redis connectivity from Docker container..."

  if docker run --rm --add-host=host.docker.internal:host-gateway redis:8-alpine \
    sh -c "redis-cli -h host.docker.internal -a '${redis_password}' ping" 2>/dev/null | grep -q PONG; then
    log "Redis reachable from container at host.docker.internal:6379"
  else
    log "warning: could not verify Redis connectivity (may need port exposure)"
  fi
}

main() {
  require_root

  local PASSWORD="Helloworld"

  log "============================================"
  log "SDKWork Web Server - WSL External Deps Config"
  log "Ubuntu password: ${PASSWORD}"
  log "============================================"

  echo ""

  # 1. Configure PostgreSQL
  log "--- PostgreSQL Configuration ---"
  provision_postgres_identity "sdkwork_ai_dev" "${PASSWORD}"
  provision_postgres_identity "sdkwork_ai_test" "${PASSWORD}"
  provision_postgres_identity "sdkwork_ai_prod" "${PASSWORD}"

  # Set postgres superuser password too
  su - postgres -c "psql -v ON_ERROR_STOP=1 -c \"ALTER USER postgres WITH PASSWORD '${PASSWORD}';\""
  log "set postgres superuser password"

  ensure_pg_hba_docker_access

  echo ""

  # 2. Configure Redis
  log "--- Redis Configuration ---"
  configure_redis "${PASSWORD}"

  echo ""

  # 3. Verify connectivity
  log "--- Connectivity Verification ---"
  verify_postgres_from_docker "${PASSWORD}"
  verify_redis_from_docker "${PASSWORD}"

  echo ""
  log "============================================"
  log "Configuration complete!"
  log ""
  log "PostgreSQL:"
  log "  Host: host.docker.internal (port 5432)"
  log "  Databases: sdkwork_ai_dev, sdkwork_ai_test, sdkwork_ai_prod"
  log "  Username: <dbname> (e.g., sdkwork_ai_dev)"
  log "  Password: ${PASSWORD}"
  log ""
  log "Redis:"
  log "  Host: host.docker.internal (port 6379)"
  log "  Password: ${PASSWORD}"
  log "  DB indices: 0 (dev), 1 (test), 2 (prod)"
  log "============================================"
}

main "$@"
