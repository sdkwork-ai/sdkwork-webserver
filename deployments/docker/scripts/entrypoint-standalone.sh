#!/usr/bin/env bash
# Container entrypoint for the standalone gateway image.
set -euo pipefail

CONFIG_ROOT="/etc/sdkwork/webserver"
SECRETS_ROOT="${CONFIG_ROOT}/secrets"
RUNTIME_CONFIG_FILE="${CONFIG_ROOT}/config.toml"
GATEWAY_BINARY="/app/bin/sdkwork-api-webserver-standalone-gateway"
SERVICE_USER="sdkwork"

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
# replaced by real material without a restart.
ensure_domain_certificate() {
  local domain="$1"
  local directory="${SDKWORK_CERTS_DIR:-/etc/sdkwork/certs}/${domain}"
  local cert="${directory}/cert.pem"
  local key="${directory}/key.pem"
  if [ -s "${cert}" ] && [ -s "${key}" ]; then
    log "certificate present: ${domain}"
    return 0
  fi
  ensure_directory "${directory}"
  log "generating self-signed bootstrap certificate for ${domain} (replace under ${directory})"
  openssl req -x509 -newkey rsa:2048 -nodes     -days 3650     -keyout "${key}" -out "${cert}"     -subj "/CN=${domain}"     -addext "subjectAltName=DNS:${domain}" 2>/dev/null
  chmod 0600 "${key}"
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${directory}"
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
ensure_credential_entry_bootstrap_token() {
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  local file="${SECRETS_ROOT}/credential-entry-bootstrap-access-token"
  ensure_directory "${SECRETS_ROOT}"
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

apply_primary_domain() {
  local domain="${SDKWORK_WEBSERVER_PRIMARY_DOMAIN:-sdkwork.com}"
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
  # When WSL nginx :80 is unavailable, browsers use host-published ports; keep
  # portless origins too for nginx-backed operator routing.
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
  # SDKWORK_SPACE_MODULES remains the explicit override.
  auto_discover="${SDKWORK_SPACE_AUTO_DISCOVER:-true}"
  checkout="$(space_checkout_dir)"
  if [ "${auto_discover}" = "true" ] || [ "${auto_discover}" = "1" ] || [ "${auto_discover}" = "yes" ]; then
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
  fi
  if [ -n "${SDKWORK_SPACE_MODULES:-}" ]; then
    printf '%s' "${SDKWORK_SPACE_MODULES}"
  else
    printf '%s' "${discovered}"
  fi
}

clone_sdkwork_space_modules() {
  local space_root="${SDKWORK_SPACE_ROOT:-/opt/deploy}"
  local checkout clone_url base module module_dir local_path
  export SDKWORK_SPACE_ROOT="${space_root}"
  if [ ! -d "${space_root}" ]; then
    ensure_writable_directory "${space_root}"
  fi

  checkout="$(space_checkout_dir)"
  local_path="${SDKWORK_SPACE_LOCAL_PATH:-}"
  if [ -n "${local_path}" ] && [ -d "${local_path}" ] && [ ! -e "${checkout}" ]; then
    ln -sfn "${local_path}" "${checkout}"
    log "linked space checkout ${checkout} -> ${local_path}"
  fi

  clone_url="${SDKWORK_SPACE_CLONE_URL:-https://github.com/sdkwork-ai/sdkwork-space.git}"
  if [ -d "${checkout}" ] && [ -n "$(ls -A "${checkout}" 2>/dev/null || true)" ]; then
    log "using existing space checkout at ${checkout}"
  elif [ -n "${clone_url}" ] && [ ! -d "${checkout}" ]; then
    clone_or_update_git_repo "${clone_url}" "${checkout}" || true
  elif [ -d "${checkout}/.git" ] && [ -n "${clone_url}" ]; then
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

  export SDKWORK_SPACE_IMPORT_MODULES="$(discover_importable_modules)"
  if touch "${space_root}/.sdkwork-write-test" 2>/dev/null; then
    rm -f "${space_root}/.sdkwork-write-test"
    chown -R "${SERVICE_USER}:${SERVICE_USER}" "${space_root}" 2>/dev/null || true
  else
    log "space root ${space_root} is read-only; using host bind mount as-is"
  fi
}

materialize_module_webserver_configs() {
  local modules_root="${CONFIG_ROOT}/modules"
  local catalog_root="${CONFIG_ROOT}/module-app-roots"
  local module module_root module_ws link_target dest pc_root h5_root dist_alias gateway_port
  dist_alias="$(environment_dist_alias "${SDKWORK_WEBSERVER_ENVIRONMENT:-development}")"
  gateway_port="${SDKWORK_WEBSERVER_CONTAINER_HEALTH_PORT:-${SDKWORK_WEBSERVER_APPLICATION_PUBLIC_INGRESS_BIND##*:}}"
  gateway_port="${gateway_port:-3800}"
  ensure_directory "${modules_root}"
  ensure_directory "${catalog_root}"
  IFS=',' read -r -a module_list <<< "${SDKWORK_SPACE_IMPORT_MODULES:-}"
  for module in "${module_list[@]}"; do
    module="$(printf '%s' "${module}" | xargs)"
    [ -z "${module}" ] && continue
    module_root="$(module_repo_root "${module}")"
    module_ws="${module_root}/deployments/webserver"
    if ! module_webserver_enabled "${module_root}"; then
      continue
    fi
    dest="${modules_root}/${module}"
    rm -rf "${dest}"
    mkdir -p "${dest}"
    # Materialize the complete layout v3 directory (SDKWORK_WEBSERVER_SPEC.md
    # §17): common + one file per lifecycle environment + both profile files,
    # with the standalone upstream patched to the container gateway port. The
    # snapshot is itself a loadable layout v3 directory for operators.
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
      sed "s/127.0.0.1:3800/127.0.0.1:${gateway_port}/g" \
        "${module_ws}/server.standalone.toml" > "${dest}/server.standalone.toml"
    fi
    log "materialized module webserver config -> ${dest} (layout v3, gateway upstream 127.0.0.1:${gateway_port})"

    pc_root="$(module_app_static_root "${module}" pc)"
    h5_root="$(module_app_static_root "${module}" h5)"
    cat > "${catalog_root}/${module}.toml" <<EOF
# Generated Adaptive Web catalog for imported module ${module}.
# Authority: SDKWORK_WEBSERVER_SPEC.md §13.6 / §17.
# Dist alias: ${dist_alias} (lifecycle environment: ${SDKWORK_WEBSERVER_ENVIRONMENT:-development})

[app_roots]
tablet_surface = "pc"
pc_static_root = "${pc_root:-}"
h5_static_root = "${h5_root:-}"
static_fallback_root = "${module_root}/deployments/webserver/static"
EOF
    chown "${SERVICE_USER}:${SERVICE_USER}" "${catalog_root}/${module}.toml" 2>/dev/null || true
    chmod 0640 "${catalog_root}/${module}.toml" 2>/dev/null || true
    log "materialized module app-roots catalog -> ${catalog_root}/${module}.toml"
  done
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${modules_root}" 2>/dev/null || true
  chown -R "${SERVICE_USER}:${SERVICE_USER}" "${catalog_root}" 2>/dev/null || true
}

# nginx-style module imports: one `imports.d/<module-id>.conf` symlink per
# sibling module pointing at the rendered nginx sidecar for the active profile
# and environment (SDKWORK_WEBSERVER_SPEC.md §17). Runtime `[webserver] include`
# loads these `.conf` files; drop or remove a symlink to add/remove a module.
materialize_module_import_files() {
  local imports_root="${CONFIG_ROOT}/imports.d"
  local module module_root nginx_conf
  ensure_directory "${imports_root}"
  rm -f "${imports_root}"/*.conf "${imports_root}"/*.toml 2>/dev/null || true
  IFS=',' read -r -a module_list <<< "${SDKWORK_SPACE_IMPORT_MODULES:-}"
  for module in "${module_list[@]}"; do
    module="$(printf '%s' "${module}" | xargs)"
    [ -z "${module}" ] && continue
    module_root="$(module_repo_root "${module}")"
    if ! module_webserver_enabled "${module_root}"; then
      log "module ${module} is disabled or missing layout; skipped"
      continue
    fi
    nginx_conf="$(module_nginx_conf_path "${module_root}")" || {
      log "module ${module} has no nginx sidecar for active profile/environment; skipped"
      continue
    }
    ln -sfn "${nginx_conf}" "${imports_root}/${module}.conf"
    chown -h "${SERVICE_USER}:${SERVICE_USER}" "${imports_root}/${module}.conf" 2>/dev/null || true
    log "module import conf -> ${imports_root}/${module}.conf (${nginx_conf})"
  done
}

module_nginx_conf_path() {
  local module_root="$1"
  local environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  local profile="${SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE:-${SDKWORK_DEPLOYMENT_PROFILE:-standalone}}"
  local module_ws="${module_root}/deployments/webserver"
  local conf="${module_ws}/nginx.${profile}.${environment}.conf"
  if [ -f "${conf}" ]; then
    printf '%s' "${conf}"
    return 0
  fi
  return 1
}

module_app_static_root() {
  local module="$1"
  local surface="$2"
  local environment dist_alias apps_root app dist match
  environment="${SDKWORK_WEBSERVER_ENVIRONMENT:-development}"
  dist_alias="$(environment_dist_alias "${environment}")"
  if [ "${surface}" = "pc" ]; then
    match="*pc*"
  else
    match="*h5*"
  fi
  apps_root="$(module_repo_root "${module}")/apps"
  [ -d "${apps_root}" ] || return 0
  for app in "${apps_root}"/${match}; do
    [ -d "${app}" ] || continue
    dist="${app}/dist/${dist_alias}"
    if [ -f "${dist}/index.html" ]; then
      printf '%s' "${dist}"
      return 0
    fi
    dist="${app}/dist"
    if [ -f "${dist}/index.html" ]; then
      printf '%s' "${dist}"
      return 0
    fi
  done
  return 0
}

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
  IFS=',' read -r -a module_list <<< "${SDKWORK_SPACE_IMPORT_MODULES:-}"
  for module in "${module_list[@]}"; do
    module="$(printf '%s' "${module}" | xargs)"
    [ -z "${module}" ] && continue
    # Point PC/H5 static roots at cloned module apps when the operator did
    # not pin them explicitly.
    if [ -z "${SDKWORK_WEBSERVER_PC_STATIC_ROOT:-}" ]; then
      local pc_root
      pc_root="$(module_app_static_root "${module}" pc)"
      if [ -n "${pc_root}" ]; then
        export SDKWORK_WEBSERVER_PC_STATIC_ROOT="${pc_root}"
        log "PC static root -> ${pc_root}"
      fi
    fi
    if [ -z "${SDKWORK_WEBSERVER_H5_STATIC_ROOT:-}" ]; then
      local h5_root
      h5_root="$(module_app_static_root "${module}" h5)"
      if [ -n "${h5_root}" ]; then
        export SDKWORK_WEBSERVER_H5_STATIC_ROOT="${h5_root}"
        log "H5 static root -> ${h5_root}"
      fi
    fi
  done
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
uuid = "${SDKWORK_WEBSERVER_NODE_UUID:-standalone-${environment}-node}"

[region]
region_code = "${SDKWORK_WEBSERVER_REGION_CODE:-cn}"
seed_locale = "${SDKWORK_DATABASE_SEED_LOCALE:-zh-CN}"

# Imported sibling-module nginx sidecars (default) and optional TOML import
# descriptors under imports.d/. Materialized layout v3 copies:
# /etc/sdkwork/webserver/modules/<module-id>/
[webserver]
include = ["imports.d/*.conf", "imports.d/*.toml"]
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
  for cert_domain in ${SDKWORK_WEBSERVER_CERT_DOMAINS:-sdkwork.com app.sdkwork.com}; do
    ensure_domain_certificate "${cert_domain}"
  done
  ensure_database_secret
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
      if [ -n "$(ls "${CONFIG_ROOT}/imports.d/"*.toml 2>/dev/null || true)" ]; then
        log "module imports detected; management in background, module-imports data plane in foreground"
        run_as_service_user "${GATEWAY_BINARY}" serve-management \
          > "${MANAGEMENT_LOG_FILE:-/var/lib/sdkwork/webserver/management.log}" 2>&1 &
        MANAGEMENT_PID=$!
        trap 'kill "${MANAGEMENT_PID}" 2>/dev/null || true' EXIT
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

main "$@"
