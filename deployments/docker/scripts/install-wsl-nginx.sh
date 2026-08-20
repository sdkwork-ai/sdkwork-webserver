#!/usr/bin/env bash
# Install nginx :80 domain routing for SDKWork Web Server Docker stacks on WSL Ubuntu.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Script lives at deployments/docker/scripts/, so repo root is 3 levels up.
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
NGINX_ROOT="${REPO_ROOT}/deployments/docker/nginx"
SITES_AVAILABLE="/etc/nginx/sites-available"
SITES_ENABLED="/etc/nginx/sites-enabled"

log() {
  echo "[sdkwork-webserver-nginx] $*"
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/install-wsl-nginx.sh"
    exit 1
  fi
}

install_site() {
  local name="$1"
  local source="${NGINX_ROOT}/${name}.conf"
  local target="${SITES_AVAILABLE}/sdkwork-webserver-${name}"
  if [ ! -f "${source}" ]; then
    log "missing nginx config: ${source}"
    exit 1
  fi
  install -m 0644 "${source}" "${target}"
  ln -sf "${target}" "${SITES_ENABLED}/sdkwork-webserver-${name}"
  log "installed ${target}"
}

main() {
  require_root
  if ! command -v nginx >/dev/null 2>&1; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq nginx
  fi
  install -d -m 0755 "${SITES_AVAILABLE}" "${SITES_ENABLED}"
  for environment in development test production; do
    install_site "${environment}"
  done
  nginx -t
  systemctl enable nginx >/dev/null 2>&1 || true
  systemctl restart nginx
  log "nginx reloaded; port 80 routes:"
  log "  server-dev.*   -> 127.0.0.1:13800"
  log "  server-test.*  -> 127.0.0.1:18888"
  log "  server.*       -> 127.0.0.1:18080"
}

main "$@"
