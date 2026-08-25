$ErrorActionPreference = "Stop"
$hostsFile = "$env:SystemRoot\System32\drivers\etc\hosts"
$mark = "# sdkwork-webserver-docker-wsl"
$wslDistro = "Ubuntu-22.04"
# sdkwork-webserver Docker publishes host :80 / :443 for the public data plane.
# WSL mirrors those ports to Windows localhost — no portproxy to a side port.
$discoverScript = "/mnt/e/sdkwork-space/sdkwork-webserver/deployments/docker/scripts/discover-module-hosts.sh"

$coreDomains = @(
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

$discovered = New-Object System.Collections.Generic.List[string]
foreach ($envName in @("development", "test", "production")) {
    $raw = wsl -d $wslDistro -e bash -lc "bash '$discoverScript' $envName" 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($raw)) { continue }
    foreach ($line in ($raw -split "[\r\n]+")) {
        $hostName = $line.Trim()
        if ($hostName.Length -gt 0) { [void]$discovered.Add($hostName) }
    }
}

$domains = @($coreDomains + $discovered.ToArray() | Select-Object -Unique | Sort-Object)
if ($domains.Count -lt 20) {
    throw "discovered too few domains ($($domains.Count)); aborting hosts rewrite"
}

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
        if ($line -match "\s$([regex]::Escape($d))(\s|$)") { continue 2 }
    }
    $filtered.Add($line)
}
$filtered.Add("")
$filtered.Add($mark)
foreach ($d in $domains) {
    $filtered.Add("127.0.0.1 $d")
}
Set-Content -Path $hostsFile -Value $filtered -Encoding ascii
Write-Host "Updated Windows hosts ($($domains.Count) sdkwork-webserver/module domains)"

# Clear stale portproxy (previously 80 -> 8088/13808). Docker now owns :80/:443.
$iphlp = Get-Service -Name iphlpsvc -ErrorAction SilentlyContinue
if ($iphlp -and $iphlp.Status -ne "Running") {
    Start-Service iphlpsvc
    Write-Host "Started IP Helper"
}
netsh interface portproxy reset | Out-Null
Write-Host "Cleared Windows portproxy (sdkwork-webserver Docker publishes :80 / :443 via WSL)"
Write-Host "Public domains: http://api-dev.sdkwork.com/  https://api-dev.sdkwork.com/"
Write-Host "Management console ports: :13800 / :18888 / :18080"

$ruleName = "SDKWork Web Server HTTP"
if (-not (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -LocalPort 80,443 -Protocol TCP -Action Allow -Profile Any | Out-Null
}
Write-Host "Done."
