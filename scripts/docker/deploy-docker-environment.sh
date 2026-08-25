#!/usr/bin/env bash
# Deploy sdkwebwork-webserver standalone gateway stacks in Docker (WSL/Ubuntu).
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
#   bash scripts/docker/deploy-docker-environment.sh production
#   bash scripts/docker/deploy-docker-environment.sh all
#   bash scripts/docker/deploy-docker-environment.sh all --down
set -eu

usage() {
  cat <<'EOF'
Usage: deploy-docker-environment.sh <environment|all> [options]

Environments: development, test, production, all

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

# Map environment name to its standalone compose file and default host port.
compose_file_for() {
  case "$1" in
    development) echo "$docker_root/docker-compose.development.yml" ;;
    test) echo "$docker_root/docker-compose.test.yml" ;;
    production) echo "$docker_root/docker-compose.production.yml" ;;
    *) return 1 ;;
  esac
}

port_for() {
  case "$1" in
    development) echo "${SDKWORK_WEBSERVER_DEV_HOST_PORT:-13800}" ;;
    test) echo "${SDKWORK_WEBSERVER_TEST_HOST_PORT:-18888}" ;;
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
    eval "$(grep -E '^SDKWORK_WEBSERVER_(DEV|TEST|PROD)_HOST_PORT=' "$env_file" || true)"
    set +a
  fi
}

load_module_api_gateway_settings() {
  env_file=$1
  SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker
  SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK=
  if [ -f "$env_file" ]; then
    # shellcheck disable=SC1090
    set -a
    eval "$(grep -E '^SDKWORK_MODULE_API_GATEWAY_(DEPLOYMENT|ATTACH_NETWORK|HOST|PORT)=' "$env_file" || true)"
    set +a
  fi
  SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT="${SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT:-docker}"
}

compose_platform_gateway_args() {
  deployment="${SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT:-docker}"
  attach_network="${SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK:-}"
  # Prefer attaching to an already-running independent gateway compose network.
  if [ -n "$attach_network" ]; then
    printf '%s\n' "-f" "$docker_root/docker-compose.platform-api-gateway-attach.yml"
    return 0
  fi
  if [ "$deployment" = "docker" ]; then
    printf '%s\n' "-f" "$docker_root/docker-compose.platform-api-gateway.yml"
  fi
}

env_file_for() {
  case "$1" in
    development) echo "$docker_root/env/development.env" ;;
    test) echo "$docker_root/env/test.env" ;;
    production) echo "$docker_root/env/production.env" ;;
    *) return 1 ;;
  esac
}

ensure_env_file() {
  env_name=$1
  env_file=$(env_file_for "$env_name")
  if [ ! -f "$env_file" ]; then
    echo "missing env file: $env_file" >&2
    echo "copy docker/env/$env_name.env.example and fill secrets first" >&2
    exit 1
  fi
  printf '%s' "$env_file"
}

deploy_one() {
  env_name=$1
  compose_file=$(compose_file_for "$env_name")
  env_file=$(ensure_env_file "$env_name")
  load_host_ports "$env_file"
  load_module_api_gateway_settings "$env_file"
  gateway_deployment="${SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT:-docker}"
  project="sdkwork-webserver-$env_name"
  port=$(port_for "$env_name")

  if [ "$validate" = true ] && [ "$down" = false ]; then
    (cd "$repo_root" && node scripts/docker/validate-docker-deployment.mjs --env-file "$env_file" --mode external)
  fi

  compose_args=(
    --env-file "$env_file"
    -p "$project"
    -f "$compose_file"
  )
  while IFS= read -r extra_arg; do
    [ -n "$extra_arg" ] || continue
    compose_args+=("$extra_arg")
  done < <(compose_platform_gateway_args)

  if [ -n "${SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK:-}" ]; then
    echo "module-api-gateway=docker attach-network=${SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK} host=${SDKWORK_MODULE_API_GATEWAY_HOST:-gateway}"
  fi

  if [ "$down" = true ]; then
    docker compose "${compose_args[@]}" down --remove-orphans
    echo "stopped $env_name ($project)"
    return 0
  fi

  if [ "$pull" = true ]; then
    docker compose "${compose_args[@]}" pull
  fi

  docker compose "${compose_args[@]}" up -d --remove-orphans
  echo "deployed $env_name ($project) -> http://127.0.0.1:${port}/healthz (module-api-gateway=${gateway_deployment})"
}

case "$target" in
  development|test|production)
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
