#!/usr/bin/env bash
# Add local /etc/hosts entries for SDKWork Web Server Docker domain routing on WSL.
# Hosts follow APP_RUNTIME_TOPOLOGY_NAMING.md §9.2 (role host server).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOSTS_FILE="/etc/hosts"
MARKER="# sdkwork-webserver-docker-wsl"

# Console / portal hosts are always present; module hosts come from discovery.
# Platform API plane hosts (api*.brand) are always merged so api-dev.xxx.com /
# api.xxx.com resolve even when discovery temporarily misses the gateway sidecar.
CORE_DOMAINS=(
  server-dev.sdkwork.com
  server-app-dev.sdkwork.com
  server-admin-dev.sdkwork.com
  server-test.sdkwork.com
  server-app-test.sdkwork.com
  server-admin-test.sdkwork.com
  server.sdkwork.com
  server-app.sdkwork.com
  server-admin.sdkwork.com
  # Web Server public domains (SDKWORK_WEBSERVER_CERT_DOMAINS): optional HTTPS
  # listeners use per-environment host ports 18430/28430/38430 (container 8430).
  sdkwork.com
  app.sdkwork.com
)

PLATFORM_API_BRANDS=(
  sdkwork.com birdcoder.com dtupay.com
  sdkwork.cn birdcoder.cn dtupay.cn
  skubc.com skubc.cn zowalk.com zowalk.cn
  offer86.com offer86.cn 86offer.com 86offer.cn
)

platform_api_domains() {
  local brand
  for brand in "${PLATFORM_API_BRANDS[@]}"; do
    printf 'api-dev.%s\n' "${brand}"
    printf 'api-test.%s\n' "${brand}"
    printf 'api.%s\n' "${brand}"
  done
}

collect_domains() {
  local env_name host
  printf '%s\n' "${CORE_DOMAINS[@]}"
  platform_api_domains
  if [ -x "${SCRIPT_DIR}/discover-module-hosts.sh" ]; then
    for env_name in development test production; do
      bash "${SCRIPT_DIR}/discover-module-hosts.sh" "${env_name}" 2>/dev/null || true
    done
  else
    printf '%s\n' \
      im-dev.sdkwork.com router-dev.sdkwork.com cloudrouter-dev.sdkwork.com \
      router-admin-dev.sdkwork.com router-open-dev.sdkwork.com \
      im-test.sdkwork.com router-test.sdkwork.com cloudrouter-test.sdkwork.com \
      router-admin-test.sdkwork.com router-open-test.sdkwork.com \
      im.sdkwork.com router.sdkwork.com cloudrouter.sdkwork.com \
      router-admin.sdkwork.com router-open.sdkwork.com
  fi
}

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
  mapfile -t DOMAINS < <(collect_domains | sed '/^$/d' | sort -u)
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
