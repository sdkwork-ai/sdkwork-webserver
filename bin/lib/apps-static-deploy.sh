#!/usr/bin/env bash
# apps-static-deploy.sh — deploy-apps-static capability: package the static
# dist of a sibling module's apps/ and publish it to a target host (local WSL
# or remote ssh), with extraction, verification, atomic current-symlink
# switch and quick rollback.
#
# Target layout (per app, POSIX paths on the target):
#   <target-root>/<module>-<arch>-<profile>-<env>/
#     releases/<UTC-timestamp>/    extracted static files (immutable)
#     current -> releases/<ts>     atomic switch point (ln -sfn)
#     incoming/                    upload + sha256 staging (cleared after use)
#
# Reuses the build capability in bin/lib/apps-static.sh and the unified
# target primitives in sdkwork-specs/bin/lib/sdkwork-common.sh
# (sdkwork_remote / sdkwork_remote_capture, wsl and ssh://[user@]host[:port]).

sdkwork_apps_static_deploy_usage() {
  sdkwork_log "deploy-apps-static.sh — build, package and publish PC/H5 static dist to a target host"
  sdkwork_log ""
  sdkwork_log "Usage:"
  sdkwork_log "  bin/deploy-apps-static.sh <module> [pc|h5|all] [env[:profile]] [deploy|rollback|status] [options]"
  sdkwork_log ""
  sdkwork_log "Actions:"
  sdkwork_log "  deploy    (default) build -> tar.gz(+sha256) -> upload -> verify -> extract -> prune -> switch current"
  sdkwork_log "  rollback  point 'current' at the previous release (or --to <release-id>)"
  sdkwork_log "  status    show current pointer and all releases on the target"
  sdkwork_log ""
  sdkwork_log "Options:"
  sdkwork_log "  --host <t>          target host: wsl (default) or ssh://[user@]host[:port]"
  sdkwork_log "  --target-root <dir> POSIX root on the target (default /opt/deploy/sdkwork-static-apps)"
  sdkwork_log "  --artifacts <dir>   where local tar.gz(+sha256) packages are kept (default target/static-packages)"
  sdkwork_log "  --keep <n>          releases to retain on the target (default 5; the new/current release is never pruned)"
  sdkwork_log "  --to <release>      rollback target release id (YYYYmmddTHHMMSSZ)"
  sdkwork_log "  --no-build          deploy the existing dist without rebuilding"
  sdkwork_log "  --skip-typecheck    forward --skip-typecheck to the canonical runner during build"
  sdkwork_log "  --clean             delete the dist directory before building"
  sdkwork_log "  --yes               confirm production mutations (required for env=production)"
  sdkwork_log "  --dry-run           print the plan, execute nothing"
  sdkwork_log "  -h|--help           this help"
  sdkwork_log ""
  sdkwork_log "Examples:"
  sdkwork_log "  bin/deploy-apps-static.sh im h5 test --host wsl"
  sdkwork_log "  bin/deploy-apps-static.sh im all prod --host ssh://ops@10.0.0.8 --yes"
  sdkwork_log "  bin/deploy-apps-static.sh im h5 prod rollback --host ssh://ops@10.0.0.8 --yes"
  sdkwork_log "  bin/deploy-apps-static.sh im h5 prod rollback --to 20260910T071500Z --host wsl --yes"
  sdkwork_log "  bin/deploy-apps-static.sh im h5 dev status --host wsl"
  sdkwork_log "  bin/deploy-apps-static.sh im h5 test --host wsl --no-build --dry-run"
}

# Upload one local file to the target (binary-safe stdin stream; the same
# transport sdkwork_push_dir uses for tar streams).
sdkwork_apps_static_deploy_upload() {
  local host="$1" src="$2" dest="$3"
  if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
    sdkwork_log "dry-run: upload ${src} -> ${dest}"
    return 0
  fi
  sdkwork_remote "${host}" bash -lc "cat > $(printf '%q' "${dest}")" < "${src}"
}

# Run a small shell script on the target (dry-run prints it instead).
sdkwork_apps_static_deploy_remote_script() {
  local host="$1" script="$2"
  if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
    sdkwork_log "dry-run: remote script on ${host}:"
    sdkwork_log "  ${script}"
    return 0
  fi
  sdkwork_remote "${host}" bash -lc "${script}"
}

# Build (unless --no-build) and package one architecture into tar.gz + sha256.
# Sets SDKWORK_APPS_STATIC_PACKAGE / _APP_NAME / _PACKAGE_DIGEST on success.
sdkwork_apps_static_deploy_package_arch() {
  local module_root="$1" arch="$2" no_build="$3" skip_typecheck="$4" do_clean="$5" artifacts="$6"
  local env_alias

  if [[ "${no_build}" != "1" ]]; then
    sdkwork_apps_static_build_arch "${module_root}" "${arch}" "${skip_typecheck}" "${do_clean}" "" 0
  else
    sdkwork_apps_static_detect_app "${module_root}" "${arch}" 1 \
      || sdkwork_die "${SDKWORK_BIN_E_STATE}" \
        "no ${arch} browser app with a Vite config under ${module_root}/apps"
  fi

  env_alias="$(sdkwork_environment_alias "${SDKWORK_BIN_ENVIRONMENT}")"
  local dist_dir="${SDKWORK_APPS_STATIC_APP_ROOT}/dist/${SDKWORK_BIN_PROFILE}/${env_alias}"
  sdkwork_require_dir "${dist_dir}" "build it first (drop --no-build or run build-apps-static.sh)"
  if [[ ! -f "${dist_dir}/index.html" ]]; then
    sdkwork_die "${SDKWORK_BIN_E_STATE}" "incomplete dist at ${dist_dir} (no index.html)"
  fi

  local app_name package
  app_name="$(basename "${module_root}")-${arch}-${SDKWORK_BIN_PROFILE}-${env_alias}"
  mkdir -p "${artifacts}"
  package="${artifacts}/${app_name}.tar.gz"
  sdkwork_tar_artifact "${dist_dir}" "${package}"

  SDKWORK_APPS_STATIC_PACKAGE="${package}"
  SDKWORK_APPS_STATIC_APP_NAME="${app_name}"
}

# deploy action for one architecture: upload -> verify -> extract -> prune -> switch.
sdkwork_apps_static_deploy_push_arch() {
  local host="$1" target_root="$2" keep="$3"
  local app_dir="${target_root%/}/${SDKWORK_APPS_STATIC_APP_NAME}"
  local package="${SDKWORK_APPS_STATIC_PACKAGE}"
  local name digest release_ts meta
  name="$(basename "${package}")"
  if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
    digest="<sha256>"  # sdkwork_tar_artifact skips artifacts under --dry-run
  else
    digest="$(sed -n 's/^ *\([0-9a-f]\{64\}\) .*/\1/p' "${package}.sha256" | head -1)"
    [[ -n "${digest}" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" "no digest in ${package}.sha256"
  fi
  release_ts="$(date -u +%Y%m%dT%H%M%SZ)"
  meta="${SDKWORK_MODULE_ID} ${SDKWORK_APPS_STATIC_APP_NAME} ${SDKWORK_BIN_PROFILE}.${SDKWORK_BIN_ENVIRONMENT} ${release_ts} sha256:${digest}"

  sdkwork_log "apps-static-deploy: ${SDKWORK_APPS_STATIC_APP_NAME} -> ${host}:${app_dir} (release ${release_ts})"

  # 1) staging dir + upload package and sidecar
  sdkwork_apps_static_deploy_remote_script "${host}" \
    "set -e; mkdir -p $(printf '%q' "${app_dir}/releases") $(printf '%q' "${app_dir}/incoming")"
  sdkwork_apps_static_deploy_upload "${host}" "${package}" "${app_dir}/incoming/${name}"
  sdkwork_apps_static_deploy_upload "${host}" "${package}.sha256" "${app_dir}/incoming/${name}.sha256"

  # 2) integrity check on the target before anything is unpacked
  sdkwork_apps_static_deploy_remote_script "${host}" \
    "set -e; cd $(printf '%q' "${app_dir}/incoming") && sha256sum -c $(printf '%q' "${name}.sha256")" # PORTABILITY:target-linux

  # 3) extract into an immutable release dir, verify the entrypoint, record
  #    metadata, clear staging, prune (newest <keep> survive; this new release
  #    is the newest so it is never pruned), then atomically switch current.
  sdkwork_apps_static_deploy_remote_script "${host}" "set -e
app=$(printf '%q' "${app_dir}")
rel=\${app}/releases/${release_ts}
[ ! -e \"\${rel}\" ] || { echo 'release ${release_ts} already exists' >&2; exit 67; }
mkdir -p \"\${rel}\"
tar --no-same-permissions -xzf \"\${app}/incoming/$(printf '%q' "${name}")\" -C \"\${rel}\"
test -f \"\${rel}/index.html\" || { echo 'extracted release has no index.html' >&2; exit 67; }
printf '%s\\n' $(printf '%q' "${meta}") > \"\${rel}/.sdkwork-static-release\"
rm -f \"\${app}\"/incoming/*
cd \"\${app}/releases\"
ls -1 | sort -r | tail -n +$((keep + 1)) | while read -r r; do rm -rf -- \"./\$r\"; done
ln -sfn \"releases/${release_ts}\" \"\${app}/current\"
echo \"current -> \$(readlink \"\${app}/current\")\""

  sdkwork_log "apps-static-deploy: published ${name} as release ${release_ts} (keep=${keep})"
}

# Fetch release state from the target: first line "CURRENT:<id>" (or NONE),
# then one release id per line, newest first. Empty output under --dry-run.
sdkwork_apps_static_deploy_release_list() {
  local host="$1" app_dir="$2"
  sdkwork_remote_capture "${host}" bash -lc "set -e
app=$(printf '%q' "${app_dir}")
if [ ! -L \"\${app}/current\" ]; then echo NONE; exit 0; fi
echo \"CURRENT:\$(basename \"\$(readlink \"\${app}/current\")\")\"
[ -d \"\${app}/releases\" ] && cd \"\${app}/releases\" && ls -1 | sort -r"
}

# rollback action for one architecture. Selection runs locally against the
# release list pulled from the target, so it is testable and quoting-safe.
sdkwork_apps_static_deploy_rollback_arch() {
  local host="$1" target_root="$2" to_release="$3"
  local app_dir="${target_root%/}/${SDKWORK_APPS_STATIC_APP_NAME}"

  if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
    sdkwork_log "dry-run: rollback ${SDKWORK_APPS_STATIC_APP_NAME} current -> ${to_release:-<previous-release>} on ${host}:${app_dir}"
    return 0
  fi

  local raw cur="" prev="" line found=0
  raw="$(sdkwork_apps_static_deploy_release_list "${host}" "${app_dir}")"
  cur="$(printf '%s\n' "${raw}" | sed -n 's/^CURRENT://p')"
  if [[ -z "${cur}" ]]; then
    sdkwork_die "${SDKWORK_BIN_E_STATE}" \
      "no current release for ${SDKWORK_APPS_STATIC_APP_NAME} at ${host}:${app_dir} (deploy first)"
  fi
  while IFS= read -r line; do
    [[ -n "${line}" && "${line}" != CURRENT:* ]] || continue
    if [[ -n "${to_release}" ]]; then
      [[ "${line}" == "${to_release}" ]] && { prev="${line}"; break; }
    else
      if [[ "${found}" == "1" ]]; then prev="${line}"; break; fi
      [[ "${line}" == "${cur}" ]] && found=1
    fi
  done <<< "${raw}"
  if [[ -z "${to_release}" && -z "${prev}" && "${found}" != "1" ]]; then
    # Dangling current pointer (release dir gone): fall back to the newest.
    prev="$(printf '%s\n' "${raw}" | grep -v '^CURRENT:' | sed -n '1p')"
  fi

  if [[ -n "${to_release}" && -z "${prev}" ]]; then
    sdkwork_die "${SDKWORK_BIN_E_STATE}" \
      "release '${to_release}' not found for ${SDKWORK_APPS_STATIC_APP_NAME} (run 'status' to list releases)"
  fi
  if [[ -z "${prev}" ]]; then
    sdkwork_die "${SDKWORK_BIN_E_STATE}" \
      "no release older than '${cur}' for ${SDKWORK_APPS_STATIC_APP_NAME}; nothing to roll back to"
  fi

  sdkwork_apps_static_deploy_remote_script "${host}" \
    "set -e; ln -sfn $(printf '%q' "releases/${prev}") $(printf '%q' "${app_dir}/current") && echo \"current -> \$(readlink $(printf '%q' "${app_dir}/current"))\""
  sdkwork_log "apps-static-deploy: rolled back ${SDKWORK_APPS_STATIC_APP_NAME} ${cur} -> ${prev}"
}

# status action for one architecture.
sdkwork_apps_static_deploy_status_arch() {
  local host="$1" target_root="$2"
  local app_dir="${target_root%/}/${SDKWORK_APPS_STATIC_APP_NAME}"
  sdkwork_log "apps-static-deploy: ${SDKWORK_APPS_STATIC_APP_NAME} on ${host}:${app_dir}"
  if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
    sdkwork_log "dry-run: readlink current; ls releases"
    return 0
  fi
  local raw cur line
  raw="$(sdkwork_apps_static_deploy_release_list "${host}" "${app_dir}")"
  cur="$(printf '%s\n' "${raw}" | sed -n 's/^CURRENT://p')"
  if [[ -z "${cur}" && "${raw}" != *CURRENT:* ]]; then
    sdkwork_log "  current: <none>"
    return 0
  fi
  sdkwork_log "  current: releases/${cur}"
  while IFS= read -r line; do
    [[ -n "${line}" && "${line}" != CURRENT:* ]] || continue
    sdkwork_log "  release: ${line}"
  done <<< "${raw}"
}

sdkwork_entry_deploy_apps_static() {
  local module="" arch="all" spec="" profile_opt="" action="deploy"
  local host="wsl" target_root="/opt/deploy/sdkwork-static-apps" keep=5 artifacts="" to_release=""
  local arch_given=0 no_build=0 skip_typecheck=0 do_clean=0
  while (($#)); do
    case "$1" in
      --host) host="$(sdkwork_need_value --host "${2-}")"; shift 2 ;;
      --target-root) target_root="$(sdkwork_need_value --target-root "${2-}")"; shift 2 ;;
      --artifacts) artifacts="$(sdkwork_anchor_invocation_path "$(sdkwork_need_value --artifacts "${2-}")")"; shift 2 ;;
      --keep) keep="$(sdkwork_need_value --keep "${2-}")"; shift 2 ;;
      --to) to_release="$(sdkwork_need_value --to "${2-}")"; shift 2 ;;
      --no-build) no_build=1; shift ;;
      --skip-typecheck) skip_typecheck=1; shift ;;
      --clean) do_clean=1; shift ;;
      --yes) SDKWORK_BIN_YES=1; shift ;;
      --dry-run) SDKWORK_BIN_DRY_RUN=1; shift ;;
      -h|--help) sdkwork_apps_static_deploy_usage; return 0 ;;
      -*) sdkwork_die "${SDKWORK_BIN_E_USAGE}" "unknown option '$1' for deploy-apps-static.sh" ;;
      *)
        if [[ "${arch_given}" == "0" && "$1" =~ ^(pc|h5|all)$ ]]; then
          arch="$1"; arch_given=1
        elif [[ "$1" =~ ^(deploy|rollback|status)$ ]]; then
          action="$1"
        elif [[ -z "${module}" ]]; then
          module="$1"
        elif [[ -z "${spec}" ]]; then
          spec="$1"
        else
          sdkwork_die "${SDKWORK_BIN_E_USAGE}" "unexpected extra argument '$1' for deploy-apps-static.sh"
        fi
        shift
        ;;
    esac
  done

  case "${keep}" in
    ''|*[!0-9]*) sdkwork_die "${SDKWORK_BIN_E_USAGE}" "--keep must be a number (got '${keep}')" ;;
  esac
  sdkwork_remote_parse "${host}" >/dev/null
  case "${target_root}" in
    /*) ;;
    *) sdkwork_die "${SDKWORK_BIN_E_USAGE}" "--target-root must be an absolute POSIX path on the target (got '${target_root}')" ;;
  esac
  [[ -n "${module}" ]] || { sdkwork_apps_static_deploy_usage; return "${SDKWORK_BIN_E_USAGE}"; }
  case "${arch}" in
    pc|h5|all) ;;
    *) sdkwork_die "${SDKWORK_BIN_E_ENV}" "unknown architecture '${arch}' (use pc|h5|all)" ;;
  esac
  if [[ -n "${to_release}" ]]; then
    [[ "${action}" == "rollback" ]] || sdkwork_die "${SDKWORK_BIN_E_USAGE}" "--to is only valid with the rollback action"
    case "${to_release}" in
      [0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9][0-9][0-9][0-9][0-9][0-9]Z) ;;
      *) sdkwork_die "${SDKWORK_BIN_E_USAGE}" "--to must be a release id like 20260910T071500Z (got '${to_release}')" ;;
    esac
  fi

  if [[ -z "${spec}" ]]; then
    spec="dev"
    sdkwork_log "apps-static-deploy: no environment given, defaulting to dev:standalone"
  fi
  sdkwork_split_env_profile "${spec}"

  # Mutating actions on production need --yes (plan-only dry-run is allowed).
  if [[ "${action}" != "status" && "${SDKWORK_BIN_DRY_RUN}" != "1" ]]; then
    sdkwork_confirm_production
  fi

  local ws
  ws="$(cd "${SDKWORK_MODULE_ROOT}/.." && pwd)"
  sdkwork_apps_static_resolve_module "${module}" "${ws}"
  local module_root="${SDKWORK_APPS_STATIC_MODULE_ROOT}"

  if [[ -z "${artifacts}" ]]; then
    artifacts="${SDKWORK_MODULE_ROOT}/target/static-packages"
  fi

  sdkwork_evidence "deploy-apps-static ${action} ${module} ${arch} ${SDKWORK_BIN_ENVIRONMENT}:${SDKWORK_BIN_PROFILE} host=${host} root=${target_root}"

  local arch_list a env_alias
  case "${arch}" in all) arch_list="pc h5" ;; *) arch_list="${arch}" ;; esac
  env_alias="$(sdkwork_environment_alias "${SDKWORK_BIN_ENVIRONMENT}")"

  for a in ${arch_list}; do
    SDKWORK_APPS_STATIC_APP_NAME="$(basename "${module_root}")-${a}-${SDKWORK_BIN_PROFILE}-${env_alias}"
    case "${action}" in
      deploy)
        sdkwork_apps_static_deploy_package_arch "${module_root}" "${a}" "${no_build}" "${skip_typecheck}" "${do_clean}" "${artifacts}"
        sdkwork_apps_static_deploy_push_arch "${host}" "${target_root}" "${keep}"
        ;;
      rollback)
        sdkwork_apps_static_deploy_rollback_arch "${host}" "${target_root}" "${to_release}"
        ;;
      status)
        sdkwork_apps_static_deploy_status_arch "${host}" "${target_root}"
        ;;
    esac
  done
  return 0
}
