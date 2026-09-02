param(
    [string[]]$Languages = @("typescript", "dart", "python", "go", "java", "kotlin", "swift", "csharp", "flutter", "rust", "php", "ruby"),
    [string]$BaseUrl = "http://localhost:3800",
    [string]$SdkVersion = "1.0.0"
)

$ErrorActionPreference = "Stop"

function Resolve-PackageName {
    param([string]$Language)

    switch ($Language) {
        "typescript" { return "@sdkwork/webserver-internal-sdk" }
        "dart" { return "sdkwork_webserver_internal_sdk" }
        "python" { return "sdkwork-webserver-internal-sdk" }
        "go" { return "github.com/sdkwork/sdkwork-webserver-internal-sdk" }
        "java" { return "com.sdkwork:sdkwork-webserver-internal-sdk" }
        "kotlin" { return "com.sdkwork:sdkwork-webserver-internal-sdk" }
        "swift" { return "sdkwork-webserver-internal-sdk" }
        "csharp" { return "SDKWork.WebserverInternalSdk" }
        "flutter" { return "sdkwork_webserver_internal_sdk" }
        "rust" { return "sdkwork-webserver-internal-sdk" }
        "php" { return "sdkwork/webserver-internal-sdk" }
        "ruby" { return "sdkwork-webserver-internal-sdk" }
        default { return "sdkwork-webserver-internal-sdk-$Language" }
    }
}

function Resolve-NamespaceArgs {
    param([string]$Language)

    switch ($Language) {
        "java" { return @("--namespace", "com.sdkwork.webserver.internal.sdk") }
        "kotlin" { return @("--namespace", "com.sdkwork.webserver.internal.sdk") }
        "csharp" { return @("--namespace", "SDKWork.WebserverInternalSdk") }
        "php" { return @("--namespace", "SDKWork\Webserver\InternalSdk") }
        default { return @() }
    }
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$FamilyRoot = (Get-Item $ScriptDir).Parent.FullName
$WebRoot = (Get-Item $FamilyRoot).Parent.Parent.FullName
$WorkspaceRoot = (Get-Item (Join-Path $FamilyRoot "..\..\..")).FullName
$GeneratorPath = Join-Path $WorkspaceRoot "sdkwork-sdk-generator\bin\sdkgen.js"
$InputPath = Join-Path $FamilyRoot "openapi\sdkwork-webserver-internal-api.sdkgen.yaml"
$SdkName = "sdkwork-webserver-internal-sdk"
$ApiPrefix = "/internal/v3/api"
$SupportedLanguages = @("typescript", "dart", "python", "go", "java", "kotlin", "swift", "csharp", "flutter", "rust", "php", "ruby")

if (-not (Test-Path $GeneratorPath)) {
    throw "Canonical SDK generator not found: $GeneratorPath"
}
if (-not (Test-Path $InputPath)) {
    & node (Join-Path $WebRoot "tools\materialize_web_phase1_contracts.mjs")
}
if (-not (Test-Path $InputPath)) {
    throw "OpenAPI sdkgen input not found: $InputPath"
}

foreach ($LanguageValue in $Languages) {
    foreach ($LanguagePart in "$LanguageValue".Split(",")) {
        $Language = $LanguagePart.Trim()
        if ([string]::IsNullOrWhiteSpace($Language)) {
            continue
        }
        if ($Language -notin $SupportedLanguages) {
            throw "Unsupported SDK language: $Language"
        }

        $LanguageWorkspace = Join-Path $FamilyRoot "$SdkName-$Language"
        $OutputPath = Join-Path $LanguageWorkspace "generated\server-openapi"
        $PackageName = Resolve-PackageName $Language
        $NamespaceArgs = Resolve-NamespaceArgs $Language
        $ResolvedLanguageWorkspace = [System.IO.Path]::GetFullPath($LanguageWorkspace)
        $ResolvedOutputPath = [System.IO.Path]::GetFullPath($OutputPath)
        $LanguageWorkspacePrefix = $ResolvedLanguageWorkspace.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar

        if (-not $ResolvedOutputPath.StartsWith($LanguageWorkspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing SDK output outside language workspace: $ResolvedOutputPath"
        }

        Write-Host "Generating $Language SDK at $OutputPath" -ForegroundColor Cyan
        & node $GeneratorPath generate `
            -i $InputPath `
            -o $OutputPath `
            -n $SdkName `
            -t custom `
            -l $Language `
            --fixed-sdk-version $SdkVersion `
            --base-url $BaseUrl `
            --api-prefix $ApiPrefix `
            --package-name $PackageName `
            --standard-profile sdkwork-v3 `
            --sdk-root $FamilyRoot `
            --sdk-name $SdkName `
            --no-sync-published-version `
            @NamespaceArgs

        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
}
