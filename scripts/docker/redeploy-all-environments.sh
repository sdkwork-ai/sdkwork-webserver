#!/usr/bin/env bash
# Rebuild browser static assets, standalone release archive, Docker image, and
# redeploy every external-mode environment (development, test, production).
#
# Modeled on deployments/docker/scripts/wsl-external-deploy.sh but focused on
# shipping a fresh frontend + gateway bundle without reprovisioning databases.
#
# Usage:
#   bash scripts/docker/redeploy-all-environments.sh
#   bash scripts/docker/redeploy-all-environments.sh --deploy-only
#   bash scripts/docker/redeploy-all-environments.sh --skip-frontend-build --skip-release-build
#
# WSL one-liner (from Windows repo checkout on /mnt/<drive>/...):
#   wsl -e bash -lc "cd /mnt/e/sdkwork-space/sdkwork-webserver && bash scripts/docker/redeploy-all-environments.sh"
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
docker_root="$repo_root/deployments/docker"

skip_frontend_build=false
skip_release_build=false
skip_image_build=false
deploy_only=false
validate=true
pull_image=false

usage() {
  cat <<'EOF'
Usage: redeploy-all-environments.sh [options]

Rebuilds PC/H5 standalone.production static assets, packages the linux-x64
standalone release, builds the Docker image, and redeploys development, test,
and production compose stacks (external PostgreSQL/Redis).

Options:
  --deploy-only              Skip build steps; redeploy existing image tag only
  --skip-frontend-build      Reuse apps/*/dist/standalone/prod from the working tree
  --skip-release-build       Reuse dist/release/*.tar.gz when packaging image
  --skip-image-build         Skip docker build; compose up existing image tag
  --no-validate              Skip validate-docker-deployment.mjs before compose up
  --pull                     docker compose pull before up
  -h, --help                 Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --deploy-only)
      deploy_only=true
      skip_frontend_build=true
      skip_release_build=true
      skip_image_build=true
      shift
      ;;
    --skip-frontend-build) skip_frontend_build=true; shift ;;
    --skip-release-build) skip_release_build=true; shift ;;
    --skip-image-build) skip_image_build=true; shift ;;
    --no-validate) validate=false; shift ;;
    --pull) pull_image=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unsupported option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

log() { echo "[redeploy-all] $*"; }

ensure_linux_release_stage_parent() {
  if [ "$(uname -s)" != "Linux" ]; then
    return 0
  fi
  if [ -n "${SDKWORK_RELEASE_STAGE_PARENT:-}" ]; then
    return 0
  fi
  case "$repo_root" in
    /mnt/*)
      export SDKWORK_RELEASE_STAGE_PARENT="/tmp/sdkwork-release-stage"
      log "repo on /mnt/* — using SDKWORK_RELEASE_STAGE_PARENT=${SDKWORK_RELEASE_STAGE_PARENT}"
      ;;
  esac
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

build_browser_app() {
  local architecture="$1"
  local lifecycle_environment="${2:-production}"
  local deployment_profile="${3:-standalone}"
  log "building webserver ${architecture} ${deployment_profile}.${lifecycle_environment}"
  # Canonical runner materializes runtime-env, ensures bootstrap access token
  # for development/test, and writes dist/<profile>/<alias>/ (FRONTEND_CODE_SPEC §7).
  if ! (
    cd "$repo_root"
    node ../sdkwork-specs/tools/build-browser-client.mjs \
      --root . \
      --architecture "$architecture" \
      --environment "$lifecycle_environment" \
      --deployment-profile "$deployment_profile"
  ); then
    return 1
  fi
  local env_alias
  case "$lifecycle_environment" in
    development) env_alias=dev ;;
    production) env_alias=prod ;;
    *) env_alias="$lifecycle_environment" ;;
  esac
  local index_path="$repo_root/apps/sdkwork-webserver-${architecture}/dist/${deployment_profile}/${env_alias}/index.html"
  if [ ! -f "$index_path" ]; then
    echo "missing ${index_path#$repo_root/} after vite build" >&2
    return 1
  fi
}

build_frontend_assets() {
  require_command pnpm
  require_command node
  if [ "$(uname -s)" = "Linux" ] && [ -d "$repo_root/node_modules" ]; then
    if ! node -e "require('@rolldown/binding-linux-x64-gnu')" >/dev/null 2>&1; then
      log "installing Linux-native frontend toolchain (rolldown binding)"
      (cd "$repo_root" && pnpm install --frozen-lockfile)
    fi
  fi
  # Webserver console is standalone-only (SDKWORK_WEBSERVER_SPEC.md §17.4).
  build_browser_app pc production standalone
  build_browser_app h5 production standalone
}

reuse_existing_frontend_dist() {
  local pc_index="$repo_root/apps/sdkwork-webserver-pc/dist/standalone/prod/index.html"
  local h5_index="$repo_root/apps/sdkwork-webserver-h5/dist/standalone/prod/index.html"
  if [ -f "$pc_index" ] && [ -f "$h5_index" ]; then
    log "reusing existing dist/standalone/prod from working tree (PC + H5)"
    assert_no_stale_random_uuid_in_dist
    return 0
  fi
  return 1
}

package_release_archive() {
  if [ "$skip_release_build" = true ]; then
    log "skipping release archive build (--skip-release-build)"
    return 0
  fi
  if [ "$(uname -s)" != "Linux" ]; then
    echo "release packaging requires Linux (use WSL): uname -s=$(uname -s)" >&2
    exit 1
  fi
  ensure_linux_release_stage_parent
  log "packaging standalone linux-x64 release archive"
  (
    cd "$repo_root"
    node scripts/webserver-release.mjs package \
      --deployment-profile standalone \
      --architecture x64 \
      --skip-pc-build \
      --skip-h5-build
  )
}

build_docker_image() {
  if [ "$skip_image_build" = true ]; then
    log "skipping docker image build (--skip-image-build)"
    return 0
  fi
  require_command docker
  log "building standalone docker image"
  local args=(node scripts/docker/build-standalone-image.mjs)
  if [ "$skip_release_build" = true ]; then
    args+=(--skip-release-build)
  fi
  (cd "$repo_root" && "${args[@]}")
}

stop_existing_stacks() {
  log "recycling existing webserver compose projects"
  for env_name in development test production; do
    bash "$repo_root/scripts/docker/deploy-docker-environment.sh" "$env_name" --down || true
  done
}

deploy_all_environments() {
  local deploy_args=(all)
  if [ "$validate" = true ]; then
    deploy_args+=(--validate)
  fi
  if [ "$pull_image" = true ]; then
    deploy_args+=(--pull)
  fi
  bash "$repo_root/scripts/docker/deploy-docker-environment.sh" "${deploy_args[@]}"
}

verify_health() {
  # shellcheck source=resolve-host-ports.sh
  source "$docker_root/scripts/resolve-host-ports.sh"
  log "waiting for gateway health checks..."
  sleep 20
  for env_name in development test production; do
    local env_file="$docker_root/env/${env_name}.env"
    load_host_ports_from_env "$env_file"
    local port
    port="$(host_http_port_for "$env_name")"
    if curl -fsS "http://127.0.0.1:${port}/healthz" >/dev/null 2>&1; then
      log "  ${env_name}: http://127.0.0.1:${port}/healthz OK"
    else
      echo "health check failed for ${env_name} on port ${port}" >&2
      docker logs "sdkwork-webserver-${env_name}" --tail 80 2>&1 || true
      return 1
    fi
  done
}

assert_no_stale_random_uuid_in_dist() {
  local pc_dist="$repo_root/apps/sdkwork-webserver-pc/dist/standalone/prod"
  if command -v rg >/dev/null 2>&1; then
    if rg -q 'globalThis\.crypto\.randomUUID|crypto\.randomUUID\(\)' \
      "$pc_dist" 2>/dev/null; then
      echo "PC dist/standalone/prod still contains direct crypto.randomUUID calls; rebuild required" >&2
      exit 1
    fi
    return 0
  fi
  if grep -RqE 'globalThis\.crypto\.randomUUID|crypto\.randomUUID\(\)' \
    "$pc_dist" 2>/dev/null; then
    echo "PC dist/standalone/prod still contains direct crypto.randomUUID calls; rebuild required" >&2
    exit 1
  fi
}

main() {
  require_command docker
  require_command curl

  if [ "$deploy_only" = false ]; then
    if [ "$skip_frontend_build" = false ]; then
      if ! build_frontend_assets; then
        if reuse_existing_frontend_dist; then
          log "frontend vite build unavailable; continuing with existing dist/standalone/prod"
        else
          exit 1
        fi
      fi
    elif reuse_existing_frontend_dist; then
      log "skipping frontend vite builds (--skip-frontend-build)"
    else
      echo "missing dist/standalone/prod; run vite build on the host or omit --skip-frontend-build" >&2
      exit 1
    fi
    assert_no_stale_random_uuid_in_dist
    package_release_archive
    build_docker_image
  fi

  stop_existing_stacks
  deploy_all_environments
  verify_health

  log "redeploy complete"
  log "  development -> http://127.0.0.1:13800/healthz (server-dev.sdkwork.com)"
  log "  test        -> http://127.0.0.1:18888/healthz (server-test.sdkwork.com)"
  log "  production  -> http://127.0.0.1:18080/healthz (server.sdkwork.com)"
}

main "$@"
