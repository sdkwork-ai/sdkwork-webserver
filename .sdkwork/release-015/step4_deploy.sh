#!/usr/bin/env bash
# 0.1.5 release — step 4: roll the image out through the bin/ single operator
# channel. Environments are passed as arguments (canary first).
set -uo pipefail
cd /mnt/e/sdkwork-space/sdkwork-webserver
log() { echo "[$(date +%H:%M:%S)] $*"; }

[ "$#" -ge 1 ] || { echo "usage: $0 <env> [env...]" >&2; exit 64; }

overall=0
for e in "$@"; do
  log "=== bin/docker-deploy.sh upgrade --environment ${e} --host wsl --deps external --image-tag 0.1.5 --yes ==="
  bash bin/docker-deploy.sh upgrade --environment "${e}" --host wsl --deps external --image-tag 0.1.5 --yes
  rc=$?
  log "UPGRADE_EXIT env=${e} rc=${rc}"
  if [ "${rc}" -ne 0 ]; then overall=1; fi
done
log "DEPLOY_OVERALL_RC=${overall}"
exit "${overall}"
