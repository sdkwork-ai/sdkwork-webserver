#!/usr/bin/env bash
# Verify sibling module nginx sidecars are aggregated in imports.d/import.conf
# and served by the module-imports data plane (SDKWORK_WEBSERVER_SPEC.md §17).
set -euo pipefail

CONTAINER="${SDKWORK_VERIFY_CONTAINER:-sdkwork-webserver-development}"
IMPORT_PORT="${SDKWORK_VERIFY_IMPORT_PORT:-13808}"
MODULE="${SDKWORK_VERIFY_MODULE:-sdkwork-im}"
CHECKOUT="${SDKWORK_SPACE_CHECKOUT_HOST_PATH:-${SDKWORK_SPACE_HOST_PATH:-/opt/deploy}/sdkwork-space}"
ENVIRONMENT="${SDKWORK_VERIFY_ENVIRONMENT:-development}"
PROFILE="${SDKWORK_VERIFY_PROFILE:-standalone}"

fail() {
  echo "[verify-module-nginx-import] FAIL: $*" >&2
  exit 1
}

ok() {
  echo "[verify-module-nginx-import] OK: $*"
}

command -v docker >/dev/null 2>&1 || fail "docker not found"
docker inspect "${CONTAINER}" >/dev/null 2>&1 || fail "container ${CONTAINER} not running"

import_conf="/etc/sdkwork/webserver/imports.d/import.conf"
docker exec "${CONTAINER}" test -f "${import_conf}" \
  || fail "missing ${import_conf} (entrypoint must write nginx include aggregator)"

if docker exec "${CONTAINER}" find /etc/sdkwork/webserver/imports.d -maxdepth 1 -type l -name '*.conf' 2>/dev/null | grep -q .; then
  fail "imports.d must not contain per-module symlink conf files; use import.conf only"
fi

sidecar="${CHECKOUT}/${MODULE}/deployments/webserver/nginx.${PROFILE}.${ENVIRONMENT}.conf"
overlay="/etc/sdkwork/webserver/import-sidecars/${MODULE}/nginx.${PROFILE}.${ENVIRONMENT}.conf"

docker exec "${CONTAINER}" grep -Fq "include ${sidecar};" "${import_conf}" \
  || docker exec "${CONTAINER}" grep -Fq "include ${overlay};" "${import_conf}" \
  || fail "${import_conf} does not include ${MODULE} checkout sidecar ${sidecar}"

resolved_sidecar="${sidecar}"
if docker exec "${CONTAINER}" grep -Fq "include ${overlay};" "${import_conf}"; then
  resolved_sidecar="${overlay}"
  docker exec "${CONTAINER}" test -f "${overlay}" \
    || fail "overlay sidecar missing in container: ${overlay} (mount checkout :rw to use checkout paths)"
  ok "using import-sidecars overlay ${overlay} (prefer checkout include ${sidecar})"
else
  docker exec "${CONTAINER}" test -f "${sidecar}" \
    || fail "checkout sidecar missing in container: ${sidecar}"
  ok "using checkout sidecar ${sidecar}"
fi

mapfile -t sample_hosts < <(
  docker exec "${CONTAINER}" sed -n 's/.*server_name[[:space:]]\+\([^;]*\);.*/\1/p' "${resolved_sidecar}" \
    | tr ' ' '\n' \
    | sed '/^$/d' \
    | head -2
)
[ "${#sample_hosts[@]}" -gt 0 ] || fail "no server_name hosts in ${sidecar}"

for host in "${sample_hosts[@]}"; do
  body="$(curl -sS --noproxy '*' -m 5 -H "Host: ${host}" "http://127.0.0.1:${IMPORT_PORT}/" || true)"
  if printf '%s' "${body}" | grep -q 'Static fallback placeholder'; then
    fail "${host} via :${IMPORT_PORT} returned static fallback (dist missing or wrong spa_root)"
  fi
  if ! printf '%s' "${body}" | grep -qi '<html'; then
    fail "${host} via :${IMPORT_PORT} did not return HTML"
  fi
  ok "${host} -> import data plane :${IMPORT_PORT} (not fallback)"
done

spa_root="$(docker exec "${CONTAINER}" sed -n 's/.*root \([^;]*\);.*/\1/p' \
  "$(dirname "${resolved_sidecar}")/snippets/gateway-locations.docker.conf" \
  2>/dev/null | sed -n '/dist/p' | head -1 | tr -d ' ')"
if [ -n "${spa_root}" ]; then
  docker exec "${CONTAINER}" test -f "${spa_root}/index.html" \
    || fail "spa_root index missing in container: ${spa_root}/index.html"
  ok "spa_root ${spa_root}/index.html present in checkout tree"
fi

echo "[verify-module-nginx-import] ${MODULE} nginx import.conf chain verified"
