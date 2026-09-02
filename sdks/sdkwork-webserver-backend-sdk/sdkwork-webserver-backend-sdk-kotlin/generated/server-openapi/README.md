# sdkwork-webserver-backend-sdk (Kotlin)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to your `build.gradle.kts`:

```kotlin
implementation("com.sdkwork:sdkwork-webserver-backend-sdk:1.0.0")
```

Or with Gradle Groovy:

```groovy
implementation 'com.sdkwork:sdkwork-webserver-backend-sdk:1.0.0'
```

## Quick Start

```kotlin
import com.sdkwork.webserver.backend.sdk.SdkworkBackendClient
import com.sdkwork.webserver.backend.sdk.*
import com.sdkwork.common.core.SdkConfig
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    val config = SdkConfig(baseUrl = "http://localhost:3800")
    val client = SdkworkBackendClient(config)
    client.setAuthToken("your-auth-token")
client.setAccessToken("your-access-token")

    // Use the SDK
    val result = client.nginx.statusRetrieve()
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
val client = SdkworkBackendClient(config)
```

## API Modules

- `client.application` - application API
- `client.applicationDomain` - application_domain API
- `client.certificate` - certificate API
- `client.domain` - domain API
- `client.applicationSourceVersion` - application_source_version API
- `client.applicationDeployment` - application_deployment API
- `client.certificateDistribution` - certificate_distribution API
- `client.nginx` - nginx API
- `client.server` - server API
- `client.serverFile` - server_file API
- `client.agent` - agent API
- `client.audit` - audit API

## Usage Examples

### application

```kotlin
// List managed applications
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2,
    "application_type" to "WEB",
    "site_type" to 4,
    "status" to 5,
    "keyword" to "keyword"
)
val result = client.application.applicationsList(params)
println(result)
```

### application_domain

```kotlin
// List application domains
val applicationId = "1"
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2
)
val result = client.applicationDomain.applicationsDomainsList(applicationId, params)
println(result)
```

### certificate

```kotlin
// List canonical certificates
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2,
    "domain_id" to "00000000-0000-0000-0000-000000000001"
)
val result = client.certificate.certificatesList(params)
println(result)
```

### domain

```kotlin
// List tenant custom domain assets
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2
)
val result = client.domain.domainsList(params)
println(result)
```

### application_source_version

```kotlin
// List immutable application source versions
val applicationId = "1"
val params = linkedMapOf<String, Any>(
    "page_size" to 1,
    "cursor" to "cursor"
)
val result = client.applicationSourceVersion.applicationsSourceVersionsList(applicationId, params)
println(result)
```

### application_deployment

```kotlin
// List application deployments
val applicationId = "1"
val params = linkedMapOf<String, Any>(
    "page_size" to 1,
    "cursor" to "cursor",
    "status" to 3
)
val result = client.applicationDeployment.applicationsDeploymentsList(applicationId, params)
println(result)
```

### certificate_distribution

```kotlin
// List certificate manifest convergence by server
val params = linkedMapOf<String, Any>(
    "page" to 1,
    "page_size" to 2
)
val result = client.certificateDistribution.certificatesDistributionList(params)
println(result)
```

### nginx

```kotlin
// Retrieve Nginx status
val result = client.nginx.statusRetrieve()
println(result)
```

### server

```kotlin
// List managed servers
val params = linkedMapOf<String, Any>(
    "page_size" to 1,
    "cursor" to "cursor"
)
val result = client.server.serversList(params)
println(result)
```

### server_file

```kotlin
// List Server Files deployment nodes
val result = client.serverFile.serverFilesNodesList()
println(result)
```

### agent

```kotlin
// Retrieve the Nginx configuration and certificate bundle
val params = linkedMapOf<String, Any>(
    "if_sync_version" to "if-sync-version"
)
val result = client.agent.retrieve(params)
println(result)
```

### audit

```kotlin
// List audit logs
val params = linkedMapOf<String, Any>(
    "page_size" to 1,
    "cursor" to "cursor",
    "target_type" to "target-type",
    "action" to "action",
    "operator_id" to "1",
    "start_date" to "2026-04-10T00:00:00Z",
    "end_date" to "2026-04-10T00:00:00Z"
)
val result = client.audit.logsList(params)
println(result)
```

## Error Handling

```kotlin
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    try {
        val result = client.nginx.statusRetrieve()
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
