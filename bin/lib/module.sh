#!/usr/bin/env bash
# module.sh — sdkwork-webserver bin/ wiring (MODULE_BIN_SPEC.md §3).
# Only module identity, constants, and repo-command delegation live here.
# Every shared primitive (remote execution, packaging, checksums) comes from
# sdkwork-specs/bin/lib/sdkwork-common.sh.

SDKWORK_MODULE_ID="sdkwork-webserver"
SDKWORK_IMAGE_NAME="sdkwork-webserver-standalone"
SDKWORK_APP_TYPES="pc,h5,server"

# Operations wiring (OPERATIONS_SPEC.md): the compose service carrying the
# health probe, the probe path, and the per-environment host port it is
# published on (DOCKER_SPEC.md §3 port matrix).
SDKWORK_PRIMARY_SERVICE="webserver"
SDKWORK_HEALTH_PATH="/healthz"
SDKWORK_CONFIG_ENV_SUBDIR="env"

# Install/upgrade resolve the newest packaged install bundle via the shared
# default (newest under dist/docker-install) — this hook is OPTIONAL
# (MODULE_BIN_SPEC.md §3) and the default is already correct, so there is no
# override. There is deliberately no source-tree bundle: the executors are
# authored flat under bin/ (docker-bundle-deploy.sh / -release.sh) and the
# compose/env inputs live in deployments/docker/, so an un-packaged install
# fails fast with packaging guidance instead of pushing a directory that cannot
# run on the target.

# Source-tree env dir; used for --dry-run rendering and by config.sh.
sdkwork_module_local_env_dir() {
  printf '%s/deployments/docker/env' "${SDKWORK_MODULE_ROOT}"
}

# Live host port for the management/app port 3800 of an environment instance.
# Resolution order so bin/doctor probes what is actually published, not a
# stale fallback (MODULE_BIN_SPEC.md §4.7):
#   (1) the deployed env-file host-port key for <env> when doctor has already
#       pulled the live env into SDKWORK_CONFIG_KEYS/VALUES (covers --host-port
#       CLI overrides that were upserted into the env file, and config.sh --set);
#   (2) the built-in per-environment default below.
# The <instance> offset follows the deploy.sh stride (host port base + idx - 1),
# so instance N is probed on base + N - 1 (DOCKER_SPEC.md §3.2).
sdkwork_module_health_port() {
  local env="${1:-}" instance="${2:-1}" seg key
  case "${env}" in
    development) seg="DEV" ;; test) seg="TEST" ;; staging) seg="STAGING" ;;
    demo) seg="DEMO" ;; production) seg="PROD" ;; *) return 0 ;;
  esac
  key="SDKWORK_WEBSERVER_${seg}_HOST_PORT"
  local base=""
  if ((${#SDKWORK_CONFIG_KEYS[@]} > 0)); then
    sdkwork_config_index_of "${key}"
    if (( SDKWORK_CONFIG_INDEX >= 0 )); then
      base="${SDKWORK_CONFIG_VALUES[SDKWORK_CONFIG_INDEX]}"
    fi
  fi
  if [[ -z "${base}" ]]; then
    case "${env}" in
      development) base="13800" ;;
      test)        base="18888" ;;
      staging)     base="18081" ;;
      demo)        base="19080" ;;
      production)  base="18080" ;;
    esac
  fi
  case "${instance}" in ''|*[!0-9]*) instance=1 ;; esac
  printf '%s' "$((base + instance - 1))"
}

# §9.3 layout conformance probe (APPLICATION_DEPLOY_LAYOUT_SPEC.md §9):
# read-only verification of the host deploy root on the target. Doctor emits
# these rows only when a deploy root actually exists on the target host.
sdkwork_module_extra_doctor() {
  local environment="$1"
  if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
    sdkwork_doctor_row PASS layout "dry-run: deploy-root layout scan skipped"
    return 0
  fi
  # a) deploy root exists at all (soft: a non-deployed host has no root).
  sdkwork_doctor_capture "${SDKWORK_BIN_HOST}" bash -lc 'test -d /opt/deploy && echo yes || echo no'
  if [[ "${SDKWORK_DOCTOR_OUT}" != "yes" ]]; then
    sdkwork_doctor_row WARN layout "no host deploy root at /opt/deploy on ${SDKWORK_BIN_HOST} (docker deployments only)"
    return 0
  fi
  # b) no loose operation logs at the deploy root (§9.1 anti-drift rule).
  sdkwork_doctor_capture "${SDKWORK_BIN_HOST}" bash -lc \
    'find /opt/deploy -maxdepth 1 -type f \( -name "*.log" -o -name "*.txt" \) | head -5'
  if [[ -n "${SDKWORK_DOCTOR_OUT}" ]]; then
    sdkwork_doctor_row WARN layout "loose files at deploy root (move to /opt/deploy/logs|archives): ${SDKWORK_DOCTOR_OUT//$'\n'/, }"
  else
    sdkwork_doctor_row PASS layout "deploy root free of loose log/text files"
  fi
  # c) canonical install root + bundle for this module (§9.1).
  sdkwork_doctor_capture "${SDKWORK_BIN_HOST}" bash -lc \
    'test -d /opt/deploy/sdkwork-webserver/bundle && echo bundle-ok || echo bundle-missing'
  if [[ "${SDKWORK_DOCTOR_OUT}" == "bundle-ok" ]]; then
    sdkwork_doctor_row PASS layout "/opt/deploy/sdkwork-webserver/{bundle} present"
  else
    sdkwork_doctor_row WARN layout "/opt/deploy/sdkwork-webserver/bundle missing for '${environment}' (install first)"
  fi
  # d) shared checkout surface present (§9.2: one checkout, all stacks).
  sdkwork_doctor_capture "${SDKWORK_BIN_HOST}" bash -lc \
    'test -d /opt/deploy/sdkwork-space && echo space-ok || echo space-missing'
  case "${SDKWORK_DOCTOR_OUT}" in
    space-ok)      sdkwork_doctor_row PASS layout "shared checkout /opt/deploy/sdkwork-space present" ;;
    space-missing) sdkwork_doctor_row WARN layout "shared checkout /opt/deploy/sdkwork-space absent (webserver overlay cannot resolve sidecars)" ;;
  esac
}

# Delegates to the repository's canonical deployment validator.
sdkwork_module_config_validate() {
  local env_file="$1"
  sdkwork_local_run node scripts/docker/validate-docker-deployment.mjs --env-file "${env_file}"
}

# ----------------------------------------------------------------------------
# Container image (docker-image.sh build)
# ----------------------------------------------------------------------------
sdkwork_image_build() {
  local ref="$1" tag="$2"
  # Delegates to the repository's canonical container build (MODULE_BIN_SPEC.md
  # §4.1). Invoked through `node` rather than `pnpm run`: this workspace is
  # routinely operated straight from the DrvFs checkout (/mnt/e), where the
  # Windows-installed node_modules records a Windows storeDir in
  # node_modules/.modules.yaml and pnpm aborts with "disk I/O error" on the 9p
  # mount. package.json build:container:standalone is exactly this invocation,
  # so the canonical build stays single-sourced.
  local args=(node scripts/docker/build-standalone-image.mjs --tag "${tag}")
  # Registry-unreachable host (no daemon proxy): `docker build --pull` cannot
  # refresh base-image metadata and dies on TLS timeouts — reuse the cached base
  # image instead.
  if [[ "${SDKWORK_IMAGE_NO_PULL:-0}" == "1" ]]; then args+=(--no-pull); fi
  # Reuse an already produced release archive instead of repackaging it.
  if [[ "${SDKWORK_IMAGE_SKIP_RELEASE_BUILD:-0}" == "1" ]]; then args+=(--skip-release-build); fi
  sdkwork_local_run "${args[@]}"
}

# ----------------------------------------------------------------------------
# Application build (apps-build.sh)
# ----------------------------------------------------------------------------
sdkwork_build_app() {
  local app_type="$1" environment="$2" profile="$3"
  local alias
  alias="$(sdkwork_environment_alias "${environment}")"
  case "${app_type}" in
    pc|h5)
      local args=(node "${SDKWORK_SPECS_ROOT}/tools/build-browser-client.mjs"
                  --root "${SDKWORK_MODULE_ROOT}"
                  --architecture "${app_type}"
                  --environment "${alias}")
      if [[ "${profile}" == "cloud" ]]; then args+=(--deployment-profile cloud); fi
      # ENVIRONMENT_SPEC.md sanctions --skip-typecheck for workspace-wide
      # dependency debt: the typecheck walks sibling checkouts (sdkwork-iam,
      # sdkwork-skills, …) whose TS errors are not owned by this module.
      if [[ "${SDKWORK_BROWSER_SKIP_TYPECHECK:-0}" == "1" ]]; then args+=(--skip-typecheck); fi
      sdkwork_local_run "${args[@]}" ;;
    server)
      sdkwork_local_run cargo build --release ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" "unsupported app type '${app_type}' (declared: ${SDKWORK_APP_TYPES})" ;;
  esac
}

# ----------------------------------------------------------------------------
# Application packaging (apps-package.sh)
# ----------------------------------------------------------------------------
sdkwork_package_app() {
  local app_type="$1" environment="$2" profile="$3" out="$4"
  local alias
  alias="$(sdkwork_environment_alias "${environment}")"
  case "${app_type}" in
    pc|h5)
      local src="${SDKWORK_MODULE_ROOT}/apps/sdkwork-webserver-${app_type}/dist/${profile}/${alias}"
      sdkwork_require_dir "${src}" "run: bin/apps-build.sh ${app_type} ${environment}:${profile}"
      sdkwork_tar_artifact "${src}" \
        "${out}/sdkwork-webserver-${app_type}-${profile}-${alias}.tar.gz" ;;
    server)
      # Host-native installer channel: webserver-deb.mjs ships test|production
      # only. Every other environment is delivered by the container bundle.
      local installer_env
      case "${environment}" in
        test)       installer_env="test" ;;
        production) installer_env="production" ;;
        *) sdkwork_die "${SDKWORK_BIN_E_STATE}" \
             "the host-native .deb installer covers test|production only (got '${environment}'); use the container path: bin/docker-image.sh build && bin/docker-deploy.sh install --environment ${environment}" ;;
      esac
      sdkwork_local_run node scripts/webserver-deb.mjs package --environment "${installer_env}"
      sdkwork_collect_artifact "${SDKWORK_MODULE_ROOT}/dist/installers" "${out}" '*.deb' ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" "unsupported app type '${app_type}'" ;;
  esac
}

# ----------------------------------------------------------------------------
# Native installer packaging (apps-pkg-installer.sh, MODULE_BIN_SPEC.md §4.9)
# ----------------------------------------------------------------------------
sdkwork_installer_app() {
  local app_type="$1" platform="$2" environment="$3" profile="$4" out="$5" arch="$6" format="$7"
  case "${app_type}:${platform}" in
    server:linux)
      # Host-native installer channel: the .deb/.rpm builders cover
      # test|production only; every other environment is delivered by the
      # container bundle (§4.4 rule).
      case "${environment}" in
        test|production) ;;
        *) sdkwork_die "${SDKWORK_BIN_E_STATE}" \
             "the host-native Linux installers cover test|production only (got '${environment}'); use the container path: bin/docker-image.sh save && bin/docker-deploy.sh install --environment ${environment}" ;;
      esac
      case "${format}" in
        ""|deb)
          sdkwork_local_run node scripts/webserver-deb.mjs package --environment "${environment}" --architecture "${arch}"
          sdkwork_collect_artifact "${SDKWORK_MODULE_ROOT}/dist/installers" "${out}" '*.deb' ;;
        rpm)
          sdkwork_local_run node scripts/webserver-rpm.mjs package --environment "${environment}" --architecture "${arch}"
          sdkwork_collect_artifact "${SDKWORK_MODULE_ROOT}/dist/installers" "${out}" '*.rpm' ;;
        *) sdkwork_die "${SDKWORK_BIN_E_USAGE}" \
             "unsupported Linux installer format '${format}' (use deb|rpm)" ;;
      esac ;;
    server:*)
      sdkwork_die "${SDKWORK_BIN_E_STATE}" \
        "sdkwork-webserver ships host-native installers for Linux (deb|rpm) only; '${platform}' server delivery is the container image channel (bin/docker-image.sh save)" ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" \
        "app type '${app_type}' has no native installer channel (pc/h5 are static web bundles: bin/apps-package.sh ${app_type})" ;;
  esac
}

# ----------------------------------------------------------------------------
# Application deployment (apps-deploy.sh)
# ----------------------------------------------------------------------------
sdkwork_deploy_app() {
  local app_type="$1" action="$2" environment="$3" profile="$4" host="$5"
  case "${app_type}" in
    pc|h5)
      sdkwork_warn "'${app_type}' delivery is channel-owned: the webserver serves these bundles from its static root."
      sdkwork_log "next: bin/apps-package.sh ${app_type} ${environment}:${profile}, then publish the artifact into the webserver static root (or ship it inside the container image)."
      ;;
    server)
      local installer_env service port
      case "${environment}" in
        test)       installer_env="test";       service="sdkwork-webserver-test"; port="8888" ;;
        production) installer_env="production"; service="sdkwork-webserver";      port="8080" ;;
        *) sdkwork_die "${SDKWORK_BIN_E_STATE}" \
             "the host-native .deb channel covers test|production only (got '${environment}'); use bin/docker-deploy.sh install --environment ${environment}" ;;
      esac
      local staging="/opt/deploy/${SDKWORK_MODULE_ID}/packages"
      local deb base
      deb="$(sdkwork_require_artifact "${SDKWORK_MODULE_ROOT}/dist/installers" '*.deb' \
             "run: bin/apps-package.sh server ${environment}")"
      base="$(basename "${deb}")"

      case "${action}" in
        status)
          sdkwork_service_status "${host}" "${service}"
          sdkwork_health_probe "${host}" "http://127.0.0.1:${port}/healthz" ;;
        install|upgrade)
          # Requires a remote account with root privileges (apt + systemd).
          sdkwork_push_dir "${host}" "$(dirname "${deb}")" "${staging}"
          local apt_args=(env DEBIAN_FRONTEND=noninteractive apt-get install -y --reinstall)
          if [[ "${action}" == "upgrade" ]]; then apt_args+=(--allow-downgrades); fi
          apt_args+=("${staging}/${base}")
          sdkwork_remote "${host}" "${apt_args[@]}"
          sdkwork_service_enable_now "${host}" "${service}"
          sdkwork_health_probe "${host}" "http://127.0.0.1:${port}/healthz" ;;
        rollback)
          sdkwork_die "${SDKWORK_BIN_E_STATE}" \
            "the .deb channel keeps no install history; package the previous version (bin/apps-package.sh server ${environment}) and re-run 'bin/apps-deploy.sh server install ${environment}'" ;;
      esac ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" "unsupported app type '${app_type}'" ;;
  esac
}
