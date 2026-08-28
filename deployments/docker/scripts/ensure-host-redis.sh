#!/usr/bin/env bash
# Ensure WSL host-native Redis is running for external-mode docker deployment.
# External mode expects passwordless Redis reachable from containers via
# host.docker.internal:6379 (bind 0.0.0.0, protected-mode no).
set -euo pipefail

log() {
  echo "[ensure-host-redis] $*"
}

ensure_host_redis() {
  if command -v redis-cli >/dev/null 2>&1 && redis-cli ping >/dev/null 2>&1; then
    local requirepass
    requirepass="$(redis-cli CONFIG GET requirepass 2>/dev/null | tail -1 || true)"
    if [ -n "${requirepass}" ]; then
      log "warning: host Redis requirepass is set; external mode expects passwordless Redis"
    fi
    log "host redis already running"
    return 0
  fi

  if ! command -v redis-server >/dev/null 2>&1; then
    log "installing redis-server"
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq redis-server
  fi

  local redis_conf="/etc/redis/redis.conf"
  # Ensure no password and listen on all interfaces for host.docker.internal access.
  sed -i 's/^# requirepass .*/requirepass ""/' "${redis_conf}" 2>/dev/null || true
  sed -i 's/^requirepass .*/requirepass ""/' "${redis_conf}" 2>/dev/null || true
  sed -i 's/^bind 127.0.0.1 .*/bind 0.0.0.0 ::1/' "${redis_conf}" 2>/dev/null || true
  sed -i 's/^protected-mode yes/protected-mode no/' "${redis_conf}" 2>/dev/null || true

  systemctl enable redis-server
  systemctl restart redis-server
  sleep 1
  redis-cli ping
  log "host redis ready on 6379 (no password)"
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/ensure-host-redis.sh"
    exit 1
  fi
  ensure_host_redis
fi
