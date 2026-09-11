#!/usr/bin/env bash
# 0.1.5 release — step 2: release archive (if missing) + container image, both
# through the bin/ single operator channel.
set -uo pipefail
cd /mnt/e/sdkwork-space/sdkwork-webserver
log() { echo "[$(date +%H:%M:%S)] $*"; }

# DrvFs (/mnt/e) does not preserve POSIX permission bits; the release packager
# validates them, so stage on ext4. /var/tmp/sdkwork-release-stage is left
# root-owned by earlier root-run builds, hence a fresh per-run directory.
export SDKWORK_RELEASE_STAGE_PARENT=/var/tmp/sdkwork-release-stage-015
mkdir -p "${SDKWORK_RELEASE_STAGE_PARENT}"
if [ ! -w "${SDKWORK_RELEASE_STAGE_PARENT}" ]; then
  echo "stage parent is not writable: ${SDKWORK_RELEASE_STAGE_PARENT}" >&2
  exit 73
fi
# The WSL docker daemon has no proxy and cannot reach any registry: reuse the
# cached base image instead of letting `docker build --pull` die on TLS.
export SDKWORK_IMAGE_NO_PULL=1
# cargo is not on the PATH of a non-login shell.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

ARCHIVE="dist/release/sdkwork-webserver-linux-x64-standalone-server-0.1.5.tar.gz"

log "=== version ==="
node -e "console.log(require('./sdkwork.app.config.json').release.currentVersion)"

if [ -f "${ARCHIVE}" ]; then
  log "release archive already present: ${ARCHIVE}"
else
  log "=== node scripts/webserver-release.mjs package --version 0.1.5 --skip-pc-build --skip-h5-build ==="
  node scripts/webserver-release.mjs package \
    --deployment-profile standalone \
    --architecture x64 \
    --environment production \
    --version 0.1.5 \
    --skip-pc-build \
    --skip-h5-build
  rc=$?
  log "RELEASE_PACKAGE_EXIT=${rc}"
  if [ "${rc}" -ne 0 ]; then exit "${rc}"; fi
fi

log "=== bin/docker-image.sh build --image-tag 0.1.5 ==="
bash bin/docker-image.sh build --image-tag 0.1.5
rc=$?
log "IMAGE_BUILD_EXIT=${rc}"
if [ "${rc}" -eq 0 ]; then
  bash bin/docker-image.sh inspect --image-tag 0.1.5
  log "IMAGE_INSPECT_EXIT=$?"
fi
exit "${rc}"
