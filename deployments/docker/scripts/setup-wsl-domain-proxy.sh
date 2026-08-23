#!/usr/bin/env bash
# One-shot WSL Ubuntu setup: external PG/Redis + Docker stacks + domain nginx reverse-proxy.
#
# Usage (inside WSL Ubuntu, password sudo):
#   sudo bash deployments/docker/scripts/setup-wsl-domain-proxy.sh
#
# Windows browser access (run PowerShell as Administrator on Windows host):
#   deployments/docker/scripts/setup-windows-port-forwarding.ps1
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
docker_root="$repo_root/deployments/docker"

log() { echo "[setup-wsl-domain-proxy] $*"; }

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/setup-wsl-domain-proxy.sh"
    exit 1
  fi
}

stop_port80_conflicts() {
  if ! ss -tlnp | grep -q ':80 '; then
    return 0
  fi
  local owner
  owner="$(ss -tlnp | awk '/:80 / {print $0}' | head -1)"
  if echo "${owner}" | grep -q nginx; then
    return 0
  fi
  local pid
  pid="$(echo "${owner}" | sed -n 's/.*pid=\([0-9]*\).*/\1/p' | head -1)"
  if [ -n "${pid}" ]; then
    log "stopping non-nginx process on :80 (pid=${pid})"
    kill "${pid}" 2>/dev/null || true
    sleep 2
  fi
}

verify_domain() {
  local domain="$1"
  if curl -fsS -H "Host: ${domain}" "http://127.0.0.1/healthz" >/dev/null 2>&1; then
    log "  ${domain}: OK"
    return 0
  fi
  if curl -fsS "http://${domain}/healthz" >/dev/null 2>&1; then
    log "  ${domain}: OK (/etc/hosts)"
    return 0
  fi
  log "  ${domain}: FAILED"
  return 1
}

main() {
  require_root
  stop_port80_conflicts

  bash "${docker_root}/scripts/setup-host-external-deps.sh"
  bash "${repo_root}/scripts/docker/deploy-docker-environment.sh" all --validate
  bash "${docker_root}/scripts/install-wsl-hosts.sh"
  bash "${docker_root}/scripts/install-wsl-nginx.sh" development test production

  systemctl enable nginx >/dev/null 2>&1 || true
  systemctl restart nginx

  log "waiting for containers..."
  sleep 20

  log "domain verification (nginx :80 -> Docker host ports):"
  verify_domain server-dev.sdkwork.com
  verify_domain server-test.sdkwork.com
  verify_domain server.sdkwork.com
  verify_domain sdkwork.com
  verify_domain app.sdkwork.com

  log ""
  log "WSL access URLs:"
  log "  http://server-dev.sdkwork.com"
  log "  http://server-test.sdkwork.com"
  log "  http://server.sdkwork.com"
  log "  http://sdkwork.com"
  log ""
  log "Windows: run as Administrator:"
  log "  deployments/docker/scripts/setup-windows-port-forwarding.ps1"
}

main "$@"
