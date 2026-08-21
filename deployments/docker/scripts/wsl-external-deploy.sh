#!/usr/bin/env bash
# Provision WSL host-native external deps and deploy all sdkwork-webserver
# environments (external mode). Modeled on sdkwork-api-cloud-gateway.
#
# This script:
#   1. Ensures host Redis is running and accessible
#   2. Provisions PostgreSQL databases for each environment
#   3. Stops any existing embedded stacks
#   4. Deploys all three environments (development, test, production)
#   5. Configures /etc/hosts and nginx for domain routing
#   6. Verifies all endpoints are healthy
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
docker_root="$repo_root/deployments/docker"

log() { echo "[wsl-external-deploy] $*"; }

ensure_host_redis() {
  if command -v redis-cli >/dev/null 2>&1 && redis-cli ping >/dev/null 2>&1; then
    log "host redis already running"
    return 0
  fi

  if ! command -v redis-server >/dev/null 2>&1; then
    log "installing redis-server"
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq redis-server
  fi

  # Ensure no password and listen on all interfaces for host.docker.internal access.
  sed -i 's/^# requirepass .*/requirepass ""/' /etc/redis/redis.conf 2>/dev/null || true
  sed -i 's/^requirepass .*/requirepass ""/' /etc/redis/redis.conf 2>/dev/null || true
  sed -i 's/^bind 127.0.0.1 .*/bind 0.0.0.0 ::1/' /etc/redis/redis.conf 2>/dev/null || true
  sed -i 's/^protected-mode yes/protected-mode no/' /etc/redis/redis.conf 2>/dev/null || true

  systemctl enable redis-server
  systemctl restart redis-server
  sleep 1
  redis-cli ping
  log "host redis ready on 6379 (no password)"
}

provision_identity() {
  local db_name="$1"
  local db_user="$2"
  local db_password="$3"

  log "provisioning postgres identity: ${db_name}"

  sudo -u postgres psql -v ON_ERROR_STOP=1 <<-EOSQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '${db_user}') THEN
    EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', '${db_user}', '${db_password}');
  ELSE
    EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L', '${db_user}', '${db_password}');
  END IF;
END
\$\$;

SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE datname = '${db_name}' AND pid <> pg_backend_pid();

DROP DATABASE IF EXISTS "${db_name}";
CREATE DATABASE "${db_name}" OWNER "${db_user}";
GRANT ALL PRIVILEGES ON DATABASE "${db_name}" TO "${db_user}";
EOSQL

  sudo -u postgres psql -v ON_ERROR_STOP=1 -d "${db_name}" <<-EOSQL
DROP SCHEMA IF EXISTS "${db_name}" CASCADE;
CREATE SCHEMA "${db_name}" AUTHORIZATION "${db_user}";
GRANT ALL ON SCHEMA "${db_name}" TO "${db_user}";
GRANT CREATE ON SCHEMA "${db_name}" TO "${db_user}";
ALTER ROLE "${db_user}" SET search_path TO "${db_name}";
EOSQL

  log "provisioned: db=${db_name} user=${db_user} schema=${db_name}"
}

ensure_pg_hba_docker_access() {
  local hba_file
  hba_file="$(sudo -u postgres psql -tAc "SHOW hba_file;" | tr -d '[:space:]')"
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

stop_existing_stacks() {
  log "stopping existing webserver stacks"
  for project in sdkwork-webserver-development sdkwork-webserver-test sdkwork-webserver-production; do
    docker compose -p "$project" down --remove-orphans 2>/dev/null || true
  done
  # Also stop old-style compose projects
  for env_name in development test production; do
    for mode in "" "-external"; do
      docker compose -p "sdkwork-webserver-${env_name}${mode}" \
        -f "$docker_root/docker-compose.yml" \
        ${mode:+-f "$docker_root/docker-compose.external.yml"} \
        down --remove-orphans 2>/dev/null || true
    done
  done
}

deploy_environment() {
  local env_name="$1"
  bash "$repo_root/scripts/docker/deploy-docker-environment.sh" "$env_name" --validate
}

verify_environment() {
  local env_name="$1"
  local port="$2"
  local domain="$3"

  log "verifying $env_name..."
  if curl -fsS "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
    log "  direct port ${port}: OK"
  else
    log "  direct port ${port}: FAILED"
    return 1
  fi
  if curl -fsS -H "Host: ${domain}" "http://127.0.0.1/healthz" >/dev/null 2>&1; then
    log "  domain ${domain}: OK"
  else
    log "  domain ${domain}: FAILED (nginx may need configuration)"
  fi
}

main() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "run as root: sudo bash deployments/docker/scripts/wsl-external-deploy.sh" >&2
    exit 1
  fi

  ensure_host_redis

  # Provision PostgreSQL databases for each environment
  provision_identity sdkwork_ai_dev sdkwork_ai_dev sdkworkdev123
  provision_identity sdkwork_ai_test sdkwork_ai_test sdkworktest123
  provision_identity sdkwork_ai_prod sdkwork_ai_prod sdkworkprod123
  ensure_pg_hba_docker_access

  stop_existing_stacks

  # Deploy all environments
  for env_name in development test production; do
    deploy_environment "$env_name"
  done

  # Configure nginx and hosts
  bash "$docker_root/scripts/install-wsl-hosts.sh" || true
  bash "$docker_root/scripts/install-wsl-nginx.sh" || true

  # Wait for health checks
  log "waiting for health checks..."
  sleep 15

  # Verify all environments
  verify_environment development 13800 server-dev.sdkwork.com
  verify_environment test 18888 server-test.sdkwork.com
  verify_environment production 18080 server.sdkwork.com

  log "deployment complete"
  log ""
  log "Environment access URLs:"
  log "  Development: http://server-dev.sdkwork.com  (port 13800)"
  log "  Test:        http://server-test.sdkwork.com (port 18888)"
  log "  Production:  http://server.sdkwork.com      (port 18080)"
  log ""
  log "Database connections (external PostgreSQL):"
  log "  Development: sdkwork_ai_dev"
  log "  Test:        sdkwork_ai_test"
  log "  Production:  sdkwork_ai_prod"
}

main "$@"
