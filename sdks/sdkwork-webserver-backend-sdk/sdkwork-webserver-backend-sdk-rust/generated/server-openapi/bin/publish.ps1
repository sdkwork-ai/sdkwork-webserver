param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]] $RemainingArgs
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectDir = (Resolve-Path (Join-Path $scriptDir "..")).Path

& node (Join-Path $scriptDir "publish-core.mjs") --language "rust" --project-dir $projectDir @RemainingArgs
exit $LASTEXITCODE
