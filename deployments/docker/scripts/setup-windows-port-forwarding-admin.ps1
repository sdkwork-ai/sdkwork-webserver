$ErrorActionPreference = "Stop"
$hostsFile = "$env:SystemRoot\System32\drivers\etc\hosts"
$mark = "# sdkwork-webserver-docker-wsl"
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

$lines = Get-Content $hostsFile
$filtered = New-Object System.Collections.Generic.List[string]
$skip = $false
foreach ($line in $lines) {
    if ($line -eq $mark) {
        $skip = $true
        continue
    }
    if ($skip -and ($line -match '^\s*$' -or $line -match '^\s*#')) {
        if ($line -match '^\s*$') { $skip = $false }
        if (-not ($line -match '^\s*# sdkwork-webserver')) { continue }
    }
    if ($skip) { continue }
    foreach ($d in $domains) {
        if ($line -match "\s$d(\s|$)") { continue 2 }
    }
    $filtered.Add($line)
}
$filtered.Add("")
$filtered.Add($mark)
foreach ($d in $domains) {
    $filtered.Add("127.0.0.1 $d")
}
Set-Content -Path $hostsFile -Value $filtered -Encoding ascii
Write-Host "Updated Windows hosts ($($domains.Count) sdkwork-webserver domains)"

$wslIP = (wsl -e bash -c "hostname -I | awk '{print `$1}'").Trim()
Write-Host "WSL IP: $wslIP"
netsh interface portproxy reset | Out-Null
netsh interface portproxy add v4tov4 listenport=80 listenaddress=0.0.0.0 connectport=80 connectaddress=$wslIP | Out-Null
netsh interface portproxy add v4tov4 listenport=443 listenaddress=0.0.0.0 connectport=443 connectaddress=$wslIP | Out-Null
Write-Host "Port forwarding: Windows :80/:443 -> ${wslIP}:80/:443"

$ruleName = "SDKWork Web Server HTTP/HTTPS"
if (-not (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -LocalPort 80,443 -Protocol TCP -Action Allow -Profile Any | Out-Null
}
Write-Host "Done."
