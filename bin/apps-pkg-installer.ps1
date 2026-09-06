<#
.SYNOPSIS
  apps-pkg-installer.ps1 — native OS installer packaging on Windows
  (MODULE_BIN_SPEC.md §4.9, Windows companion of bin/apps-pkg-installer.sh).

.DESCRIPTION
  Windows-native twin of bin/apps-pkg-installer.sh: same CLI contract, same
  delegation targets (the repository's canonical installer builders), same
  fail-fast guidance and sha256 sidecar rule. Runs with Windows PowerShell 5.1+
  without bash or a WSL call in the wrapper itself — repository builders that
  need Linux tooling (dpkg-deb) bridge into WSL on their own (documented per
  builder), so the operator only needs node on PATH.

  CLI (mirrors bin/apps-pkg-installer.sh):
    bin/apps-pkg-installer.ps1 <app-type> <platform> <environment>[:<profile>]
        [-Arch x64|arm64] [-Format <fmt>] [-Out <dir>] [-DryRun]

.EXAMPLE
  bin/apps-pkg-installer.ps1 server linux test
  bin/apps-pkg-installer.ps1 server linux production -Format rpm -Arch arm64
  bin/apps-pkg-installer.ps1 server linux test -DryRun
#>
param(
  [Parameter(Position = 0)][string]$AppType = '',
  [Parameter(Position = 1)][string]$Platform = '',
  [Parameter(Position = 2)][string]$Environment = 'development',
  [string]$Arch = 'x64',
  [string]$Format = '',
  [string]$Out = '',
  [switch]$DryRun,
  [switch]$Help
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

$ModuleRoot = Split-Path -Parent $PSScriptRoot
$ModuleId = 'sdkwork-webserver'
$AppTypes = @('pc', 'h5', 'server')
$Platforms = @('windows', 'linux', 'macos', 'android', 'ios')
$Archs = @('x64', 'arm64')

function Write-Plan([string]$Message) { Write-Host "[sdkwork-bin] $Message" }
function Write-Fail([string]$Code, [string]$Message) {
  Write-Host "[sdkwork-bin] ERROR($Code): $Message"
  exit [int]$Code
}

function Convert-Environment([string]$Raw) {
  switch ($Raw) {
    'dev'                    { return 'development' }
    'prod'                   { return 'production' }
    'development'            { return 'development' }
    'test'                   { return 'test' }
    'staging'                { return 'staging' }
    'demo'                   { return 'demo' }
    'production'             { return 'production' }
    default {
      Write-Fail 66 "unknown environment '$Raw' (use development|test|staging|demo|production)"
    }
  }
}

function Get-NewestArtifact([string]$Dir, [string[]]$Patterns) {
  $best = $null
  foreach ($pattern in $Patterns) {
    $candidate = Get-ChildItem -Path $Dir -Filter $pattern -File -ErrorAction SilentlyContinue |
      Sort-Object LastWriteTimeUtc | Select-Object -Last 1
    if ($null -ne $candidate -and ($null -eq $best -or $candidate.LastWriteTimeUtc -ge $best.LastWriteTimeUtc)) {
      $best = $candidate
    }
  }
  return $best
}

function Write-Sha256Sidecar([string]$File) {
  if ($DryRun) { return }
  $hash = (Get-FileHash -Path $File -Algorithm SHA256).Hash.ToLowerInvariant()
  $name = Split-Path -Leaf $File
  Set-Content -Path "$File.sha256" -Value "$hash  $name" -Encoding Ascii
}

function Copy-InstallerArtifact([string]$SourceDir, [string]$OutDir, [string[]]$Patterns) {
  if ($DryRun) {
    Write-Plan "dry-run: would collect [$($Patterns -join ' ')] from $SourceDir into $OutDir"
    return
  }
  $found = Get-NewestArtifact $SourceDir $Patterns
  if ($null -eq $found) {
    Write-Fail 67 "no packaged artifact matching any of [$($Patterns -join ' ')] in $SourceDir (did the repo packager run?)"
  }
  if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
  $target = Join-Path $OutDir $found.Name
  Copy-Item -Path $found.FullName -Destination $target -Force
  Write-Sha256Sidecar $target
  Write-Plan "collected $target"
}

function Assert-OutArtifacts([string]$OutDir) {
  if ($DryRun) { return }
  $patterns = @('*.tar.gz', '*.deb', '*.rpm', '*.zip', '*.apk', '*.aab', '*.msi', '*.exe', '*.pkg', '*.dmg', '*.AppImage', '*.ipa')
  $count = 0
  foreach ($pattern in $patterns) {
    $files = Get-ChildItem -Path $OutDir -Filter $pattern -File -ErrorAction SilentlyContinue
    foreach ($file in $files) {
      $count++
      if (-not (Test-Path "$($file.FullName).sha256")) {
        Write-Fail 67 "artifact without checksum: $($file.FullName) (packaging must emit a sidecar .sha256)"
      }
    }
  }
  if ($count -eq 0) {
    Write-Fail 67 "packaging produced no artifact in $OutDir (MODULE_BIN_SPEC.md §4.9)"
  }
  Write-Plan "artifacts in ${OutDir}: $count"
}

# ---------------------------------------------------------------------------
# Entry flow: parse -> validate -> gate -> delegate -> evidence
# ---------------------------------------------------------------------------
if ($Help) {
  Get-Content (Join-Path $PSScriptRoot 'apps-pkg-installer.ps1') -TotalCount 22
  exit 0
}
if ([string]::IsNullOrEmpty($AppType) -or [string]::IsNullOrEmpty($Platform)) {
  Write-Host 'usage: bin/apps-pkg-installer.ps1 <app-type> <platform> <environment>[:<profile>] [-Arch x64|arm64] [-Format <fmt>] [-Out <dir>] [-DryRun]'
  Write-Host "app types: $($AppTypes -join ',') | platforms: $($Platforms -join ',')"
  exit 64
}
if ($AppTypes -notcontains $AppType) {
  Write-Fail 66 "app type '$AppType' not declared by $ModuleId (declared: $($AppTypes -join ','))"
}
if ($Platforms -notcontains $Platform) {
  Write-Fail 66 "unknown installer platform '$Platform' (use $($Platforms -join '|'))"
}
if ($Archs -notcontains $Arch) {
  Write-Fail 64 "unknown architecture '$Arch' (use x64|arm64)"
}
$envParts = $Environment -split ':'
$Environment = Convert-Environment $envParts[0]
if ($Out -eq '') { $Out = Join-Path $ModuleRoot 'target\bin-installers' }
if (-not $DryRun -and -not (Test-Path $Out)) { New-Item -ItemType Directory -Path $Out -Force | Out-Null }
Write-Plan "apps-pkg-installer $AppType $Platform $Environment arch=$Arch format=$(if ($Format) { $Format } else { '<module-default>' }) -> $Out"

switch ("$AppType`:$Platform") {
  'server:linux' {
    # Host-native Linux installers cover test|production only; every other
    # environment is delivered by the container bundle (MODULE_BIN_SPEC §4.4).
    if ($Environment -notin @('test', 'production')) {
      Write-Fail 67 "the host-native Linux installers cover test|production only (got '$Environment'); use the container path: bin/docker-image.sh save && bin/docker-deploy.sh install --environment $Environment"
    }
    switch ($Format) {
      '' {
        Write-Plan "node scripts/webserver-deb.mjs package --environment $Environment --architecture $Arch"
        if (-not $DryRun) { node (Join-Path $ModuleRoot 'scripts\webserver-deb.mjs') package --environment $Environment --architecture $Arch }
        Copy-InstallerArtifact (Join-Path $ModuleRoot 'dist\installers') $Out @('*.deb')
      }
      'deb' {
        Write-Plan "node scripts/webserver-deb.mjs package --environment $Environment --architecture $Arch"
        if (-not $DryRun) { node (Join-Path $ModuleRoot 'scripts\webserver-deb.mjs') package --environment $Environment --architecture $Arch }
        Copy-InstallerArtifact (Join-Path $ModuleRoot 'dist\installers') $Out @('*.deb')
      }
      'rpm' {
        Write-Plan "node scripts/webserver-rpm.mjs package --environment $Environment --architecture $Arch"
        if (-not $DryRun) { node (Join-Path $ModuleRoot 'scripts\webserver-rpm.mjs') package --environment $Environment --architecture $Arch }
        Copy-InstallerArtifact (Join-Path $ModuleRoot 'dist\installers') $Out @('*.rpm')
      }
      default {
        Write-Fail 64 "unsupported Linux installer format '$Format' (use deb|rpm)"
      }
    }
  }
  'server:windows' {
    Write-Fail 67 "sdkwork-webserver ships host-native installers for Linux (deb|rpm) only; 'windows' server delivery is the container image channel (bin/docker-image.sh save)"
  }
  'server:macos' {
    Write-Fail 67 "sdkwork-webserver ships host-native installers for Linux (deb|rpm) only; 'macos' server delivery is the container image channel (bin/docker-image.sh save)"
  }
  default {
    Write-Fail 66 "app type '$AppType' has no native installer channel (pc/h5 are static web bundles: bin/apps-package.sh $AppType)"
  }
}

Assert-OutArtifacts $Out
