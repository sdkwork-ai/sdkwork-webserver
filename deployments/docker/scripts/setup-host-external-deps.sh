#!/usr/bin/env bash
# Provision ENVIRONMENT_SPEC §7.1 workspace identities on WSL host-native PostgreSQL
# and verify passwordless Redis before external-mode docker compose deployment.
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
# shellcheck source=ensure-host-redis.sh
source "$script_dir/ensure-host-redis.sh"

log() {
  echo "[sdkwork-webserver-external-deps] $*"
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/setup-host-external-deps.sh"
    exit 1
  fi
}

create_identity() {
  local db="$1"
  local user="$2"
  local pass="$3"

  sudo -u postgres psql -v ON_ERROR_STOP=1 -d postgres <<EOSQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '${user}') THEN
    EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', '${user}', '${pass}');
  ELSE
    EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L', '${user}', '${pass}');
  END IF;
END\$\$;
EOSQL

  if ! sudo -u postgres psql -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='${db}'" | grep -q 1; then
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d postgres -c "CREATE DATABASE \"${db}\" OWNER \"${user}\";"
  fi

  sudo -u postgres psql -v ON_ERROR_STOP=1 -d postgres -c "GRANT ALL PRIVILEGES ON DATABASE \"${db}\" TO \"${user}\";"

  sudo -u postgres psql -v ON_ERROR_STOP=1 -d "${db}" <<EOSQL
CREATE SCHEMA IF NOT EXISTS "${db}";
GRANT ALL ON SCHEMA "${db}" TO "${user}";
GRANT CREATE ON SCHEMA "${db}" TO "${user}";
ALTER ROLE "${user}" SET search_path TO "${db}";
EOSQL

  log "provisioned db=${db} user=${user} schema=${db}"
}

ensure_postgresql_running() {
  if ! command -v psql >/dev/null 2>&1; then
    log "installing postgresql"
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq postgresql
  fi

  if command -v systemctl >/dev/null 2>&1; then
    systemctl enable postgresql 2>/dev/null || true
    systemctl start postgresql 2>/dev/null || service postgresql start
  else
    service postgresql start
  fi
}

ensure_pg_listen_addresses() {
  local pgconf
  pgconf="$(sudo -u postgres psql -tAc "SHOW config_file;" 2>/dev/null | tr -d '[:space:]')"
  if [ -z "${pgconf}" ] || [ ! -f "${pgconf}" ]; then
    log "warning: could not resolve postgresql.conf"
    return 0
  fi

  if grep -qE "^[[:space:]]*listen_addresses[[:space:]=]+'?\*'?" "${pgconf}"; then
    log "postgresql already listens on all interfaces"
    return 0
  fi

  if grep -qE "^[[:space:]]*#?[[:space:]]*listen_addresses[[:space:]=]" "${pgconf}"; then
    sed -i -E "s|^[[:space:]]*#?[[:space:]]*listen_addresses[[:space:]=]+.*|listen_addresses = '*'|" "${pgconf}"
  else
    echo "listen_addresses = '*'" >> "${pgconf}"
  fi

  systemctl reload postgresql || service postgresql reload
  log "updated ${pgconf}: listen_addresses='*'"
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

main() {
  require_root
  ensure_postgresql_running
  ensure_pg_listen_addresses
  create_identity "sdkwork_ai_dev" "sdkwork_ai_dev" "sdkworkdev123"
  create_identity "sdkwork_ai_test" "sdkwork_ai_test" "sdkworktest123"
  create_identity "sdkwork_ai_prod" "sdkwork_ai_prod" "sdkworkprod123"
  ensure_pg_hba_docker_access
  ensure_host_redis
  log "host PostgreSQL and Redis are ready for external docker deployment"
}

main "$@"
