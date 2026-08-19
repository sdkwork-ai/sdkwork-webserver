#!/usr/bin/env bash
# Deploy sdkwork-webserver standalone gateway stacks in Docker (WSL/Ubuntu).
#
# Modeled on sdkwork-api-cloud-gateway/scripts/deploy-docker-environment.sh.
#
# Usage:
#   bash scripts/docker/deploy-docker-environment.sh development
#   bash scripts/docker/deploy-docker-environment.sh test --external
#   bash scripts/docker/deploy-docker-environment.sh all --embedded-shared
set -eu

usage() {
  cat <<'EOF'
Usage: deploy-docker-environment.sh <environment|all> [options]

Environments: development, test, production, all

Options:
  --embedded          use built-in postgres/redis containers (default)
  --embedded-shared   deploy all environments in one compose project (shared postgres)
  --external          use external PostgreSQL/Redis (docker-compose.external.yml)
  --validate          validate env file before compose up
  --down              stop the selected stack
  -h, --help          show this help
EOF
}

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
docker_root="$repo_root/deployments/docker"
mode=embedded
down=false
validate=false
target=${1:-}

shift || true
while [ "$#" -gt 0 ]; do
  case "$1" in
    --embedded) mode=embedded; shift ;;
    --embedded-shared) mode=embedded-shared; shift ;;
    --external) mode=external; shift ;;
    --validate) validate=true; shift ;;
    --down) down=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unsupported option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$target" ]; then
  usage >&2
  exit 2
fi

ensure_env_file() {
  env_name=$1
  env_file="$docker_root/env/$env_name.env"
  if [ ! -f "$env_file" ]; then
    if [ ! -f "$docker_root/env/$env_name.env.example" ]; then
      echo "missing env template: $docker_root/env/$env_name.env.example" >&2
      exit 1
    fi
    cp "$docker_root/env/$env_name.env.example" "$env_file"
    echo "created $env_file from example; fill secrets before production use" >&2
  fi
  printf '%s' "$env_file"
}

compose_files=(-f "$docker_root/docker-compose.yml")
if [ "$mode" = external ]; then
  compose_files+=(-f "$docker_root/docker-compose.external.yml")
fi

deploy_one() {
  env_name=$1
  project=$2
  env_file=$(ensure_env_file "$env_name")

  if [ "$validate" = true ] && [ "$down" = false ]; then
    node_args=(--env-file "$env_file")
    if [ "$mode" = external ]; then
      node_args+=(--mode external)
    else
      node_args+=(--mode embedded)
    fi
    (cd "$repo_root" && node scripts/docker/validate-docker-deployment.mjs "${node_args[@]}")
  fi

  args=(compose --env-file "$env_file" -p "$project" "${compose_files[@]}" --profile "$env_name")
  if [ "$down" = true ]; then
    docker "${args[@]}" down
    echo "stopped $env_name ($project)"
    return 0
  fi
  docker "${args[@]}" up -d

  port_key="SDKWORK_WEBSERVER_${env_name^^}_HOST_PORT"
  port_key=${port_key/production/PROD}
  port_key=${port_key/development/DEV}
  port_key=${port_key/test/TEST}
  case "$env_name" in
    development) port=13800 ;;
    test) port=18888 ;;
    production) port=18080 ;;
    *) port='?' ;;
  esac
  if grep -E '^SDKWORK_WEBSERVER_.*_HOST_PORT=' "$env_file" >/dev/null 2>&1; then
    case "$env_name" in
      development) port=$(grep -E '^SDKWORK_WEBSERVER_DEV_HOST_PORT=' "$env_file" | cut -d= -f2-) ;;
      test) port=$(grep -E '^SDKWORK_WEBSERVER_TEST_HOST_PORT=' "$env_file" | cut -d= -f2-) ;;
      production) port=$(grep -E '^SDKWORK_WEBSERVER_PROD_HOST_PORT=' "$env_file" | cut -d= -f2-) ;;
    esac
  fi
  echo "deployed $env_name ($project, mode=$mode) -> http://127.0.0.1:${port}/healthz"
}

deploy_shared_all() {
  env_file=$(ensure_env_file development)
  project=sdkwork-webserver-shared
  if [ "$validate" = true ] && [ "$down" = false ]; then
    for env_name in development test production; do
      ef=$(ensure_env_file "$env_name")
      (cd "$repo_root" && node scripts/docker/validate-docker-deployment.mjs --env-file "$ef" --mode embedded)
    done
  fi
  args=(compose --env-file "$env_file" -p "$project" "${compose_files[@]}" \
    --profile development --profile test --profile production)
  if [ "$down" = true ]; then
    docker "${args[@]}" down
    echo "stopped shared stack ($project)"
    return 0
  fi
  docker "${args[@]}" up -d
  echo "deployed shared embedded stack ($project)"
}

case "$target" in
  development|test|production)
    if [ "$mode" = embedded-shared ]; then
      echo "--embedded-shared requires target all" >&2
      exit 2
    fi
    deploy_one "$target" "sdkwork-webserver-$target"
    ;;
  all)
    if [ "$mode" = embedded-shared ]; then
      deploy_shared_all
    else
      for env_name in development test production; do
        deploy_one "$env_name" "sdkwork-webserver-$env_name"
      done
    fi
    ;;
  *)
    echo "unsupported environment: $target" >&2
    usage >&2
    exit 2
    ;;
esac
