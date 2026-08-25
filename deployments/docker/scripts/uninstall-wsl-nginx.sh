#!/usr/bin/env bash
# Fully remove host nginx from WSL Ubuntu.
#
# sdkwork-webserver Docker (serve-imports / Adaptive Web) owns public reverse
# proxy for module and platform API domains (SDKWORK_WEBSERVER_SPEC.md §17).
# Host nginx is retired — do not reinstall for domain routing.
#
# Usage (WSL Ubuntu):
#   sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh
set -euo pipefail

log() { echo "[uninstall-wsl-nginx] $*"; }

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh"
    exit 1
  fi
}

stop_nginx() {
  if command -v systemctl >/dev/null 2>&1; then
    systemctl stop nginx 2>/dev/null || true
    systemctl disable nginx 2>/dev/null || true
  fi
  if command -v service >/dev/null 2>&1; then
    service nginx stop 2>/dev/null || true
  fi
  pkill -x nginx 2>/dev/null || true
}

remove_sdkwork_sites() {
  local sites_dir="/etc/nginx/sites-enabled/sdkwork"
  local available_dir="/etc/nginx/sites-available/sdkwork"
  if [ -d "${sites_dir}" ]; then
    log "removing ${sites_dir}"
    rm -rf "${sites_dir}"
  fi
  if [ -d "${available_dir}" ]; then
    log "removing ${available_dir}"
    rm -rf "${available_dir}"
  fi
  # Legacy per-domain confs under sites-enabled
  shopt -s nullglob
  for conf in /etc/nginx/sites-enabled/*sdkwork* /etc/nginx/sites-enabled/*birdcoder* \
    /etc/nginx/sites-enabled/module-imports-*.conf; do
    [ -e "${conf}" ] || continue
    log "removing ${conf}"
    rm -f "${conf}"
  done
  shopt -u nullglob
}

purge_packages() {
  if ! command -v apt-get >/dev/null 2>&1; then
    log "apt-get not available; skipped package purge"
    return 0
  fi
  export DEBIAN_FRONTEND=noninteractive
  if dpkg -l nginx nginx-common nginx-core 2>/dev/null | grep -q '^ii'; then
    log "purging nginx packages"
    apt-get purge -y nginx nginx-common nginx-core nginx-full nginx-light 2>/dev/null || \
      apt-get purge -y nginx nginx-common nginx-core 2>/dev/null || true
    apt-get autoremove -y 2>/dev/null || true
  else
    log "nginx packages not installed"
  fi
}

remove_nginx_tree() {
  if [ -d /etc/nginx ]; then
    log "removing leftover /etc/nginx"
    rm -rf /etc/nginx
  fi
  if [ -d /var/log/nginx ]; then
    rm -rf /var/log/nginx
  fi
  if [ -d /var/cache/nginx ]; then
    rm -rf /var/cache/nginx
  fi
}

main() {
  require_root
  log "retiring host nginx — public reverse proxy is sdkwork-webserver Docker"
  stop_nginx
  remove_sdkwork_sites
  purge_packages
  remove_nginx_tree
  if command -v nginx >/dev/null 2>&1; then
    log "warning: nginx binary still on PATH: $(command -v nginx)"
    exit 1
  fi
  if ss -tlnp 2>/dev/null | grep -qE ':80 |:8088 '; then
    log "note: something still listens on :80 or :8088 (not necessarily nginx)"
    ss -tlnp | grep -E ':80 |:8088 ' || true
  fi
  log "nginx fully uninstalled"
  log "Domain access: Docker public plane host :80 / :443 (dev) or :18898/:18098 (test/prod)"
  log "  curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz"
  log "management console: Docker :13800 / :18888 / :18080"
}

main "$@"
