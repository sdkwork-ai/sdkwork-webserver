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
