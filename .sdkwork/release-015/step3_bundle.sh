#!/usr/bin/env bash
# 0.1.5 release — step 3: package the self-contained stage-2 install bundle
# (image.tar.gz + sha256 + deploy.sh + release.sh + compose + env x5 +
# postgres assets + manifest.json) into dist/docker-install/.
set -uo pipefail
cd /mnt/e/sdkwork-space/sdkwork-webserver
log() { echo "[$(date +%H:%M:%S)] $*"; }

export SDKWORK_DOCKER_NO_PULL=1

log "=== node scripts/docker/package-install-bundle.mjs --tag 0.1.5 --skip-image-build ==="
node scripts/docker/package-install-bundle.mjs --tag 0.1.5 --skip-image-build
rc=$?
log "BUNDLE_PACKAGE_EXIT=${rc}"
if [ "${rc}" -ne 0 ]; then exit "${rc}"; fi

BUNDLE="dist/docker-install/sdkwork-webserver-install-0.1.5.bundle"
log "=== bundle inventory: ${BUNDLE} ==="
ls -la "${BUNDLE}/" "${BUNDLE}/env/"
log "=== manifest ==="
cat "${BUNDLE}/manifest.json"
log "=== image.sha256 ==="
cat "${BUNDLE}/image.sha256"
log "=== CRLF gate on executed bundle scripts ==="
for f in "${BUNDLE}/deploy.sh" "${BUNDLE}/release.sh"; do
  printf '%-52s CR=%s\n' "$f" "$(tr -cd '\r' < "$f" | wc -c)"
done
log "BUNDLE_READY"
