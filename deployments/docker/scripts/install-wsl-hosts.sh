#!/usr/bin/env bash
# Add local /etc/hosts entries for SDKWork Web Server Docker domain routing on WSL.
set -euo pipefail

HOSTS_FILE="/etc/hosts"
MARKER="# sdkwork-webserver-docker-wsl"

DOMAINS=(
  server-dev.sdkwork.com
  server-dev.birdcoder.com
  server-dev.dtupay.com
  server-test.sdkwork.com
  server-test.birdcoder.com
  server-test.dtupay.com
  server.sdkwork.com
  server.birdcoder.com
  server.dtupay.com
)

log() {
  echo "[sdkwork-webserver-hosts] $*"
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    log "run as root: sudo bash deployments/docker/scripts/install-wsl-hosts.sh"
    exit 1
  fi
}

main() {
  require_root
  if grep -q "${MARKER}" "${HOSTS_FILE}"; then
    log "hosts block already present (${MARKER})"
    exit 0
  fi
  {
    echo ""
    echo "${MARKER}"
    for domain in "${DOMAINS[@]}"; do
      printf '127.0.0.1 %s\n' "${domain}"
    done
  } >> "${HOSTS_FILE}"
  log "added ${#DOMAINS[@]} local domain entries to ${HOSTS_FILE}"
}

main "$@"
