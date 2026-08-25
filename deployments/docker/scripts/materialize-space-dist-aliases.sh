#!/usr/bin/env bash
# Materialize Adaptive Web dist/<profile>/<envAlias> trees on the Ubuntu host
# checkout (default profile: standalone).
# Authority: FRONTEND_CODE_SPEC.md §7, SDKWORK_WEBSERVER_SPEC.md §17.1
#
# Prefer an existing dist/standalone/{dev,test,staging,prod}/index.html as the
# seed; legacy dist/{dev,test,staging,prod}/index.html trees are accepted as
# migration seeds and copied into the standalone profile subtree.
# When only a legacy bare dist/index.html exists, copy it into every alias.
# When no dist tree exists yet, seed from deployments/webserver/static so Docker
# Adaptive Web roots resolve under apps/*-{pc,h5}/dist/standalone/<envAlias>/
# instead of falling back to the imported static placeholder path.
# Operators should still run pnpm build:{pc,h5}:{dev,test,staging,prod} for
# real environment-specific bundles; this script only closes the Docker gap.
set -euo pipefail

CHECKOUT="${1:-${SDKWORK_SPACE_CHECKOUT_HOST_PATH:-${SDKWORK_SPACE_HOST_PATH:-/opt/deploy}/sdkwork-space}}"
PROFILE="${SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE:-standalone}"
ALIASES=(dev test staging prod)

log() {
  echo "[sdkwork-webserver-dist-aliases] $*"
}

copy_seed_into_alias() {
  local seed="$1"
  local dest="$2"
  mkdir -p "${dest}"
  if [ -d "${seed}" ] && [ "${seed}" = "$(dirname "${dest}")" ]; then
    # Seed is the bare dist/ directory; exclude sibling alias folders.
    find "${seed}" -mindepth 1 -maxdepth 1 \
      ! -name 'standalone' ! -name 'cloud' \
      ! -name 'dev' ! -name 'test' ! -name 'staging' ! -name 'prod' \
      -exec cp -a -t "${dest}" {} +
  else
    cp -a "${seed}/." "${dest}/"
  fi
}

resolve_seed_for_app() {
  local app="$1"
  local module_dir="$2"
  local dist_root="${app}/dist"
  local profile_root="${dist_root}/${PROFILE}"
  local seed=""
  local alias
  if [ -d "${profile_root}" ]; then
    for alias in "${ALIASES[@]}"; do
      if [ -f "${profile_root}/${alias}/index.html" ]; then
        printf '%s' "${profile_root}/${alias}"
        return 0
      fi
    done
  fi
  if [ -d "${dist_root}" ]; then
    # Migration seeds: legacy environment-only subtrees and bare dist/.
    for alias in "${ALIASES[@]}"; do
      if [ -f "${dist_root}/${alias}/index.html" ]; then
        printf '%s' "${dist_root}/${alias}"
        return 0
      fi
    done
    if [ -f "${dist_root}/index.html" ]; then
      printf '%s' "${dist_root}"
      return 0
    fi
  fi
  if [ -f "${module_dir}/deployments/webserver/static/index.html" ]; then
    printf '%s' "${module_dir}/deployments/webserver/static"
    return 0
  fi
  return 1
}

if [ ! -d "${CHECKOUT}" ]; then
  log "checkout missing: ${CHECKOUT}"
  exit 1
fi

log "materializing dist aliases under ${CHECKOUT} (profile=${PROFILE})"
count=0
for module_dir in "${CHECKOUT}"/sdkwork-*; do
  [ -d "${module_dir}" ] || continue
  module="$(basename "${module_dir}")"
  case "${module}" in
    sdkwork-webserver) continue ;;
  esac
  apps_root="${module_dir}/apps"
  [ -d "${apps_root}" ] || continue
  for app in "${apps_root}"/*-pc "${apps_root}"/*-h5; do
    [ -d "${app}" ] || continue
    dist_root="${app}/dist"
    seed=""
    if ! seed="$(resolve_seed_for_app "${app}" "${module_dir}")"; then
      continue
    fi
    for alias in "${ALIASES[@]}"; do
      dest="${dist_root}/${PROFILE}/${alias}"
      if [ -f "${dest}/index.html" ]; then
        continue
      fi
      copy_seed_into_alias "${seed}" "${dest}"
      count=$((count + 1))
      log "seeded ${module}/$(basename "${app}")/dist/${PROFILE}/${alias} <- ${seed#"${CHECKOUT}/"}"
    done
  done
done
log "materialized ${count} missing dist/${PROFILE}/<alias> tree(s)"
