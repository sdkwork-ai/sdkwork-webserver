# Rebuild frontend + release + Docker image, then redeploy development, test, and production.
# On Windows, delegates to WSL bash (release packaging requires Linux file modes).
param(
    [switch]$DeployOnly,
    [switch]$SkipFrontendBuild,
    [switch]$SkipReleaseBuild,
    [switch]$SkipImageBuild,
    [switch]$NoValidate,
    [switch]$Pull
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path

function Resolve-WslRepoPath {
    param([string]$WindowsPath)
    $drive = $WindowsPath.Substring(0, 1).ToLowerInvariant()
    $rest = $WindowsPath.Substring(2).Replace("\", "/")
    return "/mnt/$drive$rest"
}

$wslRepo = Resolve-WslRepoPath $repoRoot
$argsList = @()
if ($DeployOnly) { $argsList += "--deploy-only" }
if ($SkipFrontendBuild) { $argsList += "--skip-frontend-build" }
if ($SkipReleaseBuild) { $argsList += "--skip-release-build" }
if ($SkipImageBuild) { $argsList += "--skip-image-build" }
if ($NoValidate) { $argsList += "--no-validate" }
if ($Pull) { $argsList += "--pull" }

$bashArgs = ($argsList | ForEach-Object { $_ }) -join " "
$command = "export SDKWORK_RELEASE_STAGE_PARENT=`${SDKWORK_RELEASE_STAGE_PARENT:-/tmp/sdkwork-release-stage}; cd '$wslRepo' && bash scripts/docker/redeploy-all-environments.sh $bashArgs"

Write-Host "[redeploy-all:windows] $command"
wsl -e bash -lc $command
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
