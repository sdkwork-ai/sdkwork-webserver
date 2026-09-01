#!/usr/bin/env bash
# Container entrypoint for the standalone gateway image.
set -euo pipefail

CONFIG_ROOT="${SDKWORK_WEBSERVER_CONFIG_ROOT:-/etc/sdkwork/webserver}"
SECRETS_ROOT="${CONFIG_ROOT}/secrets"
RUNTIME_CONFIG_FILE="${CONFIG_ROOT}/config.toml"
GATEWAY_BINARY="/app/bin/sdkwork-api-webserver-standalone-gateway"
PLATFORM_GATEWAY_BINARY="${SDKWORK_MODULE_API_GATEWAY_BINARY:-/app/bin/sdkwork-api-cloud-gateway}"
PLATFORM_GATEWAY_INSTALL_ROOT="${SDKWORK_MODULE_API_GATEWAY_INSTALL_ROOT:-/opt/sdkwork/api-gateway}"
PLATFORM_GATEWAY_CONFIG="${SDKWORK_MODULE_API_GATEWAY_CONFIG_FILE:-/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml}"
PLATFORM_GATEWAY_SECRETS_ROOT="/run/secrets/sdkwork/api-gateway"
SERVICE_USER="${SDKWORK_SERVICE_USER:-sdkwork}"

log() {
  # Always stderr: stdout is captured by $(module_imports_toml) and similar
  # substitutions and must never leak into generated config.toml.
  echo "[sdkwork-webserver-entrypoint] $*" >&2
}

ensure_directory() {
  if [ -d "$1" ]; then
    return 0
  fi
  install -d -o "${SERVICE_USER}" -g "${SERVICE_USER}" -m 0750 "$1"
}

ensure_writable_directory() {
  local dir="$1"
  if [ -d "${dir}" ]; then
    return 0
  fi
  if ! touch "${dir%/*}/.sdkwork-write-test" 2>/dev/null; then
    log "warning: parent of ${dir} is read-only; skipping create"
    return 0
  fi
  rm -f "${dir%/*}/.sdkwork-write-test"
  ensure_directory "${dir}"
}

# /opt/deploy/drive is the host-shared Drive delivery cache root
# (DEPLOYMENT_SPEC.md container shared directories): the LRU website content
# cache writes immutable, content-addressed entries there so every webserver
# instance on the host shares one disk cache. The mount may be read-only or
# unwritable on some hosts; in that case the Rust cache layer disables itself
# and content keeps streaming from the Drive facade, so this only pre-creates
# the directory when the host path is writable.
ensure_drive_delivery_cache_root() {
  local cache_root="${SDKWORK_DRIVE_WEBSITE_CACHE_ROOT:-/opt/deploy/drive/website-cache}"
  if [ -d "${cache_root}" ]; then
    log "drive delivery cache root present: ${cache_root}"
    return 0
  fi
  if ! touch "${cache_root%/*}/.sdkwork-write-test" 2>/dev/null; then
    log "warning: ${cache_root%/*} is not writable; drive delivery cache stays disabled"
    return 0
  fi
  rm -f "${cache_root%/*}/.sdkwork-write-test"
  ensure_directory "${cache_root}"
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${cache_root%/*}" 2>/dev/null || true
  log "created drive delivery cache root: ${cache_root}"
}

ensure_secret_file() {
  local name="$1"
  local file="${SECRETS_ROOT}/${name}"
  if [ ! -s "${file}" ]; then
    ensure_directory "${SECRETS_ROOT}"
    openssl rand -hex 32 > "${file}"
    chown "${SERVICE_USER}:${SERVICE_USER}" "${file}"
    chmod 0600 "${file}"
  fi
}

# A2: canonical certificate inventory (/etc/sdkwork/certs/<domain>/).
# Operator/ACME material wins; a missing domain directory is bootstrapped
# with a self-signed certificate so HTTPS listeners (8430) start and can be
# replaced by real material without a restart. Rendered nginx.*.production.conf
# sidecars reference the canonical ACME layout /etc/sdkwork/certs/letsencrypt/
# <domain>/fullchain.pem (W25), so every bootstrapped domain is mirrored into
# that layout too — never overwriting operator-provisioned files there.
lets_encrypt_certs_root() {
  printf '%s' "${SDKWORK_WEBSERVER_CERTS_LETS_ENCRYPT_DIR:-/etc/sdkwork/certs/letsencrypt}"
}

ensure_domain_certificate() {
  local domain="$1"
  local directory="${SDKWORK_CERTS_DIR:-/etc/sdkwork/certs}/${domain}"
  local cert="${directory}/cert.pem"
  local key="${directory}/key.pem"
  if [ -s "${cert}" ] && [ -s "${key}" ]; then
    log "certificate present: ${domain}"
  else
    ensure_directory "${directory}"
    log "generating self-signed bootstrap certificate for ${domain} (replace under ${directory})"
    if ! openssl req -x509 -newkey rsa:2048 -nodes \
        -days 3650 \
        -keyout "${key}" -out "${cert}" \
        -subj "/CN=${domain}" \
        -addext "subjectAltName=DNS:${domain}" 2>/dev/null \
      && [ ! -s "${cert}" ]; then
      # Older openssl builds reject -addext; retry without the extension and
      # surface a visible warning instead of failing TLS bootstrap silently.
      log "warning: openssl -addext unsupported or failed for ${domain}; retrying without SAN"
      rm -f "${key}"
      openssl req -x509 -newkey rsa:2048 -nodes \
        -days 3650 \
        -keyout "${key}" -out "${cert}" \
        -subj "/CN=${domain}" >/dev/null 2>&1 || true
    fi
    if [ ! -s "${cert}" ] || [ ! -s "${key}" ]; then
      log "warning: failed to generate bootstrap certificate for ${domain}; HTTPS listeners for it stay unserved until operator/ACME material is provisioned"
    fi
    chmod 0600 "${key}" 2>/dev/null || true
  fi
  local le_dir le_root
  le_root="$(lets_encrypt_certs_root)"
  le_dir="${le_root}/${domain}"
  if [ ! -s "${le_dir}/fullchain.pem" ] || [ ! -s "${le_dir}/privkey.pem" ]; then
    ensure_directory "${le_dir}"
    cp "${cert}" "${le_dir}/fullchain.pem"
    cp "${key}" "${le_dir}/privkey.pem"
    cp "${cert}" "${le_dir}/chain.pem"
    chmod 0644 "${le_dir}/fullchain.pem" "${le_dir}/chain.pem"
    chmod 0600 "${le_dir}/privkey.pem"
    chown -R "${SERVICE_USER}:${SERVICE_USER}" "${le_dir}" 2>/dev/null || true
  fi
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${directory}"
}

# Imported production sidecars declare `listen 443 ssl` with canonical
# ACME-layout certificate paths (W25: /etc/sdkwork/certs/letsencrypt/<domain>/).
# Bootstrap any missing per-domain inventory so imported HTTPS listeners can
# start cold; operator/ACME material replaces it without config changes.
ensure_imported_sidecar_certificates() {
  local imports_root="${CONFIG_ROOT}/imports.d"
  local aggregator conf src_cert domain le_dir seen="" le_root
  le_root="$(lets_encrypt_certs_root)"
  [ -d "${imports_root}" ] || return 0
  for aggregator in "${imports_root}/import.conf.standalone" "${imports_root}/import.conf.cloud"; do
    [ -f "${aggregator}" ] || continue
    while IFS= read -r line; do
      case "${line}" in
        include\ *) ;;
        *) continue ;;
      esac
      conf="${line#include }"
      conf="${conf%;}"
      [ -f "${conf}" ] || continue
      grep -q 'listen[[:space:]][[:space:]]*443[[:space:]][[:space:]]*ssl' "${conf}" || continue
      while IFS= read -r src_cert; do
        [ -n "${src_cert}" ] || continue
        # Recognize the configured ACME-layout root first (so operators who
        # override SDKWORK_WEBSERVER_CERTS_LETS_ENCRYPT_DIR still get
        # bootstrapping) and the stock /etc/sdkwork/certs/letsencrypt/ path
        # second (W25).
        if [ "${src_cert#"${le_root}"/}" != "${src_cert}" ]; then
          domain="${src_cert#"${le_root}"/}"
          domain="${domain%%/*}"
        elif [ "${src_cert#*/letsencrypt/}" != "${src_cert}" ]; then
          domain="$(printf '%s' "${src_cert}" | awk -F'/letsencrypt/' '{print $2}' | cut -d/ -f1)"
        else
          continue
        fi
        [ -n "${domain}" ] || continue
        case ",${seen}," in
          *,"${domain}",*) continue ;;
        esac
        seen="${seen:+${seen},}${domain}"
        le_dir="$(lets_encrypt_certs_root)/${domain}"
        if [ ! -s "${le_dir}/fullchain.pem" ] || [ ! -s "${le_dir}/privkey.pem" ]; then
          log "bootstrapping missing sidecar certificate inventory for ${domain} (${le_dir})"
          ensure_domain_certificate "${domain}"
        else
          log "sidecar certificate inventory present for ${domain}"
        fi
      done < <(sed -n 's/^[[:space:]]*ssl_certificate[[:space:]][[:space:]]*\([^;][^;]*\);.*/\1/p' "${conf}")
    done < "${aggregator}"
  done
  # Fail-soft verification: inventory entries that are still absent after all
  # bootstrap attempts get a loud operator-facing warning.
  if [ -n "${seen}" ]; then
    local domain_check le_check
    IFS=',' read -r -a domain_checks <<< "${seen}"
    for domain_check in "${domain_checks[@]}"; do
      le_check="$(lets_encrypt_certs_root)/${domain_check}"
      if [ ! -s "${le_check}/fullchain.pem" ] || [ ! -s "${le_check}/privkey.pem" ]; then
        log "warning: certificate inventory for imported domain ${domain_check} incomplete under ${le_check}; provision operator/ACME material before exposing HTTPS"
      fi
    done
  fi
}

ensure_database_secret() {
  if [ -n "${SDKWORK_DATABASE_PASSWORD_FILE:-}" ]; then
    return 0
  fi
  local file="${SECRETS_ROOT}/database.secret"
  if [ -n "${SDKWORK_DATABASE_PASSWORD:-}" ]; then
    # Compose/env password wins over a stale volume secret so restarts after
    # password rotation do not keep failing auth against host PostgreSQL.
    ensure_directory "${SECRETS_ROOT}"
    printf '%s' "${SDKWORK_DATABASE_PASSWORD}" > "${file}"
    chmod 0600 "${file}"
    chown "${SERVICE_USER}:${SERVICE_USER}" "${file}"
  fi
}

# Credential-entry bootstrap Access-Token for the PC login page (see
# IAM_CREDENTIAL_ENTRY_SPEC.md). Production IAM rejects unsigned fixture JWTs;
# issue a tenant-signed session through the gateway after runtime config and
# database bootstrap are ready.
is_unsigned_credential_entry_fixture() {
  local file="$1"
  [ -s "${file}" ] || return 1
  local token
  token="$(tr -d '[:space:]' < "${file}")"
  case "${token}" in
    *.signature) return 0 ;;
    *) return 1 ;;
  esac
}

ensure_credential_entry_bootstrap_token() {
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  local file="${SECRETS_ROOT}/credential-entry-bootstrap-access-token"
  ensure_directory "${SECRETS_ROOT}"
  if is_unsigned_credential_entry_fixture "${file}"; then
    rm -f "${file}"
    log "removed stale unsigned credential-entry bootstrap Access-Token (${environment})"
  fi
  export SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID="${SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID:-100001}"
  export SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_APP_ID="${SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_APP_ID:-sdkwork-web}"
  if run_as_service_user "${GATEWAY_BINARY}" issue-credential-entry-bootstrap-token "${file}"; then
    chown "${SERVICE_USER}:${SERVICE_USER}" "${file}" 2>/dev/null || true
    chmod 0600 "${file}" 2>/dev/null || true
    log "issued IAM-signed credential-entry bootstrap Access-Token (${environment})"
    return 0
  fi
  case "${environment}" in
    development|test) ;;
    *)
      log "warning: IAM bootstrap token issuance failed; production login metadata will be unavailable until a private token is provisioned"
      return 0
      ;;
  esac
  b64url() { printf '%s' "$1" | openssl base64 -A | tr '+/' '-_' | tr -d '='; }
  local header='{"alg":"none","typ":"JWT"}'
  local expires="$(( $(date +%s) + 86400 * 365 ))"
  local session_id="bootstrap-local-${environment}"
  local payload
  payload="$(printf '%s' "{\"token_version\":1,\"token_type\":\"access\",\"app_id\":\"sdkwork-web\",\"deployment_mode\":\"local\",\"environment\":\"${environment}\",\"exp\":${expires},\"login_scope\":\"TENANT\",\"organization_id\":\"0\",\"permission_scope\":[],\"runtime_target\":\"browser\",\"session_id\":\"${session_id}\",\"tenant_id\":\"100001\",\"user_id\":\"0\"}")"
  printf '%s.%s.%s' "$(b64url "${header}")" "$(b64url "${payload}")" "signature" > "${file}"
  chown "${SERVICE_USER}:${SERVICE_USER}" "${file}"
  chmod 0600 "${file}"
  log "provisioned development fixture credential-entry bootstrap Access-Token for ${environment}"
}

apply_primary_domain() {  local domain="${SDKWORK_WEBSERVER_PRIMARY_DOMAIN:-sdkwork.com}"
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  # Role host server (APP_RUNTIME_TOPOLOGY_NAMING.md §9.2); applicationCode remains webserver.
  local host_role="server-dev"
  if [ "${environment}" = "test" ]; then
    host_role="server-test"
  elif [ "${environment}" = "production" ]; then
    host_role="server"
  fi
  local scheme="http"
  if [ "${environment}" = "production" ] && [ "${SDKWORK_WEBSERVER_PUBLIC_SCHEME:-http}" = "https" ]; then
    scheme="https"
  fi
  local host_port
  host_port="$(host_http_port_for_environment)"
  local public_url="${scheme}://${host_role}.${domain}:${host_port}"
  # When browsers use host-published ports (no host reverse proxy), keep
  # portless origins too for operator routing via Windows :80 -> import plane.
  local public_url_portless="${scheme}://${host_role}.${domain}"
  export SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL="${SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL:-${public_url}}"
  export SDKWORK_WEBSERVER_APPLICATION_APP_HTTP_URL="${SDKWORK_WEBSERVER_APPLICATION_APP_HTTP_URL:-${public_url}}"
  export SDKWORK_WEBSERVER_APPLICATION_BACKEND_HTTP_URL="${SDKWORK_WEBSERVER_APPLICATION_BACKEND_HTTP_URL:-${public_url}}"
  export SDKWORK_CORS_ALLOWED_ORIGINS="${SDKWORK_CORS_ALLOWED_ORIGINS:-$(default_docker_cors_allowed_origins "${scheme}" "${host_port}" "${domain}" "${environment}")}"
}

host_http_port_for_environment() {
  case "${SDKWORK_WEBSERVER_ENVIRONMENT:-development}" in
    development) printf '%s' "${SDKWORK_WEBSERVER_DEV_HOST_PORT:-13800}" ;;
    test) printf '%s' "${SDKWORK_WEBSERVER_TEST_HOST_PORT:-18888}" ;;
    staging) printf '%s' "${SDKWORK_WEBSERVER_STAGING_HOST_PORT:-18081}" ;;
    production) printf '%s' "${SDKWORK_WEBSERVER_PROD_HOST_PORT:-18080}" ;;
    *) printf '%s' "${SDKWORK_WEBSERVER_DEV_HOST_PORT:-13800}" ;;
  esac
}

default_docker_cors_allowed_origins() {
  local scheme="$1"
  local host_port="$2"
  local domain="$3"
  local environment="$4"
  local origins=""
  local host hosts
  case "${environment}" in
    development)
      hosts="server-dev.${domain} server-app-dev.${domain} server-admin-dev.${domain}"
      ;;
    test)
      hosts="server-test.${domain} server-app-test.${domain} server-admin-test.${domain}"
      ;;
    staging)
      hosts="server-staging.${domain} server-app-staging.${domain} server-admin-staging.${domain}"
      ;;
    production)
      hosts="server.${domain} server-app.${domain} server-admin.${domain} ${domain} app.${domain}"
      ;;
    *)
      hosts="server-dev.${domain} server-app-dev.${domain} server-admin-dev.${domain}"
      ;;
  esac
  for host in ${hosts}; do
    origins="${origins:+$origins,}${scheme}://${host}:${host_port}"
    origins="${origins},${scheme}://${host}"
  done
  origins="${origins},${scheme}://localhost:${host_port},${scheme}://127.0.0.1:${host_port}"
  # Registered desktop WebView custom schemes and mini program runtimes
  # (WEB_FRAMEWORK_SPEC §12): first-party client origins are always allowed.
  origins="${origins},app://dsh,app://birdcoder,app://sdkwork,app://dtupay,tauri://localhost,https://servicewechat.com"
  printf '%s' "${origins}"
}

# A3: SDKWork space — clone https://github.com/sdkwork-ai/sdkwork-space under
# $SDKWORK_SPACE_ROOT (default /opt/deploy), bind-mount the same host path into
# containers, auto-discover sdkwork-* module imports, materialize layout v3 TOML
# under /etc/sdkwork/webserver/modules/, and wire PC/H5 static roots from
# apps/*/dist/<envAlias> when present (SDKWORK_WEBSERVER_SPEC.md §13.6 / §17).

space_checkout_dir() {
  printf '%s/sdkwork-space' "${SDKWORK_SPACE_ROOT:-/opt/deploy}"
}

environment_dist_alias() {
  case "${1:-development}" in
    development) printf '%s' "dev" ;;
    test) printf '%s' "test" ;;
    staging) printf '%s' "staging" ;;
    production) printf '%s' "prod" ;;
    *) printf '%s' "dev" ;;
  esac
}

# Active import-set profile (SDKWORK_WEBSERVER_SPEC.md §17.3): both sets are
# always materialized; this selects which aggregator activates as import.conf.
webserver_import_profile() {
  printf '%s' "${SDKWORK_WEBSERVER_IMPORT_PROFILE:-cloud}"
}

# PC/H5 dist variant served behind imported module hosts (§13.6 /
# ENVIRONMENT_SPEC.md §5.1.0.1): follows the active import set so cloud-mode
# edges serve cloud builds (unified api-edge base URLs) and standalone-mode
# edges serve same-origin builds. Never mix one module's profile variants.
static_source_profile() {
  printf '%s' "${SDKWORK_WEBSERVER_STATIC_SOURCE_PROFILE:-$(webserver_import_profile)}"
}

module_repo_root() {
  local module_id="$1"
  printf '%s/%s' "$(space_checkout_dir)" "${module_id}"
}

clone_or_update_git_repo() {
  local url="$1"
  local dest="$2"
  if [ ! -d "${dest}/.git" ]; then
    if [ -e "${dest}" ]; then
      log "warning: ${dest} exists but is not a git checkout; skipping clone of ${url}"
      return 1
    fi
    log "cloning ${url} into ${dest}"
    if ! run_as_service_user git clone --depth 1 "${url}" "${dest}"; then
      log "warning: git clone failed for ${url}; continuing without this module"
      return 1
    fi
    return 0
  fi
  case "${SDKWORK_SPACE_CLONE_PULL:-true}" in
    1|true|TRUE|yes|YES)
      log "updating ${dest} (git fetch + pull --ff-only)"
      if ! run_as_service_user git -C "${dest}" fetch --depth 1 origin; then
        log "warning: git fetch failed for ${dest}; keeping existing checkout"
        return 0
      fi
      if ! run_as_service_user git -C "${dest}" pull --ff-only; then
        log "warning: git pull --ff-only failed for ${dest}; keeping existing checkout"
      fi
      ;;
    *)
      log "SDKWORK_SPACE_CLONE_PULL disabled; keeping ${dest}"
      ;;
  esac
  return 0
}

module_webserver_enabled() {
  local module_dir="$1"
  local common="${module_dir}/deployments/webserver/server.common.toml"
  [ -f "${common}" ] || return 1
  if grep -Eq '^enabled[[:space:]]*=[[:space:]]*false' "${common}"; then
    return 1
  fi
  return 0
}

discover_importable_modules() {
  local checkout discovered="" module_dir module_id auto_discover
  # Standard behavior (SDKWORK_WEBSERVER_SPEC.md §17): auto-import every
  # enabled sibling module's deployments/webserver/ from the space checkout.
  # AUTO_DISCOVER=true (default): import every enabled sibling.
  # SDKWORK_SPACE_MODULES pins an explicit list only when AUTO_DISCOVER is off.
  # Accept AUTO_DISCOVER as a compose/env alias of SDKWORK_SPACE_AUTO_DISCOVER.
  auto_discover="${SDKWORK_SPACE_AUTO_DISCOVER:-${AUTO_DISCOVER:-true}}"
  case "${auto_discover}" in
    1|true|TRUE|yes|YES) auto_discover="true" ;;
    0|false|FALSE|no|NO) auto_discover="false" ;;
  esac
  checkout="$(space_checkout_dir)"
  if [ "${auto_discover}" = "true" ]; then
    if [ -d "${checkout}" ]; then
      for module_dir in "${checkout}"/sdkwork-*; do
        [ -d "${module_dir}" ] || continue
        module_id="$(basename "${module_dir}")"
        case "${module_id}" in
          sdkwork-webserver) continue ;;
        esac
        if module_webserver_enabled "${module_dir}"; then
          discovered="${discovered:+$discovered,}${module_id}"
        fi
      done
    fi
    printf '%s' "${discovered}"
  else
    printf '%s' "${SDKWORK_SPACE_MODULES:-}"
  fi
}

# Platform API plane (api*.brand): ensure sdkwork-api-cloud-gateway deployments/
# webserver sidecar is available under the space checkout so import.conf includes
# multi-brand api-dev.* / api.* reverse-proxy (SDKWORK_WEBSERVER_SPEC.md §17).
ensure_platform_api_gateway_module_checkout() {
  local checkout module_root sibling candidate
  checkout="$(space_checkout_dir)"
  module_root="${checkout}/sdkwork-api-cloud-gateway"
  if [ -f "${module_root}/deployments/webserver/server.common.toml" ]; then
    return 0
  fi
  sibling="${SDKWORK_MODULE_API_GATEWAY_CHECKOUT:-}"
  if [ -z "${sibling}" ]; then
    for candidate in \
      "${checkout}/../sdkwork-api-cloud-gateway" \
      "${SDKWORK_SPACE_ROOT:-/opt/deploy}/sdkwork-api-cloud-gateway" \
      "/workspace/sdkwork-api-cloud-gateway" \
      "/app/../sdkwork-api-cloud-gateway"; do
      if [ -f "${candidate}/deployments/webserver/server.common.toml" ]; then
        sibling="$(cd "${candidate}" && pwd)"
        break
      fi
    done
  fi
  if [ -n "${sibling}" ] && [ -f "${sibling}/deployments/webserver/server.common.toml" ]; then
    ensure_directory "${checkout}"
    ln -sfn "${sibling}" "${module_root}"
    log "linked platform API gateway module ${module_root} -> ${sibling}"
    return 0
  fi
  log "warning: sdkwork-api-cloud-gateway webserver layout not found under ${checkout}; api*.brand reverse proxy will be missing until the module is present"
  return 1
}

ensure_platform_api_gateway_import_listed() {
  local modules module_root
  modules="${SDKWORK_SPACE_IMPORT_MODULES:-}"
  case ",${modules}," in
    *,sdkwork-api-cloud-gateway,*) return 0 ;;
  esac
  module_root="$(module_repo_root sdkwork-api-cloud-gateway)"
  if ! module_webserver_enabled "${module_root}"; then
    log "warning: sdkwork-api-cloud-gateway not importable; api*.brand hosts will not be reverse-proxied"
    return 1
  fi
  export SDKWORK_SPACE_IMPORT_MODULES="sdkwork-api-cloud-gateway${modules:+,${modules}}"
  log "ensured sdkwork-api-cloud-gateway import for platform API plane (api*.brand)"
  return 0
}

clone_sdkwork_space_modules() {
  local space_root="${SDKWORK_SPACE_ROOT:-/opt/deploy}"
  local checkout clone_url base module module_dir local_path
  export SDKWORK_SPACE_ROOT="${space_root}"
  if [ ! -d "${space_root}" ]; then
    ensure_writable_directory "${space_root}"
  fi

  checkout="$(space_checkout_dir)"
  if [ -d "${checkout}/.git" ]; then
    git config --global --add safe.directory "${checkout}" 2>/dev/null || true
  fi
  local_path="${SDKWORK_SPACE_LOCAL_PATH:-}"
  if [ -n "${local_path}" ] && [ -d "${local_path}" ] && [ ! -e "${checkout}" ]; then
    ln -sfn "${local_path}" "${checkout}"
    log "linked space checkout ${checkout} -> ${local_path}"
  fi

  clone_url="${SDKWORK_SPACE_CLONE_URL:-https://github.com/sdkwork-ai/sdkwork-space.git}"
  # Branch order is exhaustive and reachable: an existing git checkout is
  # pulled (honoring SDKWORK_SPACE_CLONE_PULL), a non-git non-empty directory
  # is kept as-is, and a missing OR empty directory is cloned fresh. The
  # previous chain never updated an existing checkout and never cloned into an
  # empty pre-created mount point.
  if [ -d "${checkout}/.git" ]; then
    clone_or_update_git_repo "${clone_url}" "${checkout}" || true
  elif [ -d "${checkout}" ] && [ -n "$(ls -A "${checkout}" 2>/dev/null || true)" ]; then
    log "using existing non-git space checkout at ${checkout}"
  elif [ -n "${clone_url}" ]; then
    clone_or_update_git_repo "${clone_url}" "${checkout}" || true
  fi

  base="${SDKWORK_SPACE_CLONE_BASE:-}"
  if [ -n "${base}" ] && [ -n "${SDKWORK_SPACE_MODULES:-}" ]; then
    IFS=',' read -r -a module_list <<< "${SDKWORK_SPACE_MODULES}"
    for module in "${module_list[@]}"; do
      module="$(printf '%s' "${module}" | xargs)"
      [ -z "${module}" ] && continue
      module_dir="$(module_repo_root "${module}")"
      if [ -d "${module_dir}/.git" ]; then
        clone_or_update_git_repo "${base}/${module}" "${module_dir}" || true
        continue
      fi
      if [ ! -e "${module_dir}" ]; then
        clone_or_update_git_repo "${base}/${module}" "${module_dir}" || true
      fi
    done
  fi

  ensure_platform_api_gateway_module_checkout || true
  export SDKWORK_SPACE_IMPORT_MODULES="$(discover_importable_modules)"
  ensure_platform_api_gateway_import_listed || true
  if touch "${space_root}/.sdkwork-write-test" 2>/dev/null; then
    rm -f "${space_root}/.sdkwork-write-test"
    chown -R "${SERVICE_USER}:${SERVICE_USER}" "${space_root}" 2>/dev/null || true
  else
    log "space root ${space_root} is read-only; using host bind mount as-is"
  fi
}

module_app_static_root_for_alias() {
  local module="$1"
  local surface="$2"
  local dist_alias="$3"
  local profile="${4:-$(static_source_profile)}"
  local apps_root expected app dist
  apps_root="$(module_repo_root "${module}")/apps"
  [ -d "${apps_root}" ] || return 0
  expected="${apps_root}/${module}-${surface}"
  # Canonical layout: apps/*-{pc,h5}/dist/<profile>/<alias>/
  if [ -f "${expected}/dist/${profile}/${dist_alias}/index.html" ]; then
    printf '%s' "${expected}/dist/${profile}/${dist_alias}"
    return 0
  fi
  for app in "${apps_root}"/*-"${surface}"; do
    [ -d "${app}" ] || continue
    dist="${app}/dist/${profile}/${dist_alias}"
    if [ -f "${dist}/index.html" ]; then
      printf '%s' "${dist}"
      return 0
    fi
  done
  # Migration fallback: legacy environment-only dist/<alias>/ (same alias only).
  if [ -f "${expected}/dist/${dist_alias}/index.html" ]; then
    printf '%s' "${expected}/dist/${dist_alias}"
    return 0
  fi
  for app in "${apps_root}"/*-"${surface}"; do
    [ -d "${app}" ] || continue
    dist="${app}/dist/${dist_alias}"
    if [ -f "${dist}/index.html" ]; then
      printf '%s' "${dist}"
      return 0
    fi
  done
  return 0
}

write_module_app_roots_catalog() {
  local module="$1"
  local module_root="$2"
  local catalog_root="$3"
  local environment dist_alias pc_root h5_root static_fallback
  local pc_dev pc_test pc_staging pc_prod
  local h5_dev h5_test h5_staging h5_prod
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  dist_alias="$(environment_dist_alias "${environment}")"
  pc_root="$(module_app_static_root "${module}" pc)"
  h5_root="$(module_app_static_root "${module}" h5)"
  static_fallback="${module_root}/deployments/webserver/static"
  pc_dev="$(module_app_static_root_for_alias "${module}" pc dev)"
  pc_test="$(module_app_static_root_for_alias "${module}" pc test)"
  pc_staging="$(module_app_static_root_for_alias "${module}" pc staging)"
  pc_prod="$(module_app_static_root_for_alias "${module}" pc prod)"
  h5_dev="$(module_app_static_root_for_alias "${module}" h5 dev)"
  h5_test="$(module_app_static_root_for_alias "${module}" h5 test)"
  h5_staging="$(module_app_static_root_for_alias "${module}" h5 staging)"
  h5_prod="$(module_app_static_root_for_alias "${module}" h5 prod)"
  # Adaptive Web table (SDKWORK_DEPLOY_SPEC.md §8): prefer discovered PC/H5
  # roots over static-fallback. Fall back to the active surface root before the
  # placeholder static directory so by_environment never collapses to
  # static-fallback while pc_static_root / h5_static_root point at real assets.
  cat > "${catalog_root}/${module}.toml" <<EOF
# Generated Adaptive Web catalog for imported module ${module}.
# Authority: SDKWORK_WEBSERVER_SPEC.md §13.6 / §17.
# Active dist alias: ${dist_alias} (lifecycle environment: ${environment})

[app_roots]
tablet_surface = "pc"
pc_static_root = "${pc_root:-}"
h5_static_root = "${h5_root:-}"
static_fallback_root = "${static_fallback}"

[app_roots.pc_static_by_environment]
development = "${pc_dev:-${pc_root:-${static_fallback}}}"
test = "${pc_test:-${pc_root:-${static_fallback}}}"
staging = "${pc_staging:-${pc_root:-${static_fallback}}}"
production = "${pc_prod:-${pc_root:-${static_fallback}}}"

[app_roots.h5_static_by_environment]
development = "${h5_dev:-${h5_root:-${static_fallback}}}"
test = "${h5_test:-${h5_root:-${static_fallback}}}"
staging = "${h5_staging:-${h5_root:-${static_fallback}}}"
production = "${h5_prod:-${h5_root:-${static_fallback}}}"

[app_roots.static_fallback_by_environment]
development = "${static_fallback}"
test = "${static_fallback}"
staging = "${static_fallback}"
production = "${static_fallback}"
EOF
  chown "${SERVICE_USER}:${SERVICE_USER}" "${catalog_root}/${module}.toml" 2>/dev/null || true
  chmod 0640 "${catalog_root}/${module}.toml" 2>/dev/null || true
  log "materialized module app-roots catalog -> ${catalog_root}/${module}.toml (pc=${pc_root:-none} h5=${h5_root:-none})"
}

materialize_module_toml_layout() {
  local module_root="$1"
  local dest="$2"
  local gateway_port="$3"
  local module_ws="${module_root}/deployments/webserver"
  rm -rf "${dest}"
  mkdir -p "${dest}"
  cp "${module_ws}/server.common.toml" "${dest}/"
  for env_name in development test staging production; do
    if [ -f "${module_ws}/server.${env_name}.toml" ]; then
      cp "${module_ws}/server.${env_name}.toml" "${dest}/"
    fi
  done
  if [ -f "${module_ws}/server.cloud.toml" ]; then
    cp "${module_ws}/server.cloud.toml" "${dest}/"
  fi
  if [ -f "${module_ws}/server.standalone.toml" ]; then
    sed -e "s/127.0.0.1:3800/127.0.0.1:${gateway_port}/g" \
        -e "s/127.0.0.1:18079/127.0.0.1:${gateway_port}/g" \
        -e "s/0.0.0.0:3900/127.0.0.1:${gateway_port}/g" \
      "${module_ws}/server.standalone.toml" > "${dest}/server.standalone.toml"
  fi
  log "materialized module webserver config -> ${dest} (layout v3 TOML, gateway upstream 127.0.0.1:${gateway_port})"
}

# Prepare per-module config trees. When nginx.enabled + sidecar exist, only the
# nginx `.conf` tree is materialized (never a competing TOML import descriptor).
webserver_container_gateway_port() {
  # Resolve each optional source through an explicit :-default so a bare
  # `set -u` environment never aborts mid-expansion.
  local port="${SDKWORK_WEBSERVER_CONTAINER_HEALTH_PORT:-}"
  if [ -z "${port}" ]; then
    local bind="${SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND:-}"
    port="${bind##*:}"
  fi
  printf '%s' "${port:-3800}"
}

materialize_module_webserver_configs() {
  local modules_root="${CONFIG_ROOT}/modules"
  local catalog_root="${CONFIG_ROOT}/module-app-roots"
  local module module_root dest
  ensure_directory "${modules_root}"
  ensure_directory "${catalog_root}"
  IFS=',' read -r -a module_list <<< "${SDKWORK_SPACE_IMPORT_MODULES:-}"
  for module in "${module_list[@]}"; do
    module="$(printf '%s' "${module}" | xargs)"
    [ -z "${module}" ] && continue
    module_root="$(module_repo_root "${module}")"
    if ! module_webserver_enabled "${module_root}"; then
      continue
    fi
    dest="${modules_root}/${module}"
    write_module_app_roots_catalog "${module}" "${module_root}" "${catalog_root}"
    # nginx.conf path wins exclusively; TOML layout is only for nginx.enabled=false.
    if module_nginx_import_enabled "${module_root}"; then
      continue
    fi
    materialize_module_toml_layout "${module_root}" "${dest}" "$(webserver_container_gateway_port)"
  done
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${modules_root}" 2>/dev/null || true
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${catalog_root}" 2>/dev/null || true
}

module_nginx_sidecar_abs_path() {
  local module_root="$1"
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  local profile="${2:-${SDKWORK_WEBSERVER_IMPORT_PROFILE:-${SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE:-${SDKWORK_DEPLOYMENT_PROFILE:-standalone}}}}"
  printf '%s/deployments/webserver/nginx.%s.%s.conf' "${module_root}" "${profile}" "${environment}"
}

module_nginx_conf_path() {
  local module_root="$1"
  local conf
  conf="$(module_nginx_sidecar_abs_path "${module_root}")"
  if [ -f "${conf}" ]; then
    printf '%s' "${conf}"
    return 0
  fi
  return 1
}

# SDKWORK_WEBSERVER_SPEC.md §4.2: nginx.enabled=true with a rendered sidecar
# activates nginx `.conf` import; otherwise the layout v3 TOML directory is used.
# The two import shapes are mutually exclusive for each module.
module_nginx_import_enabled() {
  local module_dir="$1"
  local common="${module_dir}/deployments/webserver/server.common.toml"
  [ -f "${common}" ] || return 1
  if grep -Eq '^enabled[[:space:]]*=[[:space:]]*false' "${common}"; then
    return 1
  fi
  if awk '
    /^\[nginx\]/ { in_nginx=1; next }
    /^\[/ { in_nginx=0 }
    in_nginx && /^[[:space:]]*enabled[[:space:]]*=[[:space:]]*false/ { found=1; exit }
    END { exit(found ? 0 : 1) }
  ' "${common}"; then
    return 1
  fi
  module_nginx_conf_path "${module_dir}" >/dev/null 2>&1
}

# First DNS-label token: memory-dev.sdkwork.com -> memory, mem-dev -> mem.
hostname_first_token() {
  local label="${1%%.*}"
  printf '%s' "${label%%-*}"
}

# Tokens this module owns (application code plus documented short aliases).
module_hostname_tokens() {
  local code="${1#sdkwork-}"
  case "${code}" in
    memory) printf '%s' "memory mem" ;;
    cloudrouter) printf '%s' "cloudrouter router" ;;
    api-cloud-gateway) printf '%s' "api" ;;
    *) printf '%s' "${code}" ;;
  esac
}

is_platform_api_gateway_module() {
  case "${1:-}" in
    sdkwork-api-cloud-gateway) return 0 ;;
    *) return 1 ;;
  esac
}

# Platform API plane (api*.xxx.com): every path proxies to sdkwork-api-cloud-gateway.
# Do not fall back to SPA static roots — that would serve placeholder HTML for /.
module_api_gateway_deployment() {
  printf '%s' "${SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT:-docker}"
}

module_api_gateway_port() {
  printf '%s' "${SDKWORK_MODULE_API_GATEWAY_PORT:-3900}"
}
module_api_gateway_upstream_endpoint() {
  local deployment host port
  deployment="$(module_api_gateway_deployment)"
  host="$(module_api_gateway_upstream_host "${deployment}")"
  port="$(module_api_gateway_port)"
  printf '%s:%s' "${host}" "${port}"
}
discover_module_api_gateway_allowed_hosts() {
  local environment profile imports_root conf line hosts brands brand api_hosts
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  profile="${SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE:-${SDKWORK_DEPLOYMENT_PROFILE:-standalone}}"
  imports_root="${CONFIG_ROOT}/imports.d/import.conf"
  hosts=""
  if [ -f "${imports_root}" ]; then
    while IFS= read -r line; do
      case "${line}" in
        include\ *)
          conf="${line#include }"
          conf="${conf%;}"
          [ -f "${conf}" ] || continue
          while IFS= read -r host; do
            [ -n "${host}" ] || continue
            case ",${hosts}," in
              *,"${host}",*) continue ;;
            esac
            hosts="${hosts:+${hosts},}${host}"
          done < <(sed -n 's/.*server_name[[:space:]]\+\([^;]*\);.*/\1/p' "${conf}" | tr ' ' '\n' | sed '/^$/d')
          ;;
      esac
    done < "${imports_root}"
  fi
  # Platform API plane hosts (api*.brand) — always allow even when import scan is empty.
  brands="sdkwork.com birdcoder.com dtupay.com sdkwork.cn birdcoder.cn dtupay.cn skubc.com skubc.cn zowalk.com zowalk.cn offer86.com offer86.cn 86offer.com 86offer.cn"
  api_hosts=""
  case "${environment}" in
    development)
      for brand in ${brands}; do
        api_hosts="${api_hosts:+${api_hosts},}api-dev.${brand}"
      done
      ;;
    test)
      for brand in ${brands}; do
        api_hosts="${api_hosts:+${api_hosts},}api-test.${brand}"
      done
      ;;
    staging)
      # APP_RUNTIME_TOPOLOGY_NAMING §9: staging uses the -staging suffix
      # (api-staging.<brand>), mirroring api-dev/api-test/api.
      for brand in ${brands}; do
        api_hosts="${api_hosts:+${api_hosts},}api-staging.${brand}"
      done
      ;;
    production)
      for brand in ${brands}; do
        api_hosts="${api_hosts:+${api_hosts},}api.${brand}"
      done
      ;;
  esac
  if [ -n "${api_hosts}" ]; then
    hosts="${hosts:+${hosts},}${api_hosts}"
  fi
  hosts="${hosts},127.0.0.1:$(module_api_gateway_port),localhost:$(module_api_gateway_port)"
  printf '%s' "${hosts}" | sed 's/^,//'
}

ensure_platform_api_gateway_directories() {
  ensure_directory "/etc/sdkwork/api-gateway"
  ensure_directory "${PLATFORM_GATEWAY_SECRETS_ROOT}"
  ensure_directory "/var/lib/sdkwork/api-gateway"
}

ensure_platform_api_gateway_secret_file() {
  local name="$1"
  local file="${PLATFORM_GATEWAY_SECRETS_ROOT}/${name}"
  if [ ! -s "${file}" ]; then
    ensure_platform_api_gateway_directories
    openssl rand -hex 32 > "${file}"
    chown "${SERVICE_USER}:${SERVICE_USER}" "${file}" 2>/dev/null || true
    chmod 0600 "${file}" 2>/dev/null || true
  fi
}

export_platform_api_gateway_database_env() {
  export SDKWORK_DATABASE_ENGINE="${SDKWORK_DATABASE_ENGINE:-postgresql}"
  export SDKWORK_DATABASE_HOST="${SDKWORK_DATABASE_HOST:-127.0.0.1}"
  export SDKWORK_DATABASE_PORT="${SDKWORK_DATABASE_PORT:-5432}"
  export SDKWORK_DATABASE_NAME="${SDKWORK_DATABASE_NAME:-sdkwork_ai_dev}"
  export SDKWORK_DATABASE_SCHEMA="${SDKWORK_DATABASE_SCHEMA:-${SDKWORK_DATABASE_NAME}}"
  export SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC="${SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC:-false}"
  export SDKWORK_DATABASE_USERNAME="${SDKWORK_DATABASE_USERNAME:-sdkwork_ai_dev}"
  export SDKWORK_DATABASE_SSL_MODE="${SDKWORK_DATABASE_SSL_MODE:-disable}"
  export SDKWORK_DATABASE_MAX_CONNECTIONS="${SDKWORK_DATABASE_MAX_CONNECTIONS:-50}"
  export SDKWORK_DATABASE_AUTO_MIGRATE="${SDKWORK_DATABASE_AUTO_MIGRATE:-false}"
}

export_platform_api_gateway_redis_env() {
  local redis_host redis_port redis_password redis_tls
  redis_host="${SDKWORK_WEBSERVER_REDIS_HOST:-${WEBSERVER_REDIS_HOST:-127.0.0.1}}"
  redis_port="${SDKWORK_WEBSERVER_REDIS_PORT:-${WEBSERVER_REDIS_PORT:-6379}}"
  redis_password="${SDKWORK_WEBSERVER_REDIS_PASSWORD:-${WEBSERVER_REDIS_PASSWORD:-}}"
  redis_tls="${SDKWORK_WEBSERVER_REDIS_TLS:-${WEBSERVER_REDIS_TLS:-false}}"
  export SDKWORK_CLOUDROUTER_REDIS_ENABLED="true"
  export SDKWORK_CLOUDROUTER_REDIS_HOST="${redis_host}"
  export SDKWORK_CLOUDROUTER_REDIS_PORT="${redis_port}"
  export SDKWORK_CLOUDROUTER_REDIS_DATABASE="${SDKWORK_CLOUDROUTER_REDIS_DATABASE:-0}"
  export SDKWORK_CLOUDROUTER_REDIS_TLS="${redis_tls}"
  export SDKWORK_CLOUDROUTER_REDIS_PASSWORD="${redis_password}"
  export SDKWORK_RTC_STATE_REDIS_URL="${SDKWORK_RTC_STATE_REDIS_URL:-redis://${redis_host}:${redis_port}/1}"
  export SDKWORK_API_CLOUD_GATEWAY_WEB_REDIS_URL="${SDKWORK_API_CLOUD_GATEWAY_WEB_REDIS_URL:-redis://${redis_host}:${redis_port}/2}"
}

export_platform_api_gateway_runtime_env() {
  local environment allowed_hosts
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  export SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=standalone
  export SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET=container
  export SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT="${environment}"
  export SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID="standalone.${environment}"
  export SDKWORK_ENVIRONMENT="${SDKWORK_ENVIRONMENT:-${environment}}"
  export SDKWORK_API_CLOUD_GATEWAY_BIND="${SDKWORK_MODULE_API_GATEWAY_BIND:-127.0.0.1:$(module_api_gateway_port)}"
  export SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE="${PLATFORM_GATEWAY_CONFIG}"
  export SDKWORK_APP_ROOT="${PLATFORM_GATEWAY_INSTALL_ROOT}"
  export SDKWORK_DATABASE_MODULES_ROOT="${PLATFORM_GATEWAY_INSTALL_ROOT}/database-modules"
  allowed_hosts="${SDKWORK_MODULE_API_GATEWAY_ALLOWED_HOSTS:-}"
  if [ -z "${allowed_hosts}" ]; then
    allowed_hosts="$(discover_module_api_gateway_allowed_hosts)"
  fi
  export SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS="${allowed_hosts}"
  export SDKWORK_CORS_ALLOWED_ORIGINS="${SDKWORK_MODULE_API_GATEWAY_CORS_ALLOWED_ORIGINS:-${SDKWORK_CORS_ALLOWED_ORIGINS:-}}"
  export SDKWORK_API_CLOUD_GATEWAY_MIGRATE_ON_START="${SDKWORK_MODULE_API_GATEWAY_MIGRATE_ON_START:-true}"
  export SDKWORK_API_CLOUD_GATEWAY_PROVISION_IAM_SIGNING_MASTER_SECRET="${SDKWORK_MODULE_API_GATEWAY_PROVISION_IAM_SIGNING_MASTER_SECRET:-true}"
  export SDKWORK_IAM_SIGNING_MASTER_SECRET_FILE="${SDKWORK_IAM_SIGNING_MASTER_SECRET_FILE:-${PLATFORM_GATEWAY_SECRETS_ROOT}/iam-signing-master.key}"
  export SDKWORK_API_CLOUD_GATEWAY_PROVISION_PAYMENT_CREDENTIAL_KEY="${SDKWORK_MODULE_API_GATEWAY_PROVISION_PAYMENT_CREDENTIAL_KEY:-true}"
  export SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE="${SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE:-${PLATFORM_GATEWAY_SECRETS_ROOT}/payment-credential-master.key}"
  export SDKWORK_API_CLOUD_GATEWAY_PROVISION_KNOWLEDGEBASE_RPC_SECRETS="${SDKWORK_MODULE_API_GATEWAY_PROVISION_KNOWLEDGEBASE_RPC_SECRETS:-true}"
  export SDKWORK_KNOWLEDGEBASE_ENVIRONMENT="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  export SDKWORK_KNOWLEDGEBASE_DEPLOYMENT_PROFILE=standalone
  export SDKWORK_KNOWLEDGEBASE_TENANT_ID="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_TENANT_ID:-100001}"
  export SDKWORK_KNOWLEDGEBASE_ACTOR_ID="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_ACTOR_ID:-1}"
  export SDKWORK_KNOWLEDGEBASE_SECRETS_ENCRYPTION_KEY_FILE="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/knowledgebase-secrets.key"
  export SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY_FILE="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/webserver-secrets.key"
  export SDKWORK_IM_GROUP_KNOWLEDGEBASE_LAUNCH_TICKET_SECRET_FILE="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/im-launch-ticket.key"
  export SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_ENDPOINT="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_RPC_ENDPOINT:-https://127.0.0.1:50054}"
  export SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CA_CERT_PATH="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/ca.crt"
  export SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CERT_PATH="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/client.crt"
  export SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_KEY_PATH="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/client.key"
  export SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_TLS_DOMAIN="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_RPC_TLS_DOMAIN:-knowledgebase.local}"
  export SDKWORK_IM_KNOWLEDGEBASE_RPC_CLIENT_CALLER_CONTEXT_SIGNING_KEY_FILE="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc/caller-context.key"
  export SDKWORK_IM_PRINCIPAL_DIRECTORY="${SDKWORK_MODULE_API_GATEWAY_IM_PRINCIPAL_DIRECTORY:-postgres}"
  export SDKWORK_IM_ID_NODE_ID="${SDKWORK_MODULE_API_GATEWAY_IM_ID_NODE_ID:-2}"
  export SDKWORK_API_CLOUD_GATEWAY_IAM_ALLOWED_AUDIENCES="${SDKWORK_MODULE_API_GATEWAY_IAM_ALLOWED_AUDIENCES:-sdkwork-api-cloud-gateway}"
  export_platform_api_gateway_database_env
  export_platform_api_gateway_redis_env
}

wait_for_bundled_knowledgebase_rpc() {
  local secrets_root ca attempt max_attempts
  secrets_root="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc"
  ca="${secrets_root}/ca.crt"
  max_attempts="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_RPC_WAIT_ATTEMPTS:-60}"
  if [ ! -s "${ca}" ]; then
    log "warning: knowledgebase RPC CA missing at ${ca}; skipping RPC readiness wait"
    return 0
  fi
  for attempt in $(seq 1 "${max_attempts}"); do
    if curl -fsS --connect-timeout 2 --cacert "${ca}" "https://127.0.0.1:50054/healthz" >/dev/null 2>&1; then
      log "bundled knowledgebase RPC ready at https://127.0.0.1:50054/healthz"
      return 0
    fi
    sleep 2
  done
  log "bundled knowledgebase RPC not ready at https://127.0.0.1:50054/healthz after ${max_attempts} attempts"
  return 1
}

start_bundled_knowledgebase_rpc() {
  local deployment rpc_entrypoint
  deployment="$(module_api_gateway_deployment)"
  [ "${deployment}" = "bundled" ] || return 0
  rpc_entrypoint="${PLATFORM_GATEWAY_INSTALL_ROOT}/container/knowledgebase-rpc-entrypoint"
  if [ ! -x "${rpc_entrypoint}" ]; then
    log "warning: bundled knowledgebase RPC entrypoint missing at ${rpc_entrypoint}; continuing without RPC sidecar"
    return 0
  fi
  ensure_platform_api_gateway_directories
  export SDKWORK_KNOWLEDGEBASE_RPC_SECRETS_ROOT="${PLATFORM_GATEWAY_SECRETS_ROOT}/knowledgebase-rpc"
  export SDKWORK_KNOWLEDGEBASE_ENVIRONMENT="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  export SDKWORK_KNOWLEDGEBASE_DEPLOYMENT_PROFILE=standalone
  export SDKWORK_KNOWLEDGEBASE_RPC_ENABLED="true"
  export SDKWORK_KNOWLEDGEBASE_RPC_BIND_ADDR="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_RPC_BIND:-127.0.0.1:50054}"
  export SDKWORK_KNOWLEDGEBASE_DRIVE_STORAGE_ROOT="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_DRIVE_ROOT:-/var/lib/sdkwork/api-gateway/knowledgebase-drive}"
  export SDKWORK_KNOWLEDGEBASE_OPERATOR_ID=sdkwork-knowledgebase
  export SDKWORK_KNOWLEDGEBASE_ACTOR_ID="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_ACTOR_ID:-1}"
  export SDKWORK_KNOWLEDGEBASE_TENANT_ID="${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_TENANT_ID:-100001}"
  export SDKWORK_API_CLOUD_GATEWAY_PROVISION_KNOWLEDGEBASE_RPC_SECRETS="${SDKWORK_MODULE_API_GATEWAY_PROVISION_KNOWLEDGEBASE_RPC_SECRETS:-true}"
  export_platform_api_gateway_database_env
  log "starting bundled knowledgebase RPC (${rpc_entrypoint}) on ${SDKWORK_KNOWLEDGEBASE_RPC_BIND_ADDR}"
  run_as_service_user "${rpc_entrypoint}" \
    > "${SDKWORK_MODULE_API_GATEWAY_KNOWLEDGEBASE_RPC_LOG_FILE:-/var/lib/sdkwork/webserver/module-knowledgebase-rpc.log}" 2>&1 &
  PLATFORM_KB_RPC_PID=$!
  wait_for_bundled_knowledgebase_rpc || {
    log "warning: bundled knowledgebase RPC failed readiness; continuing gateway startup"
  }
}

wait_for_module_api_gateway() {
  local endpoint host port attempt max_attempts
  endpoint="$(module_api_gateway_upstream_endpoint)"
  host="${endpoint%%:*}"
  port="${endpoint##*:}"
  max_attempts="${SDKWORK_MODULE_API_GATEWAY_WAIT_ATTEMPTS:-}"
  if [ -z "${max_attempts}" ]; then
    case "${SDKWORK_MODULE_API_GATEWAY_REQUIRED:-false}" in
      1|true|TRUE|yes|YES) max_attempts=90 ;;
      # Docker/external: webserver must start with import reverse-proxy config
      # even when the independent gateway is temporarily down (502 until healthy).
      *) max_attempts=3 ;;
    esac
  fi
  for attempt in $(seq 1 "${max_attempts}"); do
    if curl -fsS --connect-timeout 2 "http://${host}:${port}/healthz" >/dev/null 2>&1; then
      log "module API gateway ready at http://${endpoint}/healthz"
      return 0
    fi
    sleep 1
  done
  log "module API gateway not ready at http://${endpoint}/healthz after ${max_attempts} attempts"
  return 1
}

start_bundled_module_api_gateway() {
  local deployment
  deployment="$(module_api_gateway_deployment)"
  [ "${deployment}" = "bundled" ] || return 0
  if [ ! -x "${PLATFORM_GATEWAY_BINARY}" ]; then
    log "bundled module API gateway binary missing at ${PLATFORM_GATEWAY_BINARY}"
    log "build sdkwork-api-cloud-gateway and mount it, or set SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker"
    exit 1
  fi
  if [ ! -d "${PLATFORM_GATEWAY_INSTALL_ROOT}/database-modules" ]; then
    log "bundled module API gateway install root missing database-modules at ${PLATFORM_GATEWAY_INSTALL_ROOT}"
    log "mount the cloud-gateway release/install tree to ${PLATFORM_GATEWAY_INSTALL_ROOT} or use docker deployment mode"
    exit 1
  fi
  ensure_platform_api_gateway_directories
  ensure_platform_api_gateway_secret_file "iam-signing-master.key"
  ensure_platform_api_gateway_secret_file "payment-credential-master.key"
  start_bundled_knowledgebase_rpc
  export_platform_api_gateway_runtime_env
  if [ "${SDKWORK_MODULE_API_GATEWAY_MIGRATE_ON_START:-true}" = "true" ]; then
    log "running bundled module API gateway database migrations"
    run_as_service_user "${PLATFORM_GATEWAY_BINARY}" --migrate-databases || {
      log "warning: bundled module API gateway --migrate-databases failed; continuing startup"
    }
  fi
  log "starting bundled module API gateway (${PLATFORM_GATEWAY_BINARY}) on ${SDKWORK_API_CLOUD_GATEWAY_BIND}"
  run_as_service_user "${PLATFORM_GATEWAY_BINARY}" \
    > "${SDKWORK_MODULE_API_GATEWAY_LOG_FILE:-/var/lib/sdkwork/webserver/module-api-gateway.log}" 2>&1 &
  PLATFORM_GATEWAY_PID=$!
  wait_for_module_api_gateway
}

prepare_module_api_gateway() {
  local deployment required
  deployment="$(module_api_gateway_deployment)"
  required="${SDKWORK_MODULE_API_GATEWAY_REQUIRED:-false}"
  case "${deployment}" in
    bundled)
      start_bundled_module_api_gateway
      ;;
    docker|external)
      # Default: do not block webserver startup on an independent gateway.
      # Import reverse-proxy config is already rewritten; /healthz on api*.brand
      # returns 502 until the operator's gateway is healthy.
      case "${required}" in
        1|true|TRUE|yes|YES)
          log "waiting for module API gateway (${deployment}) at $(module_api_gateway_upstream_endpoint)"
          if ! wait_for_module_api_gateway; then
            log "module API gateway required but not ready; aborting startup"
            exit 1
          fi
          return 0
          ;;
      esac
      log "module API gateway deployment=${deployment} upstream=$(module_api_gateway_upstream_endpoint); not waiting (set SDKWORK_MODULE_API_GATEWAY_REQUIRED=true to block)"
      return 0
      ;;
    *)
      log "unknown module API gateway deployment ${deployment}; skipping prepare"
      return 0
      ;;
  esac
}

# Product public edge (server-dev / server-app-dev / server-admin-dev; prod:
# server / server-app / server-admin). Sibling discovery skips sdkwork-webserver
# (SPEC §17), but expose.mode:api hosts MUST reverse-proxy to the process
# AdaptiveAppShell (SPEC §11.3 / §13.6). Upstream name is unique
# (`webserver_adaptive_shell`) so it does not collide with sibling `upstream
# gateway` blocks. Production emits one TLS server block per registered brand
# domain (443 ssl + 80, canonical ACME-layout certificates, W11/W25/W26); every
# environment exposes =/healthz and =/readyz probe locations.
product_edge_proxy_locations() {
  cat <<'PROXYEOF'
        location = /healthz {
            proxy_pass http://webserver_adaptive_shell;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
        location = /readyz {
            proxy_pass http://webserver_adaptive_shell;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
        location /api/ {
            proxy_pass http://webserver_adaptive_shell;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
        location / {
            proxy_pass http://webserver_adaptive_shell;
            proxy_http_version 1.1;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_buffering off;
            proxy_read_timeout 300s;
        }
PROXYEOF
}

materialize_product_edge_nginx_conf() {
  local imports_root="${CONFIG_ROOT}/imports.d"
  local environment product_root sidecar server_names mgmt_port dest
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  mgmt_port="$(webserver_container_gateway_port)"
  product_root="$(module_repo_root sdkwork-webserver)"
  sidecar="${product_root}/deployments/webserver/nginx.standalone.${environment}.conf"
  dest="${imports_root}/product-edge-nginx.conf"
  ensure_directory "${imports_root}"

  server_names=""
  if [ -f "${sidecar}" ]; then
    # Collect every server_name directive across ALL server blocks so
    # multi-brand production sidecars mirror completely (one TLS block per
    # registered brand domain), not just the first server stanza.
    server_names="$(sed -n 's/.*server_name[[:space:]]\+\([^;]*\);.*/\1/p' "${sidecar}" | xargs)"
  fi
  if [ -z "${server_names}" ]; then
    log "warning: product edge sidecar missing or has no server_name (${sidecar}); skipping product-edge import"
    rm -f "${dest}"
    return 1
  fi

  if [ "${environment}" = "production" ]; then
    local lets_root domains="" host hostdom domain_names
    lets_root="$(lets_encrypt_certs_root)"
    for host in ${server_names}; do
      hostdom="${host#*.}"
      case " ${domains} " in
        *" ${hostdom} "*) ;;
        *) domains="${domains:+${domains} }${hostdom}" ;;
      esac
    done
    {
      cat <<HEAD
# Generated by sdkwork-webserver-entrypoint (SDKWORK_WEBSERVER_SPEC.md §11.3 / §13.6).
# Product Adaptive Web edge: server / server-app / server-admin hosts proxy to the
# process AdaptiveAppShell. One TLS server block per registered brand domain;
# certificates follow the canonical ACME layout under ${lets_root}.
# Source server_name from ${sidecar}
user sdkwork;
worker_processes auto;
pid /run/sdkwork/webserver/product-edge.pid;
error_log /var/log/sdkwork/webserver/webserver/error.log warn;
events {
    worker_connections 1024;
}
http {
    sendfile on;
    keepalive_timeout 75;
    client_max_body_size 1100m;
    server_tokens off;
    gzip on;
    upstream webserver_adaptive_shell {
        least_conn;
        keepalive 32;
        server 127.0.0.1:${mgmt_port};
    }
HEAD
      for hostdom in ${domains}; do
        domain_names=""
        for host in ${server_names}; do
          if [ "${host#*.}" = "${hostdom}" ]; then
            domain_names="${domain_names:+${domain_names} }${host}"
          fi
        done
        cat <<TLSBLOCK
    server {
        listen 443 ssl;
        listen 80;
        server_name ${domain_names};
        ssl_certificate ${lets_root}/${hostdom}/fullchain.pem;
        ssl_certificate_key ${lets_root}/${hostdom}/privkey.pem;
        ssl_trusted_certificate ${lets_root}/${hostdom}/chain.pem;
        ssl_stapling on;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_prefer_server_ciphers on;
        ssl_session_cache shared:SSL:10m;
TLSBLOCK
        product_edge_proxy_locations
        printf '    }\n'
      done
      printf '}\n'
    } > "${dest}"
    log "materialized product Adaptive Web edge -> ${dest} (production TLS over ${domains}, upstream 127.0.0.1:${mgmt_port})"
  else
    cat > "${dest}" <<PLAINHEAD
# Generated by sdkwork-webserver-entrypoint (SDKWORK_WEBSERVER_SPEC.md §11.3 / §13.6).
# Product Adaptive Web edge: proxy server-* hosts to process AdaptiveAppShell.
# Source server_name from ${sidecar}
user sdkwork;
worker_processes auto;
pid /run/sdkwork/webserver/product-edge.pid;
error_log /var/log/sdkwork/webserver/webserver/error.log warn;
events {
    worker_connections 1024;
}
http {
    sendfile on;
    keepalive_timeout 75;
    client_max_body_size 1100m;
    server_tokens off;
    gzip on;
    upstream webserver_adaptive_shell {
        least_conn;
        keepalive 32;
        server 127.0.0.1:${mgmt_port};
    }
    server {
        listen 80;
        server_name ${server_names};
PLAINHEAD
    product_edge_proxy_locations >> "${dest}"
    printf '    }\n}\n' >> "${dest}"
    log "materialized product Adaptive Web edge -> ${dest} (http, upstream 127.0.0.1:${mgmt_port})"
  fi

  chown "${SERVICE_USER}:${SERVICE_USER}" "${dest}" 2>/dev/null || true
  chmod 0644 "${dest}" 2>/dev/null || true
  return 0
}

# Write imports.d/ import sets (SDKWORK_WEBSERVER_SPEC.md §17.3): both the
# standalone and cloud aggregators (import.conf.<profile>) plus optional
# per-profile layout-imports.<profile>.toml for modules without nginx sidecars.
# The active import.conf / layout-imports.toml copies default to the cloud set
# and can be switched with scripts/webserver-import-profile.mjs.
materialize_module_import_files() {
  local imports_root="${CONFIG_ROOT}/imports.d"
  local modules_root="${CONFIG_ROOT}/modules"
  local environment active_profile import_profile
  local module module_root nginx_conf import_conf layout_toml sidecar_path
  local include_count layout_count product_edge
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  active_profile="$(webserver_import_profile)"
  product_edge="${imports_root}/product-edge-nginx.conf"

  ensure_directory "${imports_root}"
  shopt -s nullglob
  rm -f "${imports_root}"/*.conf "${imports_root}"/*.toml 2>/dev/null || true
  find "${imports_root}" -maxdepth 1 -type l -delete 2>/dev/null || true
  shopt -u nullglob

  materialize_product_edge_nginx_conf || true

  for import_profile in standalone cloud; do
    import_conf="${imports_root}/import.conf.${import_profile}"
    layout_toml="${imports_root}/layout-imports.${import_profile}.toml"
    include_count=0
    layout_count=0

    cat > "${import_conf}" <<EOF
# Generated by sdkwork-webserver-entrypoint (SDKWORK_WEBSERVER_SPEC.md §17.3).
# Import set: ${import_profile}. Each line includes one sibling module sidecar
# from the space checkout:
#   /opt/deploy/sdkwork-space/<module>/deployments/webserver/nginx.${import_profile}.${environment}.conf
# Lifecycle environment: ${environment}  Import profile: ${import_profile}
EOF

    # Product edge first so server-dev.* matches AdaptiveAppShell before the
    # sibling default_server (otherwise unmatched hosts hit static-fallback).
    if [ -f "${product_edge}" ]; then
      printf 'include %s;\n' "${product_edge}" >> "${import_conf}"
      include_count=$((include_count + 1))
      log "product edge nginx include -> ${product_edge}"
    fi

    cat > "${layout_toml}" <<EOF
# Generated by sdkwork-webserver-entrypoint for layout v3 module imports (${import_profile}).
EOF

    IFS=',' read -r -a module_list <<< "${SDKWORK_SPACE_IMPORT_MODULES:-}"
    for module in "${module_list[@]}"; do
      module="$(printf '%s' "${module}" | xargs)"
      [ -z "${module}" ] && continue
      module_root="$(module_repo_root "${module}")"
      if ! module_webserver_enabled "${module_root}"; then
        log "module ${module} is disabled or missing layout; skipped"
        continue
      fi
      if module_nginx_import_enabled "${module_root}"; then
        # High-cohesion import: the module's own checkout sidecar is the
        # single source of truth (SDKWORK_WEBSERVER_SPEC.md §17.3). The
        # aggregator includes it directly — no copies under /etc.
        sidecar_path="$(module_nginx_sidecar_abs_path "${module_root}" "${import_profile}")"
        printf 'include %s;\n' "${sidecar_path}" >> "${import_conf}"
        include_count=$((include_count + 1))
        log "module nginx include -> ${sidecar_path}"
        continue
      fi
      if [ ! -f "${modules_root}/${module}/server.common.toml" ]; then
        log "module ${module} has no materialized layout v3 config; skipped"
        continue
      fi
      cat >> "${layout_toml}" <<EOF

[[webserver.imports]]
id = "${module}"
path = "${modules_root}/${module}"
enabled = true
required = false
probe_upstreams = false
EOF
      layout_count=$((layout_count + 1))
      log "module layout import -> ${modules_root}/${module}"
    done

    if [ "${include_count}" -eq 0 ]; then
      rm -f "${import_conf}"
      log "no nginx sidecars discovered; removed ${import_conf}"
    else
      chown "${SERVICE_USER}:${SERVICE_USER}" "${import_conf}" 2>/dev/null || true
      chmod 0644 "${import_conf}" 2>/dev/null || true
      log "wrote ${import_conf} with ${include_count} module sidecar include(s)"
    fi

    if [ "${layout_count}" -eq 0 ]; then
      rm -f "${layout_toml}"
    else
      chown "${SERVICE_USER}:${SERVICE_USER}" "${layout_toml}" 2>/dev/null || true
      chmod 0640 "${layout_toml}" 2>/dev/null || true
      log "wrote ${layout_toml} with ${layout_count} layout v3 import(s)"
    fi
  done

  # Imported web modules serve PC/H5 through their sidecar @pc/@h5 named
  # locations; point those package roots at the checkout dist trees so the
  # data plane actually finds sibling SPA assets (SPEC §13.6 / §17).
  materialize_module_web_static_roots

  activate_import_profile "${active_profile}"
}

# Activate one materialized import set by copying its aggregator files to the
# active import.conf / layout-imports.toml the runtime config loads.
activate_import_profile() {
  local profile="$1"
  local imports_root="${CONFIG_ROOT}/imports.d"
  case "${profile}" in
    standalone|cloud) ;;
    *)
      log "unsupported import profile ${profile}; keeping current activation"
      return 1
      ;;
  esac
  if [ -f "${imports_root}/import.conf.${profile}" ]; then
    cp -f "${imports_root}/import.conf.${profile}" "${imports_root}/import.conf"
    chown "${SERVICE_USER}:${SERVICE_USER}" "${imports_root}/import.conf" 2>/dev/null || true
    chmod 0644 "${imports_root}/import.conf" 2>/dev/null || true
  else
    rm -f "${imports_root}/import.conf"
  fi
  if [ -f "${imports_root}/layout-imports.${profile}.toml" ]; then
    cp -f "${imports_root}/layout-imports.${profile}.toml" "${imports_root}/layout-imports.toml"
    chown "${SERVICE_USER}:${SERVICE_USER}" "${imports_root}/layout-imports.toml" 2>/dev/null || true
    chmod 0640 "${imports_root}/layout-imports.toml" 2>/dev/null || true
  else
    rm -f "${imports_root}/layout-imports.toml"
  fi
  log "activated import profile ${profile} under ${imports_root}"
}

module_app_static_root() {
  local module="$1"
  local surface="$2"
  local environment dist_alias
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  dist_alias="$(environment_dist_alias "${environment}")"
  module_app_static_root_for_alias "${module}" "${surface}" "${dist_alias}" "$(static_source_profile)"
}

# Extract the Adaptive Web named-location root a sidecar declares for @pc/@h5
# (SDKWORK_DEPLOY_SPEC.md §8.1 emission contract), e.g.
# /usr/share/sdkwork/im/web/pc. Returns non-zero when absent.
module_adaptive_named_root() {
  local sidecar="$1"
  local surface="$2"
  [ -f "${sidecar}" ] || return 1
  local root
  root="$(sed -n "/^[[:space:]]*location @${surface}[[:space:]]*{/,/^[[:space:]]*}/p" "${sidecar}" \
    | sed -n 's/^[[:space:]]*root[[:space:]]\+\([^;[:space:]]*\);.*/\1/p' \
    | head -1)"
  [ -n "${root}" ] || return 1
  printf '%s' "${root}"
}

# Materialize the container-side Adaptive Web roots every imported module's
# named locations dispatch to (usually /usr/share/sdkwork/<code>/web/{pc,h5}).
# Host-based vhosting and UA split resolve in the data plane, but nothing in
# the image owns those package paths — without this step sibling PC/H5 SPA
# surfaces 404 (SDKWORK_WEBSERVER_SPEC.md §13.6 / §17). Each root is symlinked
# to the active static-source profile dist tree; existing non-symlink content
# is never overwritten. Idempotent; safe to re-run on reload.
materialize_module_web_static_roots() {
  local adaptive_base="${SDKWORK_WEBSERVER_MODULE_WEB_ROOT:-/usr/share/sdkwork}"
  local environment dist_alias source_profile
  local modules_list module module_root conf profile surface
  local root src parent seen_roots
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  dist_alias="$(environment_dist_alias "${environment}")"
  source_profile="$(static_source_profile)"
  ensure_directory "${adaptive_base}"
  seen_roots=""
  IFS=',' read -r -a modules_list <<< "${SDKWORK_SPACE_IMPORT_MODULES:-}"
  for module in "${modules_list[@]}"; do
    module="$(printf '%s' "${module}" | xargs)"
    [ -z "${module}" ] && continue
    module_root="$(module_repo_root "${module}")"
    if ! module_webserver_enabled "${module_root}"; then
      continue
    fi
    # Both profile sets coexist (§17.3); Adaptive Web roots are identical
    # across them, so process whichever confs exist exactly once per path.
    for profile in standalone cloud; do
      conf="$(module_nginx_sidecar_abs_path "${module_root}" "${profile}")"
      [ -f "${conf}" ] || continue
      for surface in pc h5; do
        if ! root="$(module_adaptive_named_root "${conf}" "${surface}")"; then
          continue
        fi
        case "${root}" in
          "${adaptive_base}"/*) ;;
          *) log "warning: ${module} ${profile} ${surface} root outside ${adaptive_base}: ${root}; skipped" ; continue ;;
        esac
        case ",${seen_roots}," in
          *,"${root}",*) continue ;;
        esac
        seen_roots="${seen_roots:+${seen_roots},}${root}"
        src="$(module_app_static_root_for_alias "${module}" "${surface}" "${dist_alias}" "${source_profile}")"
        if [ -z "${src}" ]; then
          log "warning: ${module} ${surface}: no apps/*-${surface}/dist/${source_profile}/${dist_alias} build under ${module_root}; ${root} stays unserved (build with: pnpm --dir ${module} build:${surface}:${dist_alias}:${source_profile})"
          continue
        fi
        if [ -L "${root}" ]; then
          if [ "$(readlink -f "${root}")" = "$(readlink -f "${src}")" ]; then
            continue
          fi
        elif [ -e "${root}" ]; then
          log "warning: refusing to replace non-symlink content at ${root} (${module} ${surface}); remove it manually to re-point"
          continue
        fi
        parent="$(dirname "${root}")"
        mkdir -p "${parent}"
        chown "${SERVICE_USER}:${SERVICE_USER}" "${parent}" 2>/dev/null || true
        ln -sfn "${src}" "${root}"
        log "linked ${root} -> ${src#${module_root}/} (${module} ${surface}, ${source_profile}/${dist_alias})"
      done
    done
  done
}

# Seed a readable SPA shell when module static/ (or Docker copy) has no
# index.html. Operators replace this by building apps/*-{pc,h5}/dist/<alias>.
app_roots_by_environment_toml() {
  local pc_root="${SDKWORK_WEBSERVER_PC_STATIC_ROOT:-/app/share/sdkwork/webserver/web/pc}"
  local h5_root="${SDKWORK_WEBSERVER_H5_STATIC_ROOT:-/app/share/sdkwork/webserver/web/h5}"
  local static_root="${SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT:-/app/share/sdkwork/webserver/web/static}"
  cat <<EOF
tablet_surface = "pc"
pc_static_root = "${pc_root}"
h5_static_root = "${h5_root}"
static_fallback_root = "${static_root}"
EOF
}

render_runtime_config() {
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  local bind="${SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND:-0.0.0.0:3800}"
  local module
  # Keep bundled webserver PC/H5 roots as the management console. Sibling
  # Adaptive Web assets are wired per-module via module-app-roots + nginx
  # named locations (SDKWORK_WEBSERVER_SPEC.md §13.6 / §17).
  local APP_ROOTS_ENV_TOML
  APP_ROOTS_ENV_TOML="$(app_roots_by_environment_toml)"
  local public_url="${SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL}"
  local data_plane_bind="${SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND:-127.0.0.1:3901}"
  local ingress_port="${bind##*:}"
  local internal_api_url="http://127.0.0.1:${ingress_port}"

  # [database] config only supports password_file (not password).
  # If compose provided SDKWORK_DATABASE_PASSWORD, ensure_database_secret()
  # will write it to /etc/sdkwork/webserver/secrets/database.secret, so we
  # always point TOML at password_file.
  local db_password_field='password_file = "/etc/sdkwork/webserver/secrets/database.secret"'
  if [ -n "${SDKWORK_DATABASE_PASSWORD_FILE:-}" ]; then
    db_password_field="password_file = \"${SDKWORK_DATABASE_PASSWORD_FILE}\""
  fi

  # Build cors_allowed_origins TOML array from comma-separated env var.
  # "http://a.com, http://b.com" -> ["http://a.com", "http://b.com"]
  local cors_origins="${SDKWORK_CORS_ALLOWED_ORIGINS:-${public_url}}"
  local cors_toml_array
  cors_toml_array=$(printf '%s' "${cors_origins}" | tr ',' '\n' \
    | sed 's/^[[:space:]]*//' | sed 's/[[:space:]]*$//' \
    | awk 'NF{printf "\"%s\", ", $0}' | sed 's/, $//')
  cors_toml_array="[${cors_toml_array}]"

  # NOTE: The RuntimeTomlConfig struct uses #[serde(deny_unknown_fields)].
  # Supported top-level sections: profile, ingress, app_roots, deploy, database,
  # secrets, acme, tls, node, region, webserver (imports). Redis is NOT a TOML
  # section — it is configured exclusively via SDKWORK_WEBSERVER_REDIS_* environment
  # variables injected directly by Docker Compose.

  local webserver_includes=""
  if [ -f "${CONFIG_ROOT}/imports.d/import.conf" ]; then
    webserver_includes='"imports.d/import.conf"'
  fi
  if [ -f "${CONFIG_ROOT}/imports.d/layout-imports.toml" ]; then
    if [ -n "${webserver_includes}" ]; then
      webserver_includes+=', "imports.d/layout-imports.toml"'
    else
      webserver_includes='"imports.d/layout-imports.toml"'
    fi
  fi
  if [ -z "${webserver_includes}" ]; then
    webserver_includes=""
  else
    webserver_includes="[${webserver_includes}]"
  fi

  ensure_directory "${CONFIG_ROOT}"
  cat > "${RUNTIME_CONFIG_FILE}" <<EOF
# Generated by sdkwork-webserver-entrypoint for container runtime.
[profile]
deployment_profile = "standalone"
environment = "${environment}"
profile_id = "standalone.${environment}"
node_id = 0

[ingress]
bind = "${bind}"
management_expose_allowed = true
data_plane_operations_bind = "${data_plane_bind}"
public_http_url = "${public_url}"
app_http_url = "${public_url}"
backend_http_url = "${public_url}"
cors_allowed_origins = ${cors_toml_array}

[app_roots]
app_root = "/app"
iam_app_root = "/app/share/sdkwork/iam"
drive_app_root = "/app/share/sdkwork/drive"
deploy_app_root = "/app/share/sdkwork/deploy"
web_store_app_root = "/app/share/sdkwork/webstore"
${APP_ROOTS_ENV_TOML}

[deploy]
deployment_profile = "standalone"
environment = "${environment}"
profile_id = "standalone.${environment}"
use_memory_drive = false
use_memory_content_provider = false
drive_facade_url = "${internal_api_url}"
drive_internal_api_url = "${internal_api_url}"
drive_internal_api_ingress_token_file = "${SECRETS_ROOT}/drive-internal-api-ingress-token"
knowledgebase_internal_api_url = "${internal_api_url}"
knowledgebase_internal_api_ingress_token_file = "${SECRETS_ROOT}/knowledgebase-internal-api-ingress-token"
web_internal_api_url = "${internal_api_url}"
web_internal_api_ingress_token_file = "${SECRETS_ROOT}/web-internal-api-ingress-token"
runtime_assignment_worker_id = "deploy-worker-0"

[database]
engine = "${SDKWORK_DATABASE_ENGINE:-postgresql}"
host = "${SDKWORK_DATABASE_HOST:-127.0.0.1}"
port = ${SDKWORK_DATABASE_PORT:-5432}
name = "${SDKWORK_DATABASE_NAME:-sdkwork_ai_dev}"
schema = "${SDKWORK_DATABASE_SCHEMA:-${SDKWORK_DATABASE_NAME:-sdkwork_ai_dev}}"
schema_fallback_public = ${SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC:-false}
username = "${SDKWORK_DATABASE_USERNAME:-sdkwork_ai_dev}"
${db_password_field}
ssl_mode = "${SDKWORK_DATABASE_SSL_MODE:-disable}"
max_connections = ${SDKWORK_DATABASE_MAX_CONNECTIONS:-10}
auto_migrate = true

[secrets]
encryption_key_file = "${SECRETS_ROOT}/encryption-key"
deploy_encryption_key_file = "${SECRETS_ROOT}/deploy-encryption-key"
credential_entry_bootstrap_access_token_file = "${SECRETS_ROOT}/credential-entry-bootstrap-access-token"

[acme]
profile = "${SDKWORK_WEBSERVER_ACME_PROFILE:-staging}"
directory_url = "${SDKWORK_WEBSERVER_ACME_DIRECTORY_URL:-https://acme-staging-v02.api.letsencrypt.org/directory}"
contact_email = "${SDKWORK_WEBSERVER_ACME_CONTACT_EMAIL:-admin@localhost}"
webroot = "${SDKWORK_WEBSERVER_ACME_WEBROOT:-/var/lib/sdkwork/webserver/acme-webroot}"
account_root = "${SDKWORK_WEBSERVER_ACME_ACCOUNT_ROOT:-/var/lib/sdkwork/webserver/acme-accounts}"
renew_before_days = ${SDKWORK_WEBSERVER_CERT_RENEW_BEFORE_DAYS:-30}
worker_id = "${SDKWORK_WEBSERVER_CERT_WORKER_ID:-certificate-worker-0}"
operation_poll_interval_secs = ${SDKWORK_WEBSERVER_CERT_OPERATION_POLL_INTERVAL_SECS:-5}
renew_scan_interval_secs = ${SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS:-3600}

[tls]
material_root = "${SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT:-/var/lib/sdkwork/webserver/tls-materials}"
runtime_snapshot_file = "${SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE:-/var/lib/sdkwork/webserver/tls-materials/tls-runtime.json}"
snapshot_alpn = "${SDKWORK_WEBSERVER_TLS_SNAPSHOT_ALPN:-h2,http/1.1}"

[node]
# Multi-instance safe default (DEPLOYMENT_SPEC.md §6): the container hostname
# is unique per instance, so scaled instances never share one node identity.
# Operators override with SDKWORK_WEBSERVER_NODE_UUID (bundle deploy.sh sets
# standalone-<environment>-i<index>).
uuid = "${SDKWORK_WEBSERVER_NODE_UUID:-standalone-${environment}-${HOSTNAME:-node}}"

[region]
region_code = "${SDKWORK_WEBSERVER_REGION_CODE:-cn}"
seed_locale = "${SDKWORK_DATABASE_SEED_LOCALE:-zh-CN}"

# Sibling-module imports: imports.d/import.conf aggregates checkout nginx
# sidecars via include; layout-imports.toml lists layout v3 directories.
[webserver]
include = ${webserver_includes:-[]}
EOF
  chown root:"${SERVICE_USER}" "${RUNTIME_CONFIG_FILE}"
  chmod 0640 "${RUNTIME_CONFIG_FILE}"
}

ensure_spa_static_root() {
  local env_name="$1"
  local static_root="$2"
  local bundled_app="$3"
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  # Never invent a missing SPA shell (SDKWORK_DEPLOY_SPEC.md §8).

  ensure_directory "${static_root}"

  if [ -f "${bundled_app}/index.html" ] && [ "${static_root}" != "${bundled_app}" ]; then
    log "copying bundled app from ${bundled_app} to ${static_root} (${env_name})"
    cp -r "${bundled_app}/." "${static_root}/"
    chown -R "${SERVICE_USER}:${SERVICE_USER}" "${static_root}"
  fi

  # Bundled SPA may ship a production runtime-env.json; rewrite to match the
  # active container environment so standalone static delivery validation passes.
  # messagingPcUrl is required by Adaptive Web PC/H5 (cross-app Messaging PC
  # notification center). Match apps/*/etc/browser/runtime-env.*.json.
  if [ -f "${static_root}/index.html" ]; then
    local messaging_pc_url
    case "${environment}" in
      development|test)
        messaging_pc_url="${SDKWORK_WEBSERVER_MESSAGING_PC_URL:-http://127.0.0.1:5184/notifications}"
        ;;
      staging)
        messaging_pc_url="${SDKWORK_WEBSERVER_MESSAGING_PC_URL:-https://messaging-staging.sdkwork.com/notifications}"
        ;;
      production)
        messaging_pc_url="${SDKWORK_WEBSERVER_MESSAGING_PC_URL:-https://messaging.sdkwork.com/notifications}"
        ;;
      *)
        messaging_pc_url="${SDKWORK_WEBSERVER_MESSAGING_PC_URL:-http://127.0.0.1:5184/notifications}"
        ;;
    esac
    cat > "${static_root}/runtime-env.json" <<EOF
{
  "environment": "${environment}",
  "deploymentProfile": "standalone",
  "profileId": "standalone.${environment}",
  "runtimeTarget": "browser",
  "browserOriginMode": "same-origin",
  "defaultLocale": "zh-CN",
  "fallbackLocale": "en-US",
  "supportedLocales": ["zh-CN", "en-US"],
  "activeLocales": ["zh-CN", "en-US"],
  "appApiBaseUrl": "/",
  "backendApiBaseUrl": "/",
  "driveAppApiBaseUrl": "/",
  "appbaseAppApiBaseUrl": "/",
  "messagingPcUrl": "${messaging_pc_url}",
  "deployAppApiBaseUrl": "/"
}
EOF
    chmod 0644 "${static_root}/index.html" "${static_root}/runtime-env.json"
  fi

  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${static_root}"
  chmod 0755 "${static_root}"
}

ensure_adaptive_web_roots() {
  local pc_root="${SDKWORK_WEBSERVER_PC_STATIC_ROOT:-/app/share/sdkwork/webserver/web/pc}"
  local h5_root="${SDKWORK_WEBSERVER_H5_STATIC_ROOT:-/app/share/sdkwork/webserver/web/h5}"
  local static_root="${SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT:-/app/share/sdkwork/webserver/web/static}"

  ensure_spa_static_root SDKWORK_WEBSERVER_PC_STATIC_ROOT "${pc_root}" /app/share/sdkwork/webserver-pc
  ensure_spa_static_root SDKWORK_WEBSERVER_H5_STATIC_ROOT "${h5_root}" /app/share/sdkwork/webserver-h5

  ensure_directory "${static_root}"
  if [ -d /app/share/sdkwork/webserver-static ] && [ "${static_root}" != /app/share/sdkwork/webserver-static ]; then
    cp -r /app/share/sdkwork/webserver-static/. "${static_root}/" 2>/dev/null || true
  fi
  if [ ! -f "${static_root}/index.html" ] && [ -f /app/share/sdkwork/webserver/web/static/index.html ]; then
    cp /app/share/sdkwork/webserver/web/static/index.html "${static_root}/index.html" 2>/dev/null || true
  fi
  if [ ! -f "${static_root}/index.html" ] && [ -f /app/deployments/webserver/static/index.html ]; then
    cp /app/deployments/webserver/static/index.html "${static_root}/index.html" 2>/dev/null || true
  fi
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${static_root}"
  chmod 0755 "${static_root}"
}

run_module_browser_build() {
  local module="$1"
  local architecture="$2"
  local environment="${3:-development}"
  local deployment_profile="${4:-standalone}"
  local module_root build_tool
  module_root="$(module_repo_root "${module}")"
  if [ ! -d "${module_root}" ]; then
    log "module checkout missing: ${module_root}"
    return 1
  fi
  build_tool="${SDKWORK_BROWSER_BUILD_TOOL:-/app/scripts/docker/build-module-browser.mjs}"
  if [ ! -f "${build_tool}" ]; then
    build_tool="$(space_checkout_dir)/sdkwork-webserver/scripts/docker/build-module-browser.mjs"
  fi
  if [ ! -f "${build_tool}" ]; then
    log "browser build tool missing; set SDKWORK_BROWSER_BUILD_TOOL"
    return 1
  fi
  log "building ${module} ${architecture} ${environment} (${deployment_profile})"
  SDKWORK_SPACE_ROOT="$(dirname "$(space_checkout_dir)")" \
    run_as_service_user node "${build_tool}" \
      --module "${module}" \
      --architecture "${architecture}" \
      --environment "${environment}" \
      --deployment-profile "${deployment_profile}"
}

module_has_browser_surface() {
  local module="$1"
  local architecture="$2"
  local apps_root app match
  if [ "${architecture}" = "pc" ]; then
    match="*pc*"
  else
    match="*h5*"
  fi
  apps_root="$(module_repo_root "${module}")/apps"
  [ -d "${apps_root}" ] || return 1
  for app in "${apps_root}"/${match}; do
    [ -d "${app}" ] || continue
    if [ -f "${app}/vite.config.ts" ] \
      || [ -f "${app}/vite.config.mjs" ] \
      || [ -f "${app}/vite.config.web.mjs" ] \
      || [ -f "${app}/vite.config.web.ts" ] \
      || [ -f "${app}/vite.config.browser.ts" ] \
      || [ -f "${app}/vite.config.browser.mjs" ]; then
      return 0
    fi
  done
  return 1
}

run_module_browser_build_all() {
  local module="$1"
  local environment="${2:-development}"
  local deployment_profile="${3:-standalone}"
  local built=0
  for architecture in pc h5; do
    if module_has_browser_surface "${module}" "${architecture}"; then
      run_module_browser_build "${module}" "${architecture}" "${environment}" "${deployment_profile}" || return 1
      built=1
    fi
  done
  if [ "${built}" -eq 0 ]; then
    log "module ${module} has no pc/h5 browser surfaces to build"
    return 1
  fi
  return 0
}

reload_module_static_catalog() {
  clone_sdkwork_space_modules
  materialize_module_webserver_configs
  materialize_module_import_files
  log "refreshed module static catalogs under ${CONFIG_ROOT}/module-app-roots"
}

run_as_service_user() {
  if [ "$(id -u)" -eq 0 ]; then
    runuser -u "${SERVICE_USER}" -- "$@"
    return $?
  fi
  "$@"
}

exec_as_service_user() {
  if [ "$(id -u)" -eq 0 ]; then
    exec runuser -u "${SERVICE_USER}" -- "$@"
  fi
  exec "$@"
}

resolve_gateway_binary() {
  if [ -x "${GATEWAY_BINARY}" ]; then
    return 0
  fi
  return 1
}

main() {
  if ! resolve_gateway_binary; then
    log "gateway binary is missing at ${GATEWAY_BINARY}"
    exit 1
  fi

  apply_primary_domain
  clone_sdkwork_space_modules
  materialize_module_webserver_configs
  materialize_module_import_files
  ensure_imported_sidecar_certificates || true
  for cert_domain in ${SDKWORK_WEBSERVER_CERT_DOMAINS:-sdkwork.com app.sdkwork.com}; do
    ensure_domain_certificate "${cert_domain}"
  done
  ensure_database_secret
  ensure_drive_delivery_cache_root
  for secret_name in encryption-key deploy-encryption-key \
    drive-internal-api-ingress-token knowledgebase-internal-api-ingress-token \
    web-internal-api-ingress-token; do
    ensure_secret_file "${secret_name}"
  done

  render_runtime_config
  ensure_credential_entry_bootstrap_token
  ensure_adaptive_web_roots

  case "${1:-serve-management}" in
    build-browser)
      shift
      module=""
      architecture="all"
      environment="dev"
      deployment_profile="standalone"
      reload_catalog="false"
      while [ $# -gt 0 ]; do
        case "$1" in
          --module) module="$2"; shift 2 ;;
          --architecture) architecture="$2"; shift 2 ;;
          --environment) environment="$2"; shift 2 ;;
          --deployment-profile) deployment_profile="$2"; shift 2 ;;
          --reload-static) reload_catalog="true"; shift ;;
          *) log "unknown build-browser option: $1"; exit 2 ;;
        esac
      done
      if [ -z "${module}" ]; then
        log "usage: build-browser --module <sdkwork-module> [--architecture pc|h5|all] [--environment dev|test|staging|prod] [--deployment-profile standalone|cloud] [--reload-static]"
        exit 2
      fi
      clone_sdkwork_space_modules
      if [ "${architecture}" = "all" ]; then
        run_module_browser_build_all "${module}" "${environment}" "${deployment_profile}" || exit 1
      else
        run_module_browser_build "${module}" "${architecture}" "${environment}" "${deployment_profile}" || exit 1
      fi
      if [ "${reload_catalog}" = "true" ]; then
        reload_module_static_catalog
      fi
      ;;
    reload-module-static)
      shift
      reload_module_static_catalog
      ;;
    serve-management)
      log "running database migration"
      run_as_service_user "${GATEWAY_BINARY}" db-migrate
      module_imports_present=false
      if [ -f "${CONFIG_ROOT}/imports.d/import.conf" ] \
        || [ -f "${CONFIG_ROOT}/imports.d/layout-imports.toml" ]; then
        module_imports_present=true
      fi
      if [ "${module_imports_present}" = true ]; then
        log "module imports detected; preparing module API gateway (deployment=$(module_api_gateway_deployment))"
        prepare_module_api_gateway
        log "management in background, module-imports data plane in foreground"
        run_as_service_user "${GATEWAY_BINARY}" serve-management \
          > "${MANAGEMENT_LOG_FILE:-/var/lib/sdkwork/webserver/management.log}" 2>&1 &
        MANAGEMENT_PID=$!
        trap 'kill "${MANAGEMENT_PID}" 2>/dev/null || true; kill "${PLATFORM_GATEWAY_PID:-}" 2>/dev/null || true; kill "${PLATFORM_KB_RPC_PID:-}" 2>/dev/null || true' EXIT
        exec_as_service_user "${GATEWAY_BINARY}" serve-imports
      fi
      log "starting management listener on ${SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND:-127.0.0.1:3800}"
      exec_as_service_user "${GATEWAY_BINARY}" serve-management
      ;;
    db-migrate)
      exec_as_service_user "${GATEWAY_BINARY}" db-migrate
      ;;
    *)
      exec_as_service_user "${GATEWAY_BINARY}" "$@"
      ;;
  esac
}

if [ "${SDKWORK_ENTRYPOINT_SKIP_MAIN:-}" != "1" ]; then
  main "$@"
fi
