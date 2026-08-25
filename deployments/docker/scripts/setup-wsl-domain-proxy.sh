#!/usr/bin/env bash
# One-shot WSL Ubuntu setup: external PG/Redis + Docker stacks + hosts.
# Host nginx is NOT used — sdkwork-webserver Docker owns domain reverse proxy.
#
# Usage (inside WSL Ubuntu, password sudo):
#   sudo bash deployments/docker/scripts/setup-wsl-domain-proxy.sh
#
# Windows browser access (run PowerShell as Administrator on Windows host):
#   deployments/docker/scripts/setup-windows-port-forwarding-admin.ps1
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
docker_root="$repo_root/deployments/docker"
dev_import_port="${SDKWORK_WEBSERVER_DEV_IMPORT_HTTP_HOST_PORT:-80}"
dev_mgmt_port="${SDKWORK_WEBSERVER_DEV_HOST_PORT:-13800}"

log() { echo "[setup-wsl-domain-proxy] $*"; }

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/setup-wsl-domain-proxy.sh"
    exit 1
  fi
}

verify_host() {
  local domain="$1"
  local port="$2"
  if curl -fsS --noproxy '*' -H "Host: ${domain}" "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
    log "  ${domain} via :${port}: OK"
    return 0
  fi
  log "  ${domain} via :${port}: FAILED"
  return 1
}

main() {
  require_root

  bash "${docker_root}/scripts/setup-host-external-deps.sh"
  bash "${repo_root}/scripts/docker/deploy-docker-environment.sh" all --validate
  bash "${docker_root}/scripts/install-wsl-hosts.sh"
  # Retire host nginx — Docker webserver is the reverse proxy.
  bash "${docker_root}/scripts/uninstall-wsl-nginx.sh" || true

  log "waiting for containers..."
  sleep 20

  log "domain verification (Docker published ports; no host nginx):"
  verify_host server-dev.sdkwork.com "${dev_mgmt_port}" || true
  verify_host api-dev.sdkwork.com "${dev_import_port}" || true
  verify_host api-dev.birdcoder.com "${dev_import_port}" || true
  verify_host im-dev.sdkwork.com "${dev_import_port}" || true

  log ""
  log "WSL / Windows access (hosts -> 127.0.0.1):"
  log "  Management:  http://server-dev.sdkwork.com:${dev_mgmt_port}/healthz"
  log "  Modules/API: http://api-dev.sdkwork.com/  (Docker host :${dev_import_port})"
  log "               http://im-dev.sdkwork.com/"
  log "               http://api-dev.birdcoder.cn/"
  log "  HTTPS:       https://api-dev.sdkwork.com/  (Docker host :443)"
  log ""
  log "Windows: run as Administrator:"
  log "  deployments/docker/scripts/setup-windows-port-forwarding-admin.ps1"
}

main "$@"
