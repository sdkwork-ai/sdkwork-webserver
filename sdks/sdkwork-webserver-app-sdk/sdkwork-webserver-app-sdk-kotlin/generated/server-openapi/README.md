# sdkwork-webserver-app-sdk (Kotlin)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to your `build.gradle.kts`:

```kotlin
implementation("com.sdkwork:sdkwork-webserver-app-sdk:1.0.0")
```

Or with Gradle Groovy:

```groovy
implementation 'com.sdkwork:sdkwork-webserver-app-sdk:1.0.0'
```

## Quick Start

```kotlin
import com.sdkwork.webserver.app.sdk.SdkworkAppClient
import com.sdkwork.webserver.app.sdk.*
import com.sdkwork.common.core.SdkConfig
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    val config = SdkConfig(baseUrl = "http://localhost:3800")
    val client = SdkworkAppClient(config)
    client.setAuthToken("your-auth-token")
client.setAccessToken("your-access-token")

    // Use the SDK
    val params = linkedMapOf<String, Any>(
        "page" to 1,
        "page_size" to 2
    )
    val result = client.domain.domainsList(params)
    println(result)
}
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```kotlin
val config = SdkConfig(baseUrl = "http://localhost:3800")
val client = SdkworkAppClient(config)
```

## API Modules

- `client.application` - application API
- `client.domain` - domain API
- `client.certificate` - certificate API
- `client.sourceVersion` - source_version API
- `client.deployment` - deployment API
- `client.envVariable` - env_variable API
- `client.monitor` - monitor API

## Usage Examples

### application

```kotlin
// 获取应用列表
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2,
    "status" to 0,
    "application_type" to "WEB",
    "site_type" to 1,
    "keyword" to "keyword"
)
val result = client.application.applicationsList(params)
println(result)
```

### domain

```kotlin
// 获取证书可签发域名列表
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2
)
val result = client.domain.domainsList(params)
println(result)
```

### certificate

```kotlin
// List certificates active on the domain listener
val applicationId = "1"
val domainId = "1"
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2
)
val result = client.certificate.applicationsDomainsListenerCertificateBindingsList(applicationId, domainId, params)
println(result)
```

### source_version

```kotlin
// 获取应用源码版本
val applicationId = "1"
val params = linkedMapOf<String, Any>(
    "page_size" to 1,
    "cursor" to "cursor"
)
val result = client.sourceVersion.applicationsSourceVersionsList(applicationId, params)
println(result)
```

### deployment

```kotlin
// 获取部署历史
val applicationId = "1"
val params = linkedMapOf<String, Any>(
    "page_size" to 1,
    "cursor" to "cursor",
    "status" to 0
)
val result = client.deployment.applicationsDeploymentsList(applicationId, params)
println(result)
```

### env_variable

```kotlin
// 获取环境变量列表
val applicationId = "1"
val params = linkedMapOf<String, Any>(
    "environment" to "environment"
)
val result = client.envVariable.applicationsEnvVariablesList(applicationId, params)
println(result)
```

### monitor

```kotlin
// 获取健康检查配置
val applicationId = "1"
val result = client.monitor.applicationsHealthChecksList(applicationId)
println(result)
```

## Error Handling

```kotlin
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    try {
        val params = linkedMapOf<String, Any>(
            "page" to 1,
            "page_size" to 2
        )
        val result = client.domain.domainsList(params)
        println(result)
    } catch (e: Exception) {
        println("Error: ${e.message}")
    }
}
```

## Publishing

This SDK includes cross-platform publish scripts in `bin/`:
- `bin/publish-core.mjs`
- `bin/publish.sh`
- `bin/publish.ps1`

### Check

```bash
./bin/publish.sh --action check
```

### Publish

```bash
./bin/publish.sh --action publish --channel release
```

```powershell
.\bin\publish.ps1 --action publish --channel test --dry-run
```

> Configure Gradle publishing credentials and optional `GRADLE_PUBLISH_TASK`.

## License

MIT

## Regeneration Contract

- HTTP/OpenAPI generator-owned files are tracked in `.sdkwork/sdkwork-generator-manifest.json`.
- HTTP/OpenAPI generation also writes `.sdkwork/sdkwork-generator-changes.json` so automation can inspect created, updated, deleted, unchanged, scaffolded, and backed-up files plus the classified impact areas, verification plan, and execution decision for the latest generation.
- HTTP/OpenAPI apply mode also writes `.sdkwork/sdkwork-generator-report.json` with the full execution report, including `schemaVersion`, `generator`, stable artifact paths, and the execution handoff commands that match CLI `--json` output.
- CLI JSON output also includes an execution handoff with concrete next commands, including reviewed apply commands for dry-run flows.
- Put HTTP/OpenAPI hand-written wrappers, adapters, and orchestration in `custom/`.
- Files scaffolded under `custom/` are created once and preserved across HTTP/OpenAPI regenerations.
- If an HTTP/OpenAPI generated-owned file was modified locally, its previous content is copied to `.sdkwork/manual-backups/` before overwrite or removal.
- RPC SDK source workspaces use convention-first evidence by default: RPC SDK family naming, language workspace naming, `rpc/*.manifest.json`, proto source references, generated client source, and native package manifests.
- Use `sdkgen inspect --protocol rpc` to verify RPC convention evidence. Request persisted generator evidence only with `--emit-control-plane` for release, CI, audit, or migration workflows; evidence paths are derived by generator convention.
