# Setup Windows port forwarding for SDKWork Web Server in WSL2
# Run this script as Administrator in Windows PowerShell

#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

# Get WSL2 IP address
$wslIP = $(wsl -d Ubuntu-22.04 -e bash -c "ip -4 addr show eth1 2>/dev/null | grep -oP '(?<=inet\s)\d+(\.\d+){3}'")

if (-not $wslIP) {
    Write-Error "Could not determine WSL2 IP address"
    exit 1
}

Write-Host "WSL2 IP: $wslIP"

# Remove existing port forwarding rules for our ports
$existingRules = netsh interface portproxy show all | Select-String -Pattern "Listen on ipv4:|0\.0\.0\.0\s+(80|443)\s"
if ($existingRules) {
    Write-Host "Removing existing port forwarding rules..."
    netsh interface portproxy reset | Out-Null
}

# Add port forwarding rules
$ports = @(
    @{ ListenPort = 80; ConnectPort = 80 },
    @{ ListenPort = 443; ConnectPort = 443 }
)

foreach ($port in $ports) {
    netsh interface portproxy add v4tov4 `
        listenport=$($port.ListenPort) `
        listenaddress=0.0.0.0 `
        connectport=$($port.ConnectPort) `
        connectaddress=$wslIP | Out-Null
    Write-Host "Added port forwarding: $($port.ListenPort) -> ${wslIP}:$($port.ConnectPort)"
}

# Add Windows firewall rule
$ruleName = "SDKWork Web Server HTTP/HTTPS"
$existingRule = Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue
if (-not $existingRule) {
    New-NetFirewallRule `
        -DisplayName $ruleName `
        -Direction Inbound `
        -LocalPort 80, 443 `
        -Protocol TCP `
        -Action Allow `
        -Profile Any `
        -Description "Allow HTTP/HTTPS access to SDKWork Web Server in WSL" | Out-Null
    Write-Host "Added firewall rule: $ruleName"
} else {
    Write-Host "Firewall rule already exists: $ruleName"
}

# Update hosts file
$hostsFile = "$env:SystemRoot\System32\drivers\etc\hosts"
$mark = "# sdkwork-webserver-docker-wsl"
$content = Get-Content $hostsFile -Raw

if ($content -notlike "*$mark*") {
    $domains = @(
        "server-dev.sdkwork.com",
        "server-app-dev.sdkwork.com",
        "server-admin-dev.sdkwork.com",
        "server-test.sdkwork.com",
        "server-app-test.sdkwork.com",
        "server-admin-test.sdkwork.com",
        "server.sdkwork.com",
        "server-app.sdkwork.com",
        "server-admin.sdkwork.com",
        "sdkwork.com",
        "app.sdkwork.com"
    )
    Add-Content -Path $hostsFile -Value ""
    Add-Content -Path $hostsFile -Value $mark
    foreach ($d in $domains) {
        Add-Content -Path $hostsFile -Value "127.0.0.1 $d"
    }
    Write-Host "Added host entries to $hostsFile"
} else {
    Write-Host "Host entries already exist in $hostsFile"
}

Write-Host ""
Write-Host "Port forwarding setup complete!"
Write-Host "Access URLs (role host server; sdkwork.com only):"
Write-Host "  Development: http://server-dev.sdkwork.com"
Write-Host "  Test:        http://server-test.sdkwork.com"
Write-Host "  Production:  http://server.sdkwork.com"
Write-Host ""
Write-Host "Declarative web config: deployments/webserver/ (SDKWORK_WEBSERVER_SPEC.md)."
