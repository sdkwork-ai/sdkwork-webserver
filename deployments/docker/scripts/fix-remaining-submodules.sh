#!/usr/bin/env bash
# Force-init any remaining uninitialized submodules under /opt/deploy/sdkwork-space.
set -euo pipefail

CHECKOUT="${SDKWORK_SPACE_CHECKOUT:-/opt/deploy/sdkwork-space}"
REFERENCE="${SDKWORK_SPACE_REFERENCE:-/mnt/e/sdkwork-space}"

log() { echo "[fix-remaining-submodules] $*"; }

git config --global url."https://github.com/".insteadOf "git@github.com:" 2>/dev/null || true
find "${CHECKOUT}" -name 'index.lock' -delete 2>/dev/null || true

git -C "${CHECKOUT}" submodule sync --recursive

while IFS= read -r path; do
  [ -n "${path}" ] || continue
  log "initializing ${path}"
  dst="${CHECKOUT}/${path}"
  ref="${REFERENCE}/${path}"

  if [ ! -e "${dst}/.git" ] && [ -d "${ref}/.git" -o -f "${ref}/.git" ]; then
    log "  seeding from reference ${ref}"
    rm -rf "${dst}"
    install -d -m 0755 "$(dirname "${dst}")"
    rsync -a --delete \
      --exclude='node_modules' \
      --exclude='target' \
      --exclude='.pnpm-store' \
      --exclude='dist' \
      "${ref}/" "${dst}/"
  fi

  if [ -d "${dst}/.git" ] || [ -f "${dst}/.git" ]; then
    git -C "${dst}" reset --hard HEAD 2>/dev/null || true
    git -C "${dst}" clean -fdx 2>/dev/null || true
  fi

  git -C "${CHECKOUT}" submodule update --init --force --recursive "${path}" || {
    log "  retry with clean checkout for ${path}"
    rm -rf "${dst}"
    git -C "${CHECKOUT}" submodule update --init --force --recursive "${path}" || {
      log "  FAILED: ${path}"
    }
  }
done < <(git -C "${CHECKOUT}" submodule status --recursive | grep '^-' | awk '{print $2}')

uninit="$(git -C "${CHECKOUT}" submodule status --recursive | grep -c '^-' || true)"
total="$(git -C "${CHECKOUT}" submodule status --recursive | wc -l | tr -d ' ')"
expected="$(grep -c '^\[submodule' "${CHECKOUT}/.gitmodules" || echo 0)"

log "submodule status: total=${total} uninit=${uninit} top-level=${expected}"

if [ "${uninit}" -gt 0 ]; then
  log "still uninitialized:"
  git -C "${CHECKOUT}" submodule status --recursive | grep '^-' || true
  exit 1
fi

log "all submodules initialized successfully"
