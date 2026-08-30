#!/usr/bin/env bash
# ============================================================================
# deploy.sh — generic deployer for the sdkwork-webserver unified install bundle
#
# One image, any environment, N instances per environment
# (DEPLOYMENT_SPEC.md §6 / PNPM_SCRIPT_SPEC.md §4.4 / docs/guides/operator/docker-install.md).
#
# Layout (auto-detected):
#   bundle:  ./compose/docker-compose.bundle.yml + ./env/  + ./image.tar.gz
#   repo:    ../docker-compose.bundle.yml      + ../env/
#
# Usage:
#   deploy.sh --environment <development|test|production> [options]
#     --replicas <N>     instances to run (default 1; every env supports N)
#     --external         skip embedded postgres/redis (use env-file hosts)
#     --image-tag <tag>  override SDKWORK_WEBSERVER_IMAGE_TAG
#     --down             stop instances + embedded deps
#     --purge            with --down: also delete volumes and network
#     --ps               show instance status
#     --logs [N]         follow instance N logs (default 1)
#     --dry-run          print the resolved commands only
#
# Multiple independently configurable webservers: create
# env/<environment>.i<index>.env to layer instance-specific values (primary
# domain, clone URL, TLS/ACME profile, ...) on top of the base env file
# (later --env-file wins in compose).
#
# Examples:
#   deploy.sh --environment development
#   deploy.sh --environment production --replicas 3
#   deploy.sh --environment production --external --replicas 2
#   deploy.sh --environment test --down --purge
#
# Idempotent: re-running apply updates the existing stack in place.
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_PREFIX="[sdkwork-deploy]"

info() { printf '%s %s\n' "$LOG_PREFIX" "$*"; }
die()  { printf '%s ERROR: %s\n' "$LOG_PREFIX" "$*" >&2; exit 1; }
usage() { sed -n '2,/^set -euo pipefail$/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0; }

# --- layout autodetection ----------------------------------------------------
if [ -f "${SCRIPT_DIR}/compose/docker-compose.bundle.yml" ]; then
  COMPOSE_DIR="${SCRIPT_DIR}/compose"
  ENV_DIR="${SCRIPT_DIR}/env"
  BUNDLE_IMAGE_TGZ="${SCRIPT_DIR}/image.tar.gz"
  BUNDLE_IMAGE_ENV="${SCRIPT_DIR}/image.env"
else
  COMPOSE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
  ENV_DIR="${COMPOSE_DIR}/env"
  BUNDLE_IMAGE_TGZ=""
  BUNDLE_IMAGE_ENV=""
fi
COMPOSE_FILE="${COMPOSE_DIR}/docker-compose.bundle.yml"
COMPOSE_EDGE_FILE="${COMPOSE_DIR}/docker-compose.bundle-edge.yml"
COMPOSE_GATEWAY_FILE="${COMPOSE_DIR}/docker-compose.bundle-gateway.yml"

# --- defaults ----------------------------------------------------------------
ENVIRONMENT=""
REPLICAS=""
EXTERNAL="0"
ACTION="apply"
PURGE="0"
IMAGE_TAG=""
DRY_RUN="0"

while [ $# -gt 0 ]; do
  case "$1" in
    --environment) ENVIRONMENT="$2"; shift 2 ;;
    --replicas)    REPLICAS="$2"; shift 2 ;;
    --external)    EXTERNAL="1"; shift ;;
    --image-tag)   IMAGE_TAG="$2"; shift 2 ;;
    --down)        ACTION="down"; shift ;;
    --purge)       PURGE="1"; shift ;;
    --ps)          ACTION="ps"; shift ;;
    --logs)        ACTION="logs"; LOG_INSTANCE="${2:-1}"; case "${2:-}" in ''|*[!0-9]*) shift ;; *) shift 2 ;; esac ;;
    --dry-run)     DRY_RUN="1"; shift ;;
    -h|--help)     usage ;;
    *)             die "unsupported option: $1 (see --help)" ;;
  esac
done

[ -n "${ENVIRONMENT}" ] || die "--environment is required (development|test|production)"
case "${ENVIRONMENT}" in
  development|test|production) ;;
  *) die "unsupported environment: ${ENVIRONMENT} (development|test|production)" ;;
esac
command -v docker >/dev/null 2>&1 || die "docker is required"
docker compose version >/dev/null 2>&1 || die "docker compose plugin is required (docker-compose-plugin)"
[ -f "${COMPOSE_FILE}" ] || die "compose template missing: ${COMPOSE_FILE}"

# --- env file ----------------------------------------------------------------
ENV_FILE="${ENV_DIR}/${ENVIRONMENT}.env"
ENV_EXAMPLE="${ENV_DIR}/${ENVIRONMENT}.env.example"
if [ ! -f "${ENV_FILE}" ]; then
  [ -f "${ENV_EXAMPLE}" ] || die "env file missing and no example: ${ENV_EXAMPLE}"
  cp "${ENV_EXAMPLE}" "${ENV_FILE}"
  info "created ${ENV_FILE} from example — fill secrets before exposing beyond localhost"
fi

# Read one KEY=value from the env file (no side effects; values are defaults).
env_key() {
  sed -n "s/^${1}=//p" "${ENV_FILE}" | tail -1 | tr -d '\r'
}

# --- image tag resolution ------------------------------------------------------
if [ -z "${IMAGE_TAG}" ]; then
  IMAGE_TAG="$(env_key SDKWORK_WEBSERVER_IMAGE_TAG)"
fi
if [ -z "${IMAGE_TAG}" ] && [ -n "${BUNDLE_IMAGE_ENV}" ] && [ -f "${BUNDLE_IMAGE_ENV}" ]; then
  IMAGE_TAG="$(sed -n 's/^SDKWORK_WEBSERVER_IMAGE_TAG=//p' "${BUNDLE_IMAGE_ENV}" | tail -1 | tr -d '\r')"
fi
IMAGE_TAG="${IMAGE_TAG:-0.1.0}"
IMAGE_REF="registry.sdkwork.com/apps/sdkwork-webserver-standalone:${IMAGE_TAG}"
export SDKWORK_WEBSERVER_IMAGE_TAG="${IMAGE_TAG}"
export SDKWORK_WEBSERVER_ENVIRONMENT="${ENVIRONMENT}"

# --- per-environment port bases (env file wins over fallbacks) ------------------
case "${ENVIRONMENT}" in
  development)
    PORT_BASE="$(env_key SDKWORK_WEBSERVER_DEV_HOST_PORT)";  PORT_BASE="${PORT_BASE:-13800}"
    EDGE_HTTP="$(env_key SDKWORK_WEBSERVER_DEV_IMPORT_HTTP_HOST_PORT)";  EDGE_HTTP="${EDGE_HTTP:-80}"
    EDGE_HTTPS="$(env_key SDKWORK_WEBSERVER_DEV_HTTPS_HOST_PORT)";  EDGE_HTTPS="${EDGE_HTTPS:-443}"
    ;;
  test)
    PORT_BASE="$(env_key SDKWORK_WEBSERVER_TEST_HOST_PORT)";  PORT_BASE="${PORT_BASE:-18888}"
    EDGE_HTTP="$(env_key SDKWORK_WEBSERVER_TEST_IMPORT_HTTP_HOST_PORT)";  EDGE_HTTP="${EDGE_HTTP:-18898}"
    EDGE_HTTPS="$(env_key SDKWORK_WEBSERVER_TEST_HTTPS_HOST_PORT)";  EDGE_HTTPS="${EDGE_HTTPS:-28430}"
    ;;
  production)
    PORT_BASE="$(env_key SDKWORK_WEBSERVER_PROD_HOST_PORT)";  PORT_BASE="${PORT_BASE:-18080}"
    EDGE_HTTP="$(env_key SDKWORK_WEBSERVER_PROD_IMPORT_HTTP_HOST_PORT)";  EDGE_HTTP="${EDGE_HTTP:-18098}"
    EDGE_HTTPS="$(env_key SDKWORK_WEBSERVER_PROD_HTTPS_HOST_PORT)";  EDGE_HTTPS="${EDGE_HTTPS:-38430}"
    ;;
esac

if [ -z "${REPLICAS}" ]; then
  REPLICAS="$(env_key SDKWORK_WEBSERVER_REPLICAS)"
fi
REPLICAS="${REPLICAS:-1}"
case "${REPLICAS}" in ''|*[!0-9]*) die "--replicas must be a positive integer" ;; esac
[ "${REPLICAS}" -ge 1 ] || die "--replicas must be a positive integer"

DEPS_PROJECT="sdkwork-webserver-${ENVIRONMENT}-deps"
GATEWAY_PROJECT="sdkwork-webserver-${ENVIRONMENT}-gateway"
NETWORK="sdkwork-webserver-${ENVIRONMENT}"
HEALTH_TIMEOUT="${SDKWORK_DEPLOY_HEALTH_TIMEOUT:-600}"

run() {
  info "$*"
  if [ "${DRY_RUN}" != "1" ]; then "$@"; fi
}

# --- shared resources (per environment, shared by all instances) ---------------
ensure_shared_resources() {
  run docker network create "${NETWORK}" 2>/dev/null || info "network ${NETWORK} already present"
  for suffix in secrets data postgres-data redis-data gateway-data gateway-secrets; do
    run docker volume create "sdkwork-webserver-${ENVIRONMENT}-${suffix}" 2>/dev/null \
      || info "volume sdkwork-webserver-${ENVIRONMENT}-${suffix} already present"
  done
}

# --- image ---------------------------------------------------------------------
ensure_image() {
  if [ "${DRY_RUN}" = "1" ]; then
    info "dry-run: would ensure image ${IMAGE_REF} (load bundle image.tar.gz when missing)"
    return 0
  fi
  if docker image inspect "${IMAGE_REF}" >/dev/null 2>&1; then
    info "image present: ${IMAGE_REF}"
    return 0
  fi
  if [ -n "${BUNDLE_IMAGE_TGZ}" ] && [ -f "${BUNDLE_IMAGE_TGZ}" ]; then
    run docker load -i "${BUNDLE_IMAGE_TGZ}"
    return 0
  fi
  die "image ${IMAGE_REF} not found and no bundle image.tar.gz beside this script; build with: pnpm build:container:install"
}

# --- embedded deps ---------------------------------------------------------------
compose_deps() {
  docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" \
    --profile deps "$@"
}

ensure_deps() {
  if [ "${EXTERNAL}" = "1" ]; then
    info "external dependencies mode: using env-file SDKWORK_DATABASE_HOST / redis host"
    return 0
  fi
  # Compose interpolates the whole file (including webserver.ports) even for
  # deps-only projects, so a placeholder instance env must be exported too.
  export_instance_env 1
  run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" --profile deps up -d
  wait_container_healthy "${DEPS_PROJECT}" postgres "${HEALTH_TIMEOUT}" \
    || die "embedded postgres dependency failed readiness"
}

# Gateway sibling (SDKWORK_WEBSERVER_SPEC.md §17.3 / §8.1): binds
# sdkwork-api-cloud-gateway:8080 on the shared network so imported cloud
# sidecar /api/ upstreams resolve. Started only when the operator supplies the
# gateway image (SDKWORK_MODULE_API_GATEWAY_IMAGE, default
# sdkwork-api-cloud-gateway:local); non-blocking when absent.
ensure_gateway() {
  [ -f "${COMPOSE_GATEWAY_FILE}" ] || return 0
  local deployment
  deployment="$(env_key SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT)"
  deployment="${deployment:-docker}"
  case "${deployment}" in
    docker) ;;
    *) info "gateway deployment=${deployment}; skipping gateway sibling"; return 0 ;;
  esac
  local gateway_image
  gateway_image="$(env_key SDKWORK_MODULE_API_GATEWAY_IMAGE)"
  gateway_image="${gateway_image:-sdkwork-api-cloud-gateway:local}"
  if ! docker image inspect "${gateway_image}" >/dev/null 2>&1; then
    info "gateway image ${gateway_image} not present; module /api/ upstream will 504 until provided"
    return 0
  fi
  export_instance_env 1
  run docker compose -p "${GATEWAY_PROJECT}" --env-file "${ENV_FILE}" -f "${COMPOSE_GATEWAY_FILE}" up -d
  if docker compose -p "${GATEWAY_PROJECT}" ps -q knowledgebase-rpc >/dev/null 2>&1; then
    wait_container_healthy "${GATEWAY_PROJECT}" knowledgebase-rpc 120 || true
  fi
  # The webserver does not require the gateway (SDKWORK_MODULE_API_GATEWAY_
  # REQUIRED=false): if the sibling fails readiness (e.g. upstream module
  # content drift), warn and continue so edge/domain serving stays up.
  if ! wait_container_healthy "${GATEWAY_PROJECT}" gateway "${SDKWORK_DEPLOY_GATEWAY_HEALTH_TIMEOUT:-180}"; then
    info "warning: gateway sibling not healthy; module /api/ will 504 until it is fixed"
  fi
}

# --- instances -------------------------------------------------------------------
# Compose interpolates `:?required` values on every command (up/down/ps/logs),
# so per-instance inputs must be exported for all actions, not just apply.
export_instance_env() {
  local index="$1"
  export SDKWORK_WEBSERVER_MGMT_HOST_PORT=$((PORT_BASE + index - 1))
  export SDKWORK_WEBSERVER_NODE_UUID="standalone-${ENVIRONMENT}-i${index}"
}

# Multiple independently configurable webservers: when
# env/<environment>.i<index>.env exists it is layered on top of the base env
# file (later --env-file wins in compose), so each instance can carry its own
# domain (SDKWORK_WEBSERVER_PRIMARY_DOMAIN), clone URL, TLS/ACME profile, or
# any other deployment input.
instance_env_args() {
  local index="$1"
  local override="${ENV_DIR}/${ENVIRONMENT}.i${index}.env"
  if [ -f "${override}" ]; then
    info "instance ${index} config override: ${override}" >&2
    printf '%s' "--env-file ${ENV_FILE} --env-file ${override}"
  else
    printf '%s' "--env-file ${ENV_FILE}"
  fi
}

start_instance() {
  local index="$1"
  local project="sdkwork-webserver-${ENVIRONMENT}-i${index}"
  # Shell env wins over the --env-file in compose; these are per-instance inputs.
  export_instance_env "${index}"
  local args=(-p "${project}" ${INSTANCE_ENV_ARGS[$index]} -f "${COMPOSE_FILE}")
  if [ "${index}" = "1" ] && [ -f "${COMPOSE_EDGE_FILE}" ]; then
    args+=(-f "${COMPOSE_EDGE_FILE}")
    export SDKWORK_WEBSERVER_IMPORT_HTTP_HOST_PORT="${EDGE_HTTP}"
    export SDKWORK_WEBSERVER_IMPORT_HTTPS_HOST_PORT="${EDGE_HTTPS}"
  fi
  args+=(--profile instance up -d)
  run docker compose "${args[@]}"
}

wait_container_healthy() {
  local project="$1" service="$2" timeout="$3"
  [ "${DRY_RUN}" = "1" ] && return 0
  local waited=0 cid status
  while true; do
    cid="$(docker compose -p "${project}" ps -q "${service}" 2>/dev/null || true)"
    if [ -n "${cid}" ]; then
      status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${cid}" 2>/dev/null || true)"
      case "${status}" in
        healthy|running)
          info "${project}/${service} is ${status} (waited ${waited}s)"
          return 0
          ;;
      esac
    fi
    if [ "${waited}" -ge "${timeout}" ]; then
      info "ERROR: ${project}/${service} not healthy after ${timeout}s"
      return 1
    fi
    sleep 5
    waited=$((waited + 5))
  done
}

apply() {
  ensure_shared_resources
  ensure_image
  ensure_deps
  ensure_gateway
  # Instance 1 first: it owns the 80/443 edge and performs database migration
  # before the remaining instances start (DEPLOYMENT_SPEC.md §6).
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    INSTANCE_ENV_ARGS[$index]="$(instance_env_args "${index}")"
    start_instance "${index}"
    wait_container_healthy "sdkwork-webserver-${ENVIRONMENT}-i${index}" webserver "${HEALTH_TIMEOUT}" \
      || die "webserver instance ${index} failed readiness"
  done
  info "environment ${ENVIRONMENT}: ${REPLICAS} instance(s) applied"
  info "instance management ports: $((PORT_BASE))..$((PORT_BASE + REPLICAS - 1)) -> 3800"
  if [ -f "${COMPOSE_EDGE_FILE}" ]; then
    info "instance 1 edge: ${EDGE_HTTP}->80 ${EDGE_HTTPS}->443"
  fi
}

down() {
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    INSTANCE_ENV_ARGS[$index]="$(instance_env_args "${index}")"
    run docker compose -p "sdkwork-webserver-${ENVIRONMENT}-i${index}" ${INSTANCE_ENV_ARGS[$index]} \
      -f "${COMPOSE_FILE}" --profile instance down
  done
  if [ "${EXTERNAL}" != "1" ]; then
    export_instance_env 1
    run docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" --profile deps down
  fi
  if [ -f "${COMPOSE_GATEWAY_FILE}" ]; then
    export_instance_env 1
    run docker compose -p "${GATEWAY_PROJECT}" --env-file "${ENV_FILE}" -f "${COMPOSE_GATEWAY_FILE}" down 2>/dev/null || true
  fi
  if [ "${PURGE}" = "1" ]; then
    info "purging per-environment network and volumes (${ENVIRONMENT})"
    for suffix in "" "-secrets" "-data" "-postgres-data" "-redis-data" "-gateway-data" "-gateway-secrets"; do
      if [ -z "${suffix}" ]; then
        run docker network rm "${NETWORK}" 2>/dev/null || true
      else
        run docker volume rm "sdkwork-webserver-${ENVIRONMENT}${suffix}" 2>/dev/null || true
      fi
    done
  fi
  info "environment ${ENVIRONMENT} down"
}

ps() {
  local index
  for index in $(seq 1 "${REPLICAS}"); do
    export_instance_env "${index}"
    INSTANCE_ENV_ARGS[$index]="$(instance_env_args "${index}")"
    info "instance ${index}:"
    docker compose -p "sdkwork-webserver-${ENVIRONMENT}-i${index}" ${INSTANCE_ENV_ARGS[$index]} \
      -f "${COMPOSE_FILE}" --profile instance ps
  done
  if [ "${EXTERNAL}" != "1" ]; then
    export_instance_env 1
    info "deps:"
    docker compose -p "${DEPS_PROJECT}" --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" --profile deps ps
  fi
}

logs() {
  local index="${LOG_INSTANCE:-1}"
  case "${index}" in ''|*[!0-9]*) die "--logs expects an instance number" ;; esac
  export_instance_env "${index}"
  INSTANCE_ENV_ARGS[$index]="$(instance_env_args "${index}")"
  docker compose -p "sdkwork-webserver-${ENVIRONMENT}-i${index}" ${INSTANCE_ENV_ARGS[$index]} \
    -f "${COMPOSE_FILE}" --profile instance logs -f --tail 200 webserver
}

case "${ACTION}" in
  apply) apply ;;
  down)  down ;;
  ps)    ps ;;
  logs)  logs ;;
esac
