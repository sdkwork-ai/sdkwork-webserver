# Host nginx is retired for SDKWork Web Server Docker deployments.
#
# Public reverse proxy for module domains and the platform API plane
# (`api-dev.*`, `api.*`, …) is owned by `sdkwork-webserver` `serve-imports`
# (SDKWORK_WEBSERVER_SPEC.md §17). Declarative module source of truth remains
# `deployments/webserver/` under each sibling module checkout.
#
# Uninstall any previously installed WSL nginx:
#   sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh
#
# Do not reintroduce hand-authored dual-authority conf files here.
