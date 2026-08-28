#!/usr/bin/env bash
# Provision WSL host-native external deps and deploy all sdkwork-webserver
# environments (external mode). Modeled on sdkwork-api-cloud-gateway.
#
# This script:
#   1. Ensures host Redis is running and accessible
#   2. Provisions PostgreSQL databases for each environment
#   3. Stops any existing embedded stacks
#   4. Deploys all three environments (development, test, production)
#   5. Configures /etc/hosts and nginx for domain routing
# Usage:
#   sudo bash deployments/docker/scripts/wsl-external-deploy.sh
#   sudo bash deployments/docker/scripts/wsl-external-deploy.sh --rebuild
#   sudo bash deployments/docker/scripts/wsl-external-deploy.sh --rebuild --skip-frontend-build
set -euo pipefail

rebuild=false
rebuild_args=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --rebuild)
      rebuild=true
      shift
      ;;
    --skip-frontend-build|--skip-release-build|--skip-image-build|--deploy-only|--no-validate|--pull)
      rebuild=true
      rebuild_args+=("$1")
      shift
      ;;
    -h|--help)
      cat <<'EOF'
Usage: wsl-external-deploy.sh [options]

  --rebuild                  Rebuild PC/H5, release archive, Docker image, then deploy
  --skip-frontend-build      Pass through to redeploy-all-environments.sh
  --skip-release-build       Pass through to redeploy-all-environments.sh
  --skip-image-build         Pass through to redeploy-all-environments.sh
  --deploy-only              Redeploy existing image only (no rebuild)
  --no-validate              Skip validate-docker-deployment.mjs before compose up
  --pull                     docker compose pull before up
EOF
      exit 0
      ;;
    *)
      echo "unsupported option: $1" >&2
      exit 2
      ;;
  esac
done

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
docker_root="$repo_root/deployments/docker"
# shellcheck source=resolve-host-ports.sh
source "$docker_root/scripts/resolve-host-ports.sh"
# shellcheck source=ensure-host-redis.sh
source "$docker_root/scripts/ensure-host-redis.sh"

log() { echo "[wsl-external-deploy] $*"; }

stop_existing_stacks() {
  log "stopping existing webserver stacks"
  for project in sdkwork-webserver-development sdkwork-webserver-test sdkwork-webserver-production; do
    docker compose -p "$project" down --remove-orphans 2>/dev/null || true
  done
  # Also stop old-style compose projects
  for env_name in development test production; do
    for mode in "" "-external"; do
      docker compose -p "sdkwork-webserver-${env_name}${mode}" \
        -f "$docker_root/docker-compose.yml" \
        ${mode:+-f "$docker_root/docker-compose.external.yml"} \
        down --remove-orphans 2>/dev/null || true
    done
  done
}

deploy_environment() {
  local env_name="$1"
  bash "$repo_root/scripts/docker/deploy-docker-environment.sh" "$env_name" --validate
}

verify_environment() {
  local env_name="$1"
  local env_file="$docker_root/env/${env_name}.env"
  load_host_ports_from_env "${env_file}"
  local port domain https_port
  port="$(host_http_port_for "${env_name}")"
  https_port="$(host_https_port_for "${env_name}")"
  domain="$(domain_for "${env_name}")"

  log "verifying $env_name..."
  if curl -fsS "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
    log "  direct port ${port}: OK"
  else
    log "  direct port ${port}: FAILED"
    return 1
  fi
  if curl -fsS -H "Host: ${domain}" "http://127.0.0.1/healthz" >/dev/null 2>&1; then
    log "  domain ${domain} (nginx :80): OK"
  else
    log "  domain ${domain} (nginx :80): FAILED (nginx may need configuration)"
  fi
  if curl -kfsS "https://127.0.0.1:${https_port}/healthz" >/dev/null 2>&1; then
    log "  direct https port ${https_port}: OK"
  else
    log "  direct https port ${https_port}: skipped or not ready"
  fi
}

main() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "run as root: sudo bash deployments/docker/scripts/wsl-external-deploy.sh" >&2
    exit 1
  fi

  ensure_host_redis

  bash "$docker_root/scripts/setup-host-space-clone.sh" || log "warning: sdkwork-space clone skipped or failed"

  # Provision PostgreSQL databases for each environment (ENVIRONMENT_SPEC §7.1)
  bash "$docker_root/scripts/setup-host-external-deps.sh"

  if [ "$rebuild" = true ]; then
    log "rebuild frontend, release archive, docker image, and redeploy all environments"
    bash "$repo_root/scripts/docker/redeploy-all-environments.sh" "${rebuild_args[@]}"
  else
    stop_existing_stacks
    for env_name in development test production; do
      deploy_environment "$env_name"
    done
  fi

  # Configure hosts; retire host nginx (Docker webserver owns reverse proxy)
  bash "$docker_root/scripts/install-wsl-hosts.sh" || true
  bash "$docker_root/scripts/uninstall-wsl-nginx.sh" || true

  # Wait for health checks
  log "waiting for health checks..."
  sleep 15

  # Verify all environments
  verify_environment development
  verify_environment test
  verify_environment production

  log "deployment complete"
  log ""
  log "Environment access (Docker published ports; host nginx retired):"
  log "  Management:  http://server-dev.sdkwork.com:13800  http://server-test.sdkwork.com:18888  http://server.sdkwork.com:18080"
  log "  Public HTTP/HTTPS: development owns host :80 / :443; test :18898/:28430; prod :18098/:38430"
  log "  Example: curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz"
  log ""
  log "Database connections (external PostgreSQL):"
  log "  Development: sdkwork_ai_dev"
  log "  Test:        sdkwork_ai_test"
  log "  Production:  sdkwork_ai_prod"
}

main "$@"
