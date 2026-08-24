#!/usr/bin/env bash
# Clone or update sdkwork-space on the Ubuntu/WSL host under /opt/deploy.
# The same host path is bind-mounted into every webserver container as /opt/deploy.
set -euo pipefail

SPACE_ROOT="${SDKWORK_SPACE_ROOT:-/opt/deploy}"
CLONE_URL="${SDKWORK_SPACE_CLONE_URL:-https://github.com/sdkwork-ai/sdkwork-space.git}"
LOCAL_PATH="${SDKWORK_SPACE_LOCAL_PATH:-}"
CHECKOUT="${SPACE_ROOT}/sdkwork-space"
RECURSE_SUBMODULES="${SDKWORK_SPACE_RECURSE_SUBMODULES:-true}"

log() {
  echo "[sdkwork-webserver-space-clone] $*"
}

link_local_checkout() {
  if [ -z "${LOCAL_PATH}" ]; then
    return 1
  fi
  if [ ! -d "${LOCAL_PATH}" ]; then
    log "warning: SDKWORK_SPACE_LOCAL_PATH=${LOCAL_PATH} does not exist"
    return 1
  fi
  install -d -m 0755 "${SPACE_ROOT}"
  if [ -e "${CHECKOUT}" ] && [ ! -L "${CHECKOUT}" ]; then
    log "warning: ${CHECKOUT} exists and is not a symlink; keeping existing tree"
    return 0
  fi
  rm -f "${CHECKOUT}"
  ln -sfn "${LOCAL_PATH}" "${CHECKOUT}"
  log "linked ${CHECKOUT} -> ${LOCAL_PATH}"
  return 0
}

require_git() {
  if ! command -v git >/dev/null 2>&1; then
    log "installing git"
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq git
  fi
}

configure_submodule_https() {
  # .gitmodules uses git@github.com: URLs; rewrite to HTTPS for hosts without SSH keys.
  git config --global url."https://github.com/".insteadOf "git@github.com:" 2>/dev/null || true
}

sync_submodules() {
  if [ "${RECURSE_SUBMODULES}" != "true" ]; then
    return 0
  fi
  configure_submodule_https
  log "initializing submodules (recursive)"
  git -C "${CHECKOUT}" submodule sync --recursive
  git -C "${CHECKOUT}" submodule update --init --recursive --jobs "$(nproc 2>/dev/null || echo 4)"
}

main() {
  if link_local_checkout; then
    log "using local workspace checkout at ${CHECKOUT}"
    exit 0
  fi

  require_git
  configure_submodule_https
  install -d -m 0755 "${SPACE_ROOT}"
  if [ ! -d "${CHECKOUT}/.git" ]; then
    if [ -e "${CHECKOUT}" ]; then
      log "removing existing ${CHECKOUT} (not a git checkout)"
      rm -rf "${CHECKOUT}"
    fi
    log "cloning ${CLONE_URL} -> ${CHECKOUT}"
    if [ "${RECURSE_SUBMODULES}" = "true" ]; then
      git clone --recurse-submodules "${CLONE_URL}" "${CHECKOUT}"
    else
      git clone "${CLONE_URL}" "${CHECKOUT}"
    fi
  else
    log "updating ${CHECKOUT}"
    git -C "${CHECKOUT}" fetch origin
    git -C "${CHECKOUT}" pull --ff-only
    sync_submodules
  fi
  log "space checkout ready at ${CHECKOUT}"
  if [ "${RECURSE_SUBMODULES}" = "true" ]; then
    submodule_count="$(git -C "${CHECKOUT}" submodule status --recursive 2>/dev/null | wc -l | tr -d ' ')"
    log "submodules initialized: ${submodule_count}"
  fi
  log "bind mount ${SPACE_ROOT}:/opt/deploy in docker compose to share modules across clusters"
}

main "$@"
