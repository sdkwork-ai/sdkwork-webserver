# Windows localhost access for SDKWork Web Server (Docker Desktop / WSL2).
#
# sdkwork-webserver owns public :80 / :443. Do NOT install stock nginx and do
# NOT portproxy :80/:443 to a side listener. Prefer the admin script when
# rewriting hosts + clearing stale portproxy:
#   setup-windows-port-forwarding-admin.ps1
#
# This non-admin helper only documents the contract and refreshes hosts when
# run elevated via the admin script path.

$ErrorActionPreference = "Stop"

Write-Host "sdkwork-webserver is the public edge (nginx-compatible; stock nginx retired)."
Write-Host "Docker publishes host :80 / :443 from sdkwork-webserver-development."
Write-Host ""
Write-Host "If you need hosts + portproxy cleanup, run as Administrator:"
Write-Host "  powershell -ExecutionPolicy Bypass -File deployments/docker/scripts/setup-windows-port-forwarding-admin.ps1"
Write-Host ""
Write-Host "Verify (no proxy):"
Write-Host "  curl --noproxy '*' -H 'Host: api-dev.birdcoder.cn' http://127.0.0.1/healthz"
Write-Host "  curl --noproxy '*' http://api-dev.birdcoder.cn/healthz"
Write-Host ""
Write-Host "Declarative web config: deployments/webserver/ (SDKWORK_WEBSERVER_SPEC.md §0.1, NGINX_SPEC.md §0)."
