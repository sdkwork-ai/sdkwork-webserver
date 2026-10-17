#!/usr/bin/env bash
# Entrypoint for the release-tarball container image.
#
# Responsibilities are deliberately narrow: validate that the operator-supplied
# configuration is present and readable, make sure the runtime directories
# exist, and hand off to the gateway. It never generates credentials and never
# rewrites the operator's config.toml -- a mounted config must stay exactly as
# written so that "what you configured is what runs".
set -euo pipefail

CONFIG_ROOT="${SDKWORK_WEBSERVER_CONFIG_ROOT:-/etc/sdkwork/webserver}"
RUNTIME_CONFIG_FILE="${SDKWORK_WEBSERVER_SERVER_CONFIG_FILE:-${CONFIG_ROOT}/config.toml}"
REDIS_SECRET_FILE="${CONFIG_ROOT}/secrets/redis.secret"
REDIS_HOST="${SDKWORK_WEBSERVER_REDIS_HOST:-cache}"
REDIS_PORT="${SDKWORK_WEBSERVER_REDIS_PORT:-6379}"
GATEWAY_BINARY="/app/bin/sdkwork-api-webserver-standalone-gateway"
SUBCOMMAND="${1:-serve-management}"

log() {
  echo "[sdkwork-webserver-entrypoint] $*" >&2
}

fail() {
  log "ERROR: $*"
  exit 1
}

# A missing config must fail loudly at startup rather than silently falling back
# to an in-image default that listens on the wrong port or the wrong database.
if [ ! -r "${RUNTIME_CONFIG_FILE}" ]; then
  fail "runtime config not readable: ${RUNTIME_CONFIG_FILE}
Mount your config.toml there, for example:
  -v /opt/sdkwork/etc/webserver:/etc/sdkwork/webserver:ro"
fi
log "using runtime config ${RUNTIME_CONFIG_FILE}"

if [ ! -x "${GATEWAY_BINARY}" ]; then
  fail "gateway binary missing or not executable: ${GATEWAY_BINARY}"
fi

# Secret files are referenced by path from config.toml; a missing one surfaces
# later as an authentication failure, so name the path now while the log still
# explains which file to fix.
if [ -d "${CONFIG_ROOT}/secrets" ]; then
  while IFS= read -r secret; do
    [ -n "${secret}" ] || continue
    if [ ! -r "${secret}" ]; then
      log "WARNING: ${secret} is present but not readable by uid $(id -u)"
    fi
  done < <(find "${CONFIG_ROOT}/secrets" -maxdepth 1 -type f 2>/dev/null || true)
fi

# Redis credentials are also file-only. The resolver cache is an optional layer
# and connects lazily, so a missing file must not abort startup; build the URL
# here only when the operator supplied a password and did not already pin a URL.
if [ -z "${SDKWORK_WEBSERVER_REDIS_URL:-}" ] && [ -r "${REDIS_SECRET_FILE}" ]; then
  REDIS_PASSWORD="$(tr -d '\r\n' < "${REDIS_SECRET_FILE}")"
  if [ -n "${REDIS_PASSWORD}" ]; then
    export SDKWORK_WEBSERVER_REDIS_URL="redis://:${REDIS_PASSWORD}@${REDIS_HOST}:${REDIS_PORT}"
    log "resolver cache redis url built from ${REDIS_SECRET_FILE}"
  else
    log "WARNING: ${REDIS_SECRET_FILE} is empty; resolver cache falls back to the memory layer"
  fi
fi

for dir in \
  "${CONFIG_ROOT}/secrets" \
  /etc/sdkwork/api-gateway \
  /etc/sdkwork/certs \
  /run/secrets/sdkwork/api-gateway \
  /var/lib/sdkwork/webserver/acme-webroot \
  /var/lib/sdkwork/webserver/acme-accounts \
  /var/lib/sdkwork/webserver/tls-materials \
  /opt/deploy \
  /opt/deploy/drive
do
  [ -d "${dir}" ] || install -d -o sdkwork -g sdkwork -m 0750 "${dir}" 2>/dev/null || true
done

# exec so the gateway replaces this shell: no orphan wrapper keeps the
# container alive after the gateway exits.
log "starting ${GATEWAY_BINARY} ${SUBCOMMAND}"
exec "${GATEWAY_BINARY}" "${SUBCOMMAND}"
