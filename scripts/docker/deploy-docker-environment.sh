#!/usr/bin/env bash
# Deploy sdkwork-webserver standalone gateway stacks in Docker (WSL/Ubuntu).
#
# Thin orchestration wrapper only: every compose invocation delegates to
# scripts/docker/compose.mjs (single reusable entry point; external layout).
# This shell layer owns host-side concerns the Node driver must not duplicate:
# env file existence checks, host port reporting, and post-deploy hints.
#
# Modeled on sdkwork-api-cloud-gateway/scripts/deploy-docker-environment.sh.
#
# Each environment runs as an isolated compose project with a distinct host port
# and sdkwork-specs database identity (sdkwork_ai_dev/test/prod). External
# PostgreSQL and Redis must already be reachable from the container (typically
# host.docker.internal on WSL).
#
# Usage:
#   bash scripts/docker/deploy-docker-environment.sh development
#   bash scripts/docker/deploy-docker-environment.sh test
#   bash scripts/docker/deploy-docker-environment.sh staging
#   bash scripts/docker/deploy-docker-environment.sh production
#   bash scripts/docker/deploy-docker-environment.sh all
#   bash scripts/docker/deploy-docker-environment.sh all --down
set -eu

usage() {
  cat <<'EOF'
Usage: deploy-docker-environment.sh <environment|all> [options]

Environments: development, test, staging, production, all
(staging is a single-target deployment; `all` sweeps development/test/production)

Options:
  --down          stop the selected stack(s) instead of starting
  --validate      validate env file before compose up
  --pull          docker compose pull before up
  -h, --help      show this help
EOF
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
docker_root="$repo_root/deployments/docker"
down=false
validate=false
pull=false
target=${1:-}

shift || true
while [ "$#" -gt 0 ]; do
  case "$1" in
    --down) down=true; shift ;;
    --validate) validate=true; shift ;;
    --pull) pull=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unsupported option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$target" ]; then
  usage >&2
  exit 2
fi

port_for() {
  case "$1" in
    development) echo "${SDKWORK_WEBSERVER_DEV_HOST_PORT:-13800}" ;;
    test) echo "${SDKWORK_WEBSERVER_TEST_HOST_PORT:-18888}" ;;
    staging) echo "${SDKWORK_WEBSERVER_STAGING_HOST_PORT:-18081}" ;;
    production) echo "${SDKWORK_WEBSERVER_PROD_HOST_PORT:-18080}" ;;
    *) echo "?" ;;
  esac
}

load_host_ports() {
  env_file=$1
  if [ -f "$env_file" ]; then
    # shellcheck disable=SC1090
    set -a
    # Only import host port variables so secrets are not exported broadly.
    eval "$(grep -E '^SDKWORK_WEBSERVER_(DEV|TEST|STAGING|PROD)_HOST_PORT=' "$env_file" || true)"
    set +a
  fi
}

# Single compose entry point (scripts/docker/compose.mjs). The external layout
# resolves docker-compose.<environment>.yml plus the standalone
# platform-api-gateway overlay, exactly like the previous inline compose calls.
compose_driver() {
  local environment="$1"
  local action="$2"
  shift 2
  local args=(node scripts/docker/compose.mjs "$action" --environment "$environment" --layout external)
  if [ "$validate" = true ] && [ "$down" = false ] && [ "$action" = "up" ]; then
    args+=(--validate)
  fi
  args+=("$@")
  (cd "$repo_root" && "${args[@]}")
}

deploy_one() {
  env_name=$1
  env_file="$docker_root/env/${env_name}.env"
  if [ ! -f "$env_file" ]; then
    echo "missing env file: $env_file" >&2
    echo "copy docker/env/${env_name}.env.example and fill secrets first" >&2
    exit 1
  fi
  load_host_ports "$env_file"
  port=$(port_for "$env_name")

  if [ -n "${SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK:-}" ]; then
    echo "module-api-gateway=docker attach-network=${SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK} host=${SDKWORK_MODULE_API_GATEWAY_HOST:-gateway}"
  fi

  if [ "$down" = true ]; then
    compose_driver "$env_name" down
    echo "stopped $env_name (sdkwork-webserver-$env_name)"
    return 0
  fi

  if [ "$pull" = true ]; then
    compose_driver "$env_name" pull
  fi

  compose_driver "$env_name" up
  echo "deployed $env_name (sdkwork-webserver-$env_name) -> http://127.0.0.1:${port}/healthz"
}

case "$target" in
  development|test|staging|production)
    deploy_one "$target"
    ;;
  all)
    for env_name in development test production; do
      deploy_one "$env_name"
    done
    ;;
  *)
    echo "unsupported environment: $target" >&2
    usage >&2
    exit 2 ;;
esac
