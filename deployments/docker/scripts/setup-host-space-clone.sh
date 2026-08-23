#!/usr/bin/env bash
# Clone or update sdkwork-space on the Ubuntu/WSL host under /opt/deploy.
# The same host path is bind-mounted into every webserver container as /opt/deploy.
set -euo pipefail

SPACE_ROOT="${SDKWORK_SPACE_ROOT:-/opt/deploy}"
CLONE_URL="${SDKWORK_SPACE_CLONE_URL:-https://github.com/Sdkwork-Cloud/sdkwork-space.git}"
LOCAL_PATH="${SDKWORK_SPACE_LOCAL_PATH:-}"
CHECKOUT="${SPACE_ROOT}/sdkwork-space"

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

main() {
  if link_local_checkout; then
    log "using local workspace checkout at ${CHECKOUT}"
    exit 0
  fi

  require_git
  install -d -m 0755 "${SPACE_ROOT}"
  if [ ! -d "${CHECKOUT}/.git" ]; then
    log "cloning ${CLONE_URL} -> ${CHECKOUT}"
    git clone --depth 1 "${CLONE_URL}" "${CHECKOUT}"
  else
    log "updating ${CHECKOUT}"
    git -C "${CHECKOUT}" fetch --depth 1 origin
    git -C "${CHECKOUT}" pull --ff-only
  fi
  log "space checkout ready at ${CHECKOUT}"
  log "bind mount ${SPACE_ROOT}:/opt/deploy in docker compose to share modules across clusters"
}

main "$@"
