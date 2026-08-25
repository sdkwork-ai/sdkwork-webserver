#!/usr/bin/env bash
# Copy browser dist trees from a development workspace into the Ubuntu
# /opt/deploy/sdkwork-space checkout, then materialize dist/<profile>/<envAlias>.
# Authority: SDKWORK_WEBSERVER_SPEC.md §17.1, FRONTEND_CODE_SPEC.md §7
set -euo pipefail

CHECKOUT="${1:-${SDKWORK_SPACE_CHECKOUT_HOST_PATH:-${SDKWORK_SPACE_HOST_PATH:-/opt/deploy}/sdkwork-space}}"
SEED_ROOT="${2:-${SDKWORK_SPACE_DIST_SEED_ROOT:-}}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

log() {
  echo "[sdkwork-webserver-dist-sync] $*"
}

if [ -z "${SEED_ROOT}" ]; then
  for candidate in \
    "${SDKWORK_SPACE_LOCAL_PATH:-}" \
    /mnt/e/sdkwork-space \
    /mnt/c/sdkwork-space \
    "$(dirname "${CHECKOUT}")/../.."
  do
    [ -n "${candidate}" ] || continue
    if [ -d "${candidate}/sdkwork-im/apps" ] || [ -d "${candidate}/sdkwork-cloudrouter/apps" ]; then
      SEED_ROOT="${candidate}"
      break
    fi
  done
fi

if [ ! -d "${CHECKOUT}" ]; then
  log "checkout missing: ${CHECKOUT}"
  exit 1
fi

copied=0
if [ -n "${SEED_ROOT}" ] && [ -d "${SEED_ROOT}" ]; then
  log "seeding dist trees from ${SEED_ROOT} -> ${CHECKOUT}"
  for module_dir in "${CHECKOUT}"/sdkwork-*; do
    [ -d "${module_dir}" ] || continue
    module="$(basename "${module_dir}")"
    case "${module}" in
      sdkwork-webserver) continue ;;
    esac
    seed_mod="${SEED_ROOT}/${module}"
    [ -d "${seed_mod}/apps" ] || continue
    for surface in pc h5; do
      for app in "${seed_mod}/apps"/*-"${surface}"; do
        [ -d "${app}" ] || continue
        app_name="$(basename "${app}")"
        src_dist="${app}/dist"
        dst_dist="${module_dir}/apps/${app_name}/dist"
        [ -d "${src_dist}" ] || continue
        if [ ! -f "${src_dist}/index.html" ] && ! ls "${src_dist}"/*/index.html >/dev/null 2>&1; then
          continue
        fi
        mkdir -p "${dst_dist}"
        find "${src_dist}" -mindepth 1 -maxdepth 1 \
          ! -name 'standalone' ! -name 'cloud' \
          ! -name 'dev' ! -name 'test' ! -name 'staging' ! -name 'prod' \
          -exec cp -a -t "${dst_dist}" {} +
        for profile in standalone cloud; do
          if [ -d "${src_dist}/${profile}" ]; then
            mkdir -p "${dst_dist}/${profile}"
            cp -a "${src_dist}/${profile}/." "${dst_dist}/${profile}/"
          fi
        done
        # Migration: legacy environment-only subtrees land in standalone.
        for alias in dev test staging prod; do
          if [ -f "${src_dist}/${alias}/index.html" ]; then
            mkdir -p "${dst_dist}/standalone/${alias}"
            cp -a "${src_dist}/${alias}/." "${dst_dist}/standalone/${alias}/"
          fi
        done
        copied=$((copied + 1))
        log "synced ${module}/${app_name}/dist"
      done
    done
  done
  log "synced ${copied} app dist tree(s)"
else
  log "no seed workspace found; skipping workspace sync"
fi

bash "${SCRIPT_DIR}/materialize-space-dist-aliases.sh" "${CHECKOUT}"
