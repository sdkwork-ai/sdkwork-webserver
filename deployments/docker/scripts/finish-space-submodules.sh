#!/usr/bin/env bash
# Finish initializing all sdkwork-space submodules under /opt/deploy.
set -euo pipefail

CHECKOUT="${SDKWORK_SPACE_CHECKOUT:-/opt/deploy/sdkwork-space}"
REFERENCE="${SDKWORK_SPACE_REFERENCE:-/mnt/e/sdkwork-space}"
JOBS="${SDKWORK_SPACE_SUBMODULE_JOBS:-8}"
LOG="/tmp/submodule-sync.log"

log() { echo "[finish-space-submodules] $*"; }

if [ ! -d "${CHECKOUT}/.git" ]; then
  log "error: ${CHECKOUT} is not a git checkout"
  exit 1
fi

git config --global url."https://github.com/".insteadOf "git@github.com:" 2>/dev/null || true
git -C "${CHECKOUT}" remote set-url origin https://github.com/sdkwork-ai/sdkwork-space.git

log "checkout=${CHECKOUT}"
log "reference=${REFERENCE}"

# Seed missing top-level submodules from the local reference checkout when available.
if [ -d "${REFERENCE}/.git" ]; then
  while IFS= read -r path; do
    [ -n "${path}" ] || continue
    ref_path="${REFERENCE}/${path}"
    dst_path="${CHECKOUT}/${path}"
    if [ -d "${ref_path}/.git" ] || [ -f "${ref_path}/.git" ]; then
      if [ ! -e "${dst_path}/.git" ] && [ ! -d "${dst_path}" ]; then
        log "seeding ${path} from reference"
        install -d -m 0755 "$(dirname "${dst_path}")"
        rsync -a --delete \
          --exclude='node_modules' \
          --exclude='target' \
          --exclude='.pnpm-store' \
          --exclude='dist' \
          "${ref_path}/" "${dst_path}/"
      fi
    fi
  done < <(git -C "${CHECKOUT}" config --file .gitmodules --get-regexp path | awk '{print $2}')
fi

log "removing stale git lock files"
find "${CHECKOUT}" -name '*.lock' -type f -delete 2>/dev/null || true

log "syncing submodule URLs"
git -C "${CHECKOUT}" submodule sync --recursive

# Seeded/copied trees may carry dirty working trees; reset before checkout.
log "resetting dirty submodules"
while IFS= read -r path; do
  [ -n "${path}" ] || continue
  sub="${CHECKOUT}/${path}"
  if [ -e "${sub}/.git" ]; then
    git -C "${sub}" reset --hard HEAD 2>/dev/null || true
    git -C "${sub}" clean -fdx 2>/dev/null || true
  fi
done < <(git -C "${CHECKOUT}" config --file .gitmodules --get-regexp path | awk '{print $2}')

log "initializing submodules (recursive, jobs=${JOBS})"
if ! git -C "${CHECKOUT}" submodule update --init --recursive --jobs "${JOBS}" --force 2>&1 | tee "${LOG}"; then
  log "first pass had errors; cleaning locks and retrying once"
  find "${CHECKOUT}" -name '*.lock' -type f -delete 2>/dev/null || true
  git -C "${CHECKOUT}" submodule update --init --recursive --jobs "${JOBS}" --force 2>&1 | tee -a "${LOG}" || true
fi

uninit="$(git -C "${CHECKOUT}" submodule status --recursive | grep -c '^-' || true)"
mismatch="$(git -C "${CHECKOUT}" submodule status --recursive | grep -c '^+' || true)"
total="$(git -C "${CHECKOUT}" submodule status --recursive | wc -l | tr -d ' ')"
expected="$(grep -c '^\[submodule' "${CHECKOUT}/.gitmodules" || echo 0)"

log "submodule status: total=${total} uninit=${uninit} mismatch=${mismatch} top-level=${expected}"

if [ "${uninit}" -gt 0 ]; then
  log "warning: ${uninit} submodule(s) still uninitialized:"
  git -C "${CHECKOUT}" submodule status --recursive | grep '^-' || true
  exit 1
fi

if [ "${mismatch}" -gt 0 ]; then
  log "warning: ${mismatch} submodule(s) at non-pinned commits (may be acceptable for deploy):"
  git -C "${CHECKOUT}" submodule status --recursive | grep '^+' | head -20 || true
fi

log "all submodules initialized successfully"
exit 0
