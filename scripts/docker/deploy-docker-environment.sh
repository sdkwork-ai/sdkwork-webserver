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
    development) echo 13800 ;;
    test) echo 18888 ;;
    production) echo 18080 ;;
    *) echo "?" ;;
  esac
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

  if [ "$down" = true ]; then
    docker compose "${compose_args[@]}" down --remove-orphans
    echo "stopped $env_name ($project)"
    return 0
  fi

  if [ "$pull" = true ]; then
    docker compose "${compose_args[@]}" pull
  fi

  docker compose "${compose_args[@]}" up -d --remove-orphans
  echo "deployed $env_name ($project) -> http://127.0.0.1:${port}/healthz"
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
