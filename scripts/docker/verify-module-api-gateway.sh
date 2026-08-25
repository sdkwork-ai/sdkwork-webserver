#!/usr/bin/env bash
# Verify sdkwork-webserver reverse-proxies the platform API plane (api*.brand)
# via the imported sdkwork-api-cloud-gateway nginx sidecar.
#
# Does NOT require the cloud-gateway process to be healthy — only that
# webserver import routing is configured (APP_RUNTIME_TOPOLOGY_NAMING.md §9).
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
environment="${1:-development}"
import_port="${2:-}"
container="${3:-}"

case "${environment}" in
  development)
    import_port="${import_port:-${SDKWORK_WEBSERVER_DEV_IMPORT_HTTP_HOST_PORT:-80}}"
    container="${container:-sdkwork-webserver-development}"
    api_prefix="api-dev"
    ;;
  test)
    import_port="${import_port:-${SDKWORK_WEBSERVER_TEST_IMPORT_HTTP_HOST_PORT:-18898}}"
    container="${container:-sdkwork-webserver-test}"
    api_prefix="api-test"
    ;;
  production)
    import_port="${import_port:-${SDKWORK_WEBSERVER_PROD_IMPORT_HTTP_HOST_PORT:-18098}}"
    container="${container:-sdkwork-webserver-production}"
    api_prefix="api"
    ;;
  *)
    echo "usage: $0 [development|test|production] [import_host_port] [container_name]" >&2
    exit 2
    ;;
esac

brands=(
  sdkwork.com birdcoder.com dtupay.com
  sdkwork.cn birdcoder.cn dtupay.cn
  skubc.com skubc.cn zowalk.com zowalk.cn
  offer86.com offer86.cn 86offer.com 86offer.cn
)

echo "== platform API plane verification (${environment}) =="

entrypoint="${repo_root}/deployments/docker/scripts/entrypoint-standalone.sh"
for pattern in \
  'rewrite_module_gateway_upstream' \
  'write_platform_api_gateway_locations_docker' \
  'is_platform_api_gateway_module' \
  'SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT'; do
  if ! grep -Eq "${pattern}" "${entrypoint}"; then
    echo "FAIL: entrypoint missing ${pattern}" >&2
    exit 1
  fi
done
echo "OK: entrypoint platform API plane helpers present"

if ! docker inspect "${container}" >/dev/null 2>&1; then
  echo "WARN: container ${container} not running; skip runtime checks"
  echo "done"
  exit 0
fi

import_conf="$(docker exec "${container}" cat /etc/sdkwork/webserver/imports.d/import.conf 2>/dev/null || true)"
if ! printf '%s\n' "${import_conf}" | grep -q 'sdkwork-api-cloud-gateway'; then
  echo "FAIL: imports.d/import.conf does not include sdkwork-api-cloud-gateway" >&2
  exit 1
fi
echo "OK: import.conf includes sdkwork-api-cloud-gateway"

api_conf_line="$(printf '%s\n' "${import_conf}" | grep 'sdkwork-api-cloud-gateway' | head -1)"
api_conf="${api_conf_line#include }"
api_conf="${api_conf%;}"
api_conf="$(printf '%s' "${api_conf}" | tr -d '[:space:]')"

nginx_body="$(docker exec "${container}" cat "${api_conf}" 2>/dev/null || true)"
if [ -z "${nginx_body}" ]; then
  echo "FAIL: cannot read imported API nginx conf ${api_conf}" >&2
  exit 1
fi

if ! printf '%s\n' "${nginx_body}" | grep -q 'upstream gateway {'; then
  echo "FAIL: ${api_conf} missing upstream gateway" >&2
  exit 1
fi
upstream_server="$(printf '%s\n' "${nginx_body}" | sed -n '/upstream gateway {/,/}/s/^[[:space:]]*server[[:space:]]\+\([^;]*\);.*/\1/p' | head -1)"
echo "OK: upstream gateway -> ${upstream_server:-unknown}"

missing=0
for brand in "${brands[@]}"; do
  host="${api_prefix}.${brand}"
  if [ "${api_prefix}" = "api" ]; then
    host="api.${brand}"
  fi
  if ! printf '%s\n' "${nginx_body}" | grep -Eq "(^|[[:space:]])${host}([[:space:]]|;|$)"; then
    echo "FAIL: missing server_name host ${host}" >&2
    missing=1
  fi
done
if [ "${missing}" -ne 0 ]; then
  exit 1
fi
echo "OK: all ${#brands[@]} brand API hosts present (${api_prefix}.<brand>)"

if ! docker exec "${container}" test -f "$(dirname "${api_conf}")/snippets/gateway-locations.docker.conf"; then
  echo "FAIL: missing gateway-locations.docker.conf beside ${api_conf}" >&2
  exit 1
fi
locations="$(docker exec "${container}" cat "$(dirname "${api_conf}")/snippets/gateway-locations.docker.conf")"
if ! printf '%s\n' "${locations}" | grep -q 'location / {' \
  || ! printf '%s\n' "${locations}" | grep -q 'proxy_pass http://gateway'; then
  echo "FAIL: platform API locations must proxy / to gateway (not SPA)" >&2
  exit 1
fi
echo "OK: platform API locations proxy / and /api/ to upstream gateway"

# Routing probe: Host must be accepted by the data plane (not default-server).
# Upstream may be 502/503/504 when cloud-gateway is down — that is OK.
probe_hosts=("api-dev.sdkwork.com" "api-dev.birdcoder.com" "api-dev.birdcoder.cn")
if [ "${environment}" = "test" ]; then
  probe_hosts=("api-test.sdkwork.com" "api-test.birdcoder.com" "api-test.birdcoder.cn")
elif [ "${environment}" = "production" ]; then
  probe_hosts=("api.sdkwork.com" "api.birdcoder.com" "api.birdcoder.cn")
fi

for host in "${probe_hosts[@]}"; do
  code="$(curl -sS -o /tmp/sdkwork-api-plane-probe.body -w '%{http_code}' --connect-timeout 3 --noproxy '*' \
    -H "Host: ${host}" "http://127.0.0.1:${import_port}/healthz" 2>/dev/null || printf '000')"
  body="$(tr -d '\r' </tmp/sdkwork-api-plane-probe.body 2>/dev/null | head -c 120 || true)"
  case "${code}" in
    000)
      echo "WARN: ${host} no response on :${import_port} (is ${container} publishing import port?)"
      ;;
    404)
      echo "FAIL: ${host} returned 404 — Host not bound on import data plane" >&2
      exit 1
      ;;
    *)
      echo "OK: ${host} routed on :${import_port} (HTTP ${code}; upstream may be down: ${body})"
      ;;
  esac
done

echo "done"
