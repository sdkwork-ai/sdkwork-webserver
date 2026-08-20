#!/usr/bin/env bash
# Provision WSL host-native external deps and deploy sdkwork-webserver (external mode).
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
docker_root="$repo_root/deployments/docker"

log() { echo "[wsl-external-deploy] $*"; }

ensure_host_redis() {
  if command -v redis-cli >/dev/null 2>&1 && redis-cli ping >/dev/null 2>&1; then
    log "host redis already running without password"
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
}

stop_embedded_stacks() {
  log "stopping embedded webserver stacks"
  for project in sdkwork-webserver-development sdkwork-webserver-test sdkwork-webserver-production sdkwork-webserver-shared; do
    docker compose -p "$project" \
      -f "$docker_root/docker-compose.yml" \
      -f "$docker_root/docker-compose.external.yml" \
      down --remove-orphans 2>/dev/null || true
    docker compose -p "$project" -f "$docker_root/docker-compose.yml" down --remove-orphans 2>/dev/null || true
  done
}

deploy_environment() {
  local env_name="$1"
  bash "$repo_root/scripts/docker/deploy-docker-environment.sh" "$env_name" --external --validate
}

main() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "run as root: sudo bash deployments/docker/scripts/wsl-external-deploy.sh" >&2
    exit 1
  fi

  ensure_host_redis

  provision_identity sdkwork_ai_dev sdkwork_ai_dev sdkworkdev123
  provision_identity sdkwork_ai_test sdkwork_ai_test sdkworktest123
  provision_identity sdkwork_ai_prod sdkwork_ai_prod sdkworkprod123

  stop_embedded_stacks

  for env_name in development test production; do
    deploy_environment "$env_name"
  done

  bash "$docker_root/scripts/install-wsl-hosts.sh" || true
  bash "$docker_root/scripts/install-wsl-nginx.sh" || true

  log "waiting for health checks..."
  sleep 15

  curl -fsS "http://127.0.0.1:13800/healthz" && echo " development OK"
  curl -fsS "http://127.0.0.1:18888/healthz" && echo " test OK"
  curl -fsS "http://127.0.0.1:18080/healthz" && echo " production OK"
  curl -fsS -H "Host: server-dev.sdkwork.com" "http://127.0.0.1/healthz" && echo " nginx dev OK"
  curl -fsS -H "Host: server-test.sdkwork.com" "http://127.0.0.1/healthz" && echo " nginx test OK"
  curl -fsS -H "Host: server.sdkwork.com" "http://127.0.0.1/healthz" && echo " nginx prod OK"

  log "deployment complete"
}

main "$@"
