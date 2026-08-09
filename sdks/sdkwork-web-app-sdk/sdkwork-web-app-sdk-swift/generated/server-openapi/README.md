# sdkwork-web-app-sdk (Swift)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/sdkwork/sdkwork-web-app-sdk", from: "1.0.0")
]
```

## Quick Start

```swift
import AppSDK
import SDKworkCommon

let config = SdkConfig(baseUrl: "http://localhost:3800")
let client = SdkworkAppClient(config: config)
client.setAuthToken("your-auth-token")
client.setAccessToken("your-access-token")

// Use the SDK
let params: [String: Any] = [
    "page": 1,
    "page_size": 2
]
let result = try await client.domain.domainsList(params: params)
print(result)
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```swift
let config = SdkConfig(baseUrl: "http://localhost:3800")
let client = SdkworkAppClient(config: config)

// Set custom headers
client.setHeader("X-Custom-Header", value: "value")
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

```swift
// 获取应用列表
let params: [String: Any] = [
    "page": 1,
    "page_size": 2,
    "status": 0,
    "application_type": "WEB",
    "site_type": 1,
    "keyword": "keyword"
]
let result = try await client.application.applicationsList(params: params)
print(result)
```

### domain

```swift
// 获取证书可签发域名列表
let params: [String: Any] = [
    "page": 1,
    "page_size": 2
]
let result = try await client.domain.domainsList(params: params)
print(result)
```

### certificate

```swift
// List certificates active on the domain listener
let applicationId = "1"
let domainId = "1"
let params: [String: Any] = [
    "page": 1,
    "page_size": 2
]
let result = try await client.certificate.applicationsDomainsListenerCertificateBindingsList(applicationId: applicationId, domainId: domainId, params: params)
print(result)
```

### source_version

```swift
// 获取应用源码版本
let applicationId = "1"
let params: [String: Any] = [
    "page_size": 1,
    "cursor": "cursor"
]
let result = try await client.sourceVersion.applicationsSourceVersionsList(applicationId: applicationId, params: params)
print(result)
```

### deployment

```swift
// 获取部署历史
let applicationId = "1"
let params: [String: Any] = [
    "page_size": 1,
    "cursor": "cursor",
    "status": 0
]
let result = try await client.deployment.applicationsDeploymentsList(applicationId: applicationId, params: params)
print(result)
```

### env_variable

```swift
// 获取环境变量列表
let applicationId = "1"
let params: [String: Any] = [
    "environment": "environment"
]
let result = try await client.envVariable.applicationsEnvVariablesList(applicationId: applicationId, params: params)
print(result)
```

### monitor

```swift
// 获取健康检查配置
let applicationId = "1"
let result = try await client.monitor.applicationsHealthChecksList(applicationId: applicationId)
print(result)
```

## Error Handling

```swift
do {
    let params: [String: Any] = [
        "page": 1,
        "page_size": 2
    ]
    try await client.domain.domainsList(params: params)
} catch {
    print("Error: \(error)")
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

> Set `SWIFT_RELEASE_TAG` (or `SDKWORK_RELEASE_TAG`) for tag-based release.

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
