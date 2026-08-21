#!/usr/bin/env bash
# Add local /etc/hosts entries for SDKWork Web Server Docker domain routing on WSL.
# Hosts follow APP_RUNTIME_TOPOLOGY_NAMING.md §9.2 (role host server).
set -euo pipefail

HOSTS_FILE="/etc/hosts"
MARKER="# sdkwork-webserver-docker-wsl"

DOMAINS=(
  server-dev.sdkwork.com
  server-app-dev.sdkwork.com
  server-admin-dev.sdkwork.com
  server-test.sdkwork.com
  server-app-test.sdkwork.com
  server-admin-test.sdkwork.com
  server.sdkwork.com
  server-app.sdkwork.com
  server-admin.sdkwork.com
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

remove_legacy_block() {
  if grep -q "${MARKER}" "${HOSTS_FILE}"; then
    # Drop previous marker block (including retired server*/testserver* names).
    sed -i "/${MARKER}/,/^\$/d" "${HOSTS_FILE}" || true
    sed -i "/${MARKER}/d" "${HOSTS_FILE}" || true
  fi
  # Best-effort cleanup of retired nicknames if left outside the marker block.
  sed -i \
    -e '/[[:space:]]server-dev\.sdkwork\.com$/d' \
    -e '/[[:space:]]server-test\.sdkwork\.com$/d' \
    -e '/[[:space:]]server\.sdkwork\.com$/d' \
    -e '/[[:space:]]testserver\.sdkwork\.com$/d' \
    -e '/[[:space:]]server-dev\.birdcoder\.com$/d' \
    -e '/[[:space:]]server-test\.birdcoder\.com$/d' \
    -e '/[[:space:]]server\.birdcoder\.com$/d' \
    -e '/[[:space:]]server-dev\.dtupay\.com$/d' \
    -e '/[[:space:]]server-test\.dtupay\.com$/d' \
    -e '/[[:space:]]server\.dtupay\.com$/d' \
    "${HOSTS_FILE}" || true
}

main() {
  require_root
  remove_legacy_block
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
