#!/usr/bin/env bash
# Verify platform API plane reverse proxy: api*.brand → webserver imports →
# sdkwork-api-cloud-gateway. Does not require the gateway process to be healthy
# (502 is accepted as “vhost matched”). Host nginx is not used.
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
environment="${1:-development}"

case "${environment}" in
  development)
    import_port="${SDKWORK_WEBSERVER_DEV_IMPORT_HTTP_HOST_PORT:-80}"
    sample_host="api-dev.sdkwork.com"
    brands_host="api-dev.birdcoder.com"
    cn_host="api-dev.birdcoder.cn"
    ;;
  test)
    import_port="${SDKWORK_WEBSERVER_TEST_IMPORT_HTTP_HOST_PORT:-18898}"
    sample_host="api-test.sdkwork.com"
    brands_host="api-test.birdcoder.com"
    cn_host="api-test.birdcoder.cn"
    ;;
  production)
    import_port="${SDKWORK_WEBSERVER_PROD_IMPORT_HTTP_HOST_PORT:-18098}"
    sample_host="api.sdkwork.com"
    brands_host="api.birdcoder.com"
    cn_host="api.birdcoder.cn"
    ;;
  *)
    echo "usage: $0 [development|test|production]" >&2
    exit 2
    ;;
esac

echo "== platform API plane verification (${environment}) =="

entrypoint="${repo_root}/deployments/docker/scripts/entrypoint-standalone.sh"
for pattern in \
  'is_platform_api_gateway_module' \
  'write_platform_api_gateway_locations_docker' \
  'ensure_platform_api_gateway_import_listed' \
  'rewrite_module_gateway_upstream' \
  'SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT'; do
  if ! grep -Eq "${pattern}" "${entrypoint}"; then
    echo "FAIL: entrypoint missing ${pattern}" >&2
    exit 1
  fi
done
echo "OK: entrypoint wires platform API plane reverse proxy (no host nginx)"

checkout="${SDKWORK_SPACE_CHECKOUT_HOST_PATH:-/opt/deploy/sdkwork-space}"
sidecar="${checkout}/sdkwork-api-cloud-gateway/deployments/webserver/nginx.standalone.${environment}.conf"
if [ -f "${sidecar}" ]; then
  if grep -q 'upstream gateway {' "${sidecar}"; then
    upstream="$(sed -n '/upstream gateway {/,/}/s/^[[:space:]]*server[[:space:]]\+\([^;]*\);.*/\1/p' "${sidecar}" | head -1)"
    echo "sidecar upstream (checkout may be rewritten at container boot): ${upstream:-unknown}"
  fi
  if [ -f "${checkout}/sdkwork-api-cloud-gateway/deployments/webserver/snippets/gateway-locations.docker.conf" ]; then
    if grep -q 'location / {' "${checkout}/sdkwork-api-cloud-gateway/deployments/webserver/snippets/gateway-locations.docker.conf" \
      && grep -q 'proxy_pass http://gateway' "${checkout}/sdkwork-api-cloud-gateway/deployments/webserver/snippets/gateway-locations.docker.conf"; then
      echo "OK: docker locations proxy / (and /api/) to gateway"
    else
      echo "WARN: docker locations may still fall back to SPA static for /"
    fi
  else
    echo "note: gateway-locations.docker.conf appears after webserver entrypoint materialize"
  fi
else
  echo "WARN: missing sidecar ${sidecar} (ensure sdkwork-api-cloud-gateway is under the space checkout)"
fi

probe() {
  local host="$1"
  local path="$2"
  local code
  code="$(curl -sS -o /tmp/sdkwork-api-plane-body -w '%{http_code}' --connect-timeout 3 --noproxy '*' \
    -H "Host: ${host}" "http://127.0.0.1:${import_port}${path}" 2>/dev/null || printf '000')"
  printf '%s' "${code}"
}

report_probe() {
  local host="$1"
  local code
  code="$(probe "${host}" /healthz)"
  case "${code}" in
    200)
      echo "OK: ${host}/healthz via :${import_port} -> ${code}"
      ;;
    502|503)
      echo "OK: ${host} vhost matched via :${import_port} -> ${code} (upstream gateway not healthy yet; webserver reverse proxy is wired)"
      ;;
    000)
      echo "WARN: import plane not reachable on :${import_port} (is webserver serve-imports running?)"
      ;;
    *)
      echo "WARN: ${host}/healthz -> ${code}"
      head -c 160 /tmp/sdkwork-api-plane-body 2>/dev/null || true
      echo
      ;;
  esac
}

report_probe "${sample_host}"
report_probe "${brands_host}"
report_probe "${cn_host}"

if command -v nginx >/dev/null 2>&1 || [ -d /etc/nginx ]; then
  echo "WARN: host nginx still present; run: sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh"
else
  echo "OK: host nginx not installed"
fi

echo "done"
