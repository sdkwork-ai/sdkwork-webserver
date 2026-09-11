#!/usr/bin/env bash
# apps-static.sh — build-apps-static capability: build the Adaptive Web static
# dist (PC / H5) of any independent sibling module in the sdkwork workspace.
#
# Delegation only: the actual build always goes through the canonical runner
# `sdkwork-specs/tools/build-browser-client.mjs` (PNPM_SCRIPT_SPEC.md §4.2,
# FRONTEND_CODE_SPEC.md §7) so environment materialization, cloud API base
# URLs, typecheck and dist layout (`dist/<profile>/<envAlias>`) stay identical
# to every other browser build in the workspace.
#
# Shared primitives (sdkwork_die / sdkwork_log / sdkwork_run /
# sdkwork_local_run / sdkwork_split_env_profile / sdkwork_tar_artifact /
# evidence trap) come from bin/lib/bootstrap.sh, which dispatches here as
# `sdkwork_entry_apps_static`.

# Vite config names mirrored from build-browser-client.mjs (VITE_CONFIG_NAMES).
SDKWORK_APPS_STATIC_VITE_CONFIGS="vite.config.ts vite.config.mts vite.config.js vite.config.mjs vite.config.web.ts vite.config.web.mjs vite.config.browser.ts vite.config.browser.mjs"

sdkwork_apps_static_usage() {
  sdkwork_log "build-apps-static.sh — build PC/H5 static dist for a workspace sibling module's apps/"
  sdkwork_log ""
  sdkwork_log "Usage:"
  sdkwork_log "  bin/build-apps-static.sh <module> [pc|h5|all] [dev|test|staging|demo|prod[:standalone|cloud]] [options]"
  sdkwork_log ""
  sdkwork_log "Arguments:"
  sdkwork_log "  <module>          sibling module under the workspace root ('sdkwork-im' or short 'im')"
  sdkwork_log "  [pc|h5|all]       architectures to build (default: all)"
  sdkwork_log "  [env[:profile]]   environment + deployment profile (default: dev:standalone)"
  sdkwork_log ""
  sdkwork_log "Options:"
  sdkwork_log "  --out <dir>        copy each dist/<profile>/<env> into <out>/<module>-<arch>-<profile>-<env>/"
  sdkwork_log "  --tar              also write <module>-<arch>-<profile>-<env>.tar.gz (+ .sha256) into --out (default: target/bin-packages)"
  sdkwork_log "  --clean            delete the target dist directory before building"
  sdkwork_log "  --skip-typecheck   forward --skip-typecheck to the canonical runner (fast iteration builds)"
  sdkwork_log "  --arch pc|h5|all   architecture via flag (alternative to the positional)"
  sdkwork_log "  --environment ...  environment[:profile] via flag (alternative to the positional)"
  sdkwork_log "  --profile ...      standalone|cloud via flag (alternative to env:profile)"
  sdkwork_log "  --workspace-root d workspace root holding the sibling modules (default: parent of this module)"
  sdkwork_log "  --list             list workspace modules that own buildable pc/h5 apps"
  sdkwork_log "  --dry-run          print the plan, execute nothing"
  sdkwork_log "  -h|--help          this help"
  sdkwork_log ""
  sdkwork_log "Examples:"
  sdkwork_log "  bin/build-apps-static.sh im                          # pc + h5, dev:standalone"
  sdkwork_log "  bin/build-apps-static.sh sdkwork-im h5 prod          # h5 only, production standalone"
  sdkwork_log "  bin/build-apps-static.sh im all test:cloud --out target/static"
  sdkwork_log "  bin/build-apps-static.sh im --skip-typecheck --out target/static --tar"
  sdkwork_log ""
  sdkwork_log "The build delegates to the canonical runner (PNPM_SCRIPT_SPEC.md §4.2);"
  sdkwork_log "output lands in <module>/apps/<app>/dist/<profile>/<envAlias>/ and every"
  sdkwork_log "run appends an evidence line to target/bin-evidence/evidence.log."
}

# Detect the single browser app root for one architecture under <module>/apps.
# Mirrors discoverBrowserAppRoots/resolveBrowserAppRoot in
# sdkwork-specs/tools/build-browser-client.mjs: an app dir whose name ends in
# `-pc` / `-h5` and carries a Vite config at its root.
# Sets SDKWORK_APPS_STATIC_APP_ROOT; returns 0 when found, 1 when absent.
# strict=1 turns a "multiple roots" ambiguity into a hard error.
sdkwork_apps_static_detect_app() {
  SDKWORK_APPS_STATIC_APP_ROOT=""
  local module_root="$1" arch="$2" strict="${3:-0}"
  local dir cfg found="" extra=""
  for dir in "${module_root}"/apps/*-"${arch}"; do
    [[ -d "${dir}" ]] || continue
    for cfg in ${SDKWORK_APPS_STATIC_VITE_CONFIGS}; do
      if [[ -f "${dir}/${cfg}" ]]; then
        if [[ -n "${found}" ]]; then
          if [[ "${strict}" == "1" ]]; then
            sdkwork_die "${SDKWORK_BIN_E_STATE}" \
              "multiple ${arch} browser app roots under ${module_root}/apps (${found} + ${dir}); the canonical runner requires exactly one"
          fi
          sdkwork_warn "skipping ${dir}: a second ${arch} browser app root (${found}) already matched"
          return 1
        fi
        found="${dir}"
        break
      fi
    done
  done
  [[ -n "${found}" ]] || return 1
  SDKWORK_APPS_STATIC_APP_ROOT="${found}"
  return 0
}

sdkwork_apps_static_resolve_module() {
  local name="$1" ws="$2" candidate=""
  case "${name}" in
    ''|*[!A-Za-z0-9._-]*) sdkwork_die "${SDKWORK_BIN_E_USAGE}" \
      "invalid module name '${name}' (allowed: letters, digits, dot, underscore, dash)" ;;
  esac
  if [[ -d "${ws}/${name}" ]]; then
    candidate="${ws}/${name}"
  elif [[ -d "${ws}/sdkwork-${name}" ]]; then
    candidate="${ws}/sdkwork-${name}"
  else
    sdkwork_die "${SDKWORK_BIN_E_USAGE}" \
      "module '${name}' not found under ${ws} (looked for ${name} and sdkwork-${name}); run --list to see buildable modules"
  fi
  [[ -f "${candidate}/package.json" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" \
    "${candidate} has no package.json (not an sdkwork module checkout)"
  [[ -d "${candidate}/apps" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" \
    "${candidate} has no apps/ directory (nothing to build statically)"
  SDKWORK_APPS_STATIC_MODULE_ROOT="${candidate}"
}

sdkwork_apps_static_list() {
  local ws="$1" dir row arch
  sdkwork_log "workspace root: ${ws}"
  sdkwork_log "buildable browser modules (apps/*-pc | apps/*-h5 with a Vite config):"
  for dir in "${ws}"/sdkwork-*; do
    [[ -d "${dir}" ]] || continue
    row=""
    for arch in pc h5; do
      if sdkwork_apps_static_detect_app "${dir}" "${arch}"; then
        row="${row} ${arch}:apps/$(basename "${SDKWORK_APPS_STATIC_APP_ROOT}")"
      fi
    done
    if [[ -n "${row}" ]]; then
      sdkwork_log "  $(basename "${dir}")${row}"
    fi
  done
  return 0
}

# Build (and optionally collect) one architecture. Emits a friendly error when
# the module owns no <arch> browser app.
sdkwork_apps_static_build_arch() {
  local module_root="$1" arch="$2" skip_typecheck="$3" do_clean="$4" out="$5" do_tar="$6"
  sdkwork_apps_static_detect_app "${module_root}" "${arch}" 1 \
    || sdkwork_die "${SDKWORK_BIN_E_STATE}" \
      "no ${arch} browser app with a Vite config under ${module_root}/apps (run: bin/build-apps-static.sh <module> --list)"

  local app_root="${SDKWORK_APPS_STATIC_APP_ROOT}"
  local alias dist_rel dist_dir
  alias="$(sdkwork_environment_alias "${SDKWORK_BIN_ENVIRONMENT}")"
  dist_rel="dist/${SDKWORK_BIN_PROFILE}/${alias}"
  dist_dir="${app_root}/${dist_rel}"
  sdkwork_log "apps-static: $(basename "${module_root}")/$(basename "${app_root}") ${arch} ${SDKWORK_BIN_PROFILE}.${SDKWORK_BIN_ENVIRONMENT} -> ${dist_rel}"

  if [[ "${do_clean}" == "1" && -d "${dist_dir}" ]]; then
    sdkwork_run rm -rf -- "${dist_dir}"
  fi

  # Canonical runner delegation (identical to `build:<arch>:<env>` scripts).
  local args=(node "${SDKWORK_SPECS_ROOT}/tools/build-browser-client.mjs"
              --root "${module_root}"
              --architecture "${arch}"
              --environment "${alias}")
  if [[ "${SDKWORK_BIN_PROFILE}" == "cloud" ]]; then
    args+=(--deployment-profile cloud)
  fi
  if [[ "${skip_typecheck}" == "1" ]]; then
    args+=(--skip-typecheck)
  fi
  sdkwork_local_run "${args[@]}"

  if [[ "${SDKWORK_BIN_DRY_RUN}" != "1" ]]; then
    local count="-"
    if [[ -d "${dist_dir}" ]]; then
      count="$(find "${dist_dir}" -type f 2>/dev/null | wc -l | tr -d ' ')"
    fi
    sdkwork_log "apps-static: ${arch} dist at ${dist_dir} (${count} files)"
  fi

  # --out: copy the static file tree into a stable, self-describing folder.
  if [[ -n "${out}" ]]; then
    local name dest
    name="$(basename "${module_root}")-${arch}-${SDKWORK_BIN_PROFILE}-${alias}"
    dest="${out}/${name}"
    sdkwork_require_dir "${dist_dir}" "run the build first"
    if [[ "${SDKWORK_BIN_DRY_RUN}" == "1" ]]; then
      sdkwork_log "dry-run: copy ${dist_dir} -> ${dest}"
    else
      mkdir -p "${out}"
      rm -rf -- "${dest}"
      sdkwork_run cp -R "${dist_dir}" "${dest}"
      sdkwork_log "apps-static: static bundle copied -> ${dest}"
    fi
  fi

  # --tar: tar.gz + sidecar checksum via the shared packaging primitive.
  if [[ "${do_tar}" == "1" ]]; then
    local tar_dir="${out:-${module_root}/target/bin-packages}"
    local artifact="${tar_dir}/$(basename "${module_root}")-${arch}-${SDKWORK_BIN_PROFILE}-${alias}.tar.gz"
    sdkwork_tar_artifact "${dist_dir}" "${artifact}"
  fi
}

sdkwork_entry_apps_static() {
  local module="" arch="all" spec="" profile_opt="" out="" ws=""
  local arch_given=0 do_list=0 do_tar=0 do_clean=0 skip_typecheck=0
  while (($#)); do
    case "$1" in
      --list) do_list=1; shift ;;
      --dry-run) SDKWORK_BIN_DRY_RUN=1; shift ;;
      --out) out="$(sdkwork_anchor_invocation_path "$(sdkwork_need_value --out "${2-}")")"; shift 2 ;;
      --tar) do_tar=1; shift ;;
      --clean) do_clean=1; shift ;;
      --skip-typecheck) skip_typecheck=1; shift ;;
      --workspace-root) ws="$(sdkwork_need_value --workspace-root "${2-}")"; shift 2 ;;
      --environment) spec="$(sdkwork_need_value --environment "${2-}")"; shift 2 ;;
      --profile) profile_opt="$(sdkwork_validate_profile "$(sdkwork_need_value --profile "${2-}")")"; shift 2 ;;
      --arch|-a) arch="$(sdkwork_need_value --arch "${2-}")"; arch_given=1; shift 2 ;;
      -h|--help) sdkwork_apps_static_usage; return 0 ;;
      -*) sdkwork_die "${SDKWORK_BIN_E_USAGE}" "unknown option '$1' for build-apps-static.sh" ;;
      *)
        if [[ -z "${module}" ]]; then
          module="$1"
        elif [[ "${arch_given}" == "0" && "$1" =~ ^(pc|h5|all)$ ]]; then
          arch="$1"; arch_given=1
        elif [[ -z "${spec}" ]]; then
          spec="$1"
        else
          sdkwork_die "${SDKWORK_BIN_E_USAGE}" "unexpected extra argument '$1' for build-apps-static.sh"
        fi
        shift
        ;;
    esac
  done

  # Workspace root: the directory holding the sibling module checkouts.
  if [[ -z "${ws}" ]]; then
    ws="$(cd "${SDKWORK_MODULE_ROOT}/.." && pwd)"
  fi
  [[ -d "${ws}" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" "workspace root not found: ${ws}"

  if [[ "${do_list}" == "1" ]]; then
    sdkwork_evidence "apps-static --list ${ws}"
    sdkwork_apps_static_list "${ws}"
    return 0
  fi

  [[ -n "${module}" ]] || { sdkwork_apps_static_usage; return "${SDKWORK_BIN_E_USAGE}"; }

  case "${arch}" in
    pc|h5|all) ;;
    *) sdkwork_die "${SDKWORK_BIN_E_ENV}" "unknown architecture '${arch}' (use pc|h5|all)" ;;
  esac

  # Environment defaults to dev:standalone; --profile composes with a bare env.
  if [[ -z "${spec}" ]]; then
    spec="dev"
    sdkwork_log "apps-static: no environment given, defaulting to dev:standalone"
  fi
  if [[ -n "${profile_opt}" ]]; then
    [[ "${spec}" != *:* ]] || sdkwork_die "${SDKWORK_BIN_E_USAGE}" \
      "--profile cannot be combined with an environment spec that already carries ':<profile>'"
    spec="${spec}:${profile_opt}"
  fi
  sdkwork_split_env_profile "${spec}"
  sdkwork_validate_environment "${SDKWORK_BIN_ENVIRONMENT}" >/dev/null

  sdkwork_apps_static_resolve_module "${module}" "${ws}"
  local module_root="${SDKWORK_APPS_STATIC_MODULE_ROOT}"

  sdkwork_evidence "apps-static ${module} ${arch} ${SDKWORK_BIN_ENVIRONMENT}:${SDKWORK_BIN_PROFILE} out=${out:-<dist>}"

  if [[ "${arch}" == "all" ]]; then
    sdkwork_apps_static_build_arch "${module_root}" pc "${skip_typecheck}" "${do_clean}" "${out}" "${do_tar}"
    sdkwork_apps_static_build_arch "${module_root}" h5 "${skip_typecheck}" "${do_clean}" "${out}" "${do_tar}"
  else
    sdkwork_apps_static_build_arch "${module_root}" "${arch}" "${skip_typecheck}" "${do_clean}" "${out}" "${do_tar}"
  fi
  return 0
}
