#!/usr/bin/env bash
# RETIRED: host nginx domain proxy is no longer used.
#
# sdkwork-webserver Docker serve-imports owns module + platform API reverse
# proxy (api*.brand, im-*.brand, …). This script uninstalls nginx instead of
# installing it.
#
# Usage:
#   sudo bash deployments/docker/scripts/install-wsl-nginx.sh
#
# Prefer the explicit uninstall entrypoint:
#   sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[install-wsl-nginx] RETIRED: host nginx is replaced by sdkwork-webserver Docker" >&2
echo "[install-wsl-nginx] running uninstall-wsl-nginx.sh instead" >&2
exec bash "${SCRIPT_DIR}/uninstall-wsl-nginx.sh"
