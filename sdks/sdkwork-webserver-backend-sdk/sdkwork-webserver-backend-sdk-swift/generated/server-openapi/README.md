# sdkwork-webserver-backend-sdk (Swift)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/sdkwork/sdkwork-webserver-backend-sdk", from: "1.0.0")
]
```

## Quick Start

```swift
import BackendSDK
import SDKworkCommon

let config = SdkConfig(baseUrl: "http://localhost:3800")
let client = SdkworkBackendClient(config: config)
client.setAuthToken("your-auth-token")
client.setAccessToken("your-access-token")

// Use the SDK
let result = try await client.nginx.statusRetrieve()
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
let client = SdkworkBackendClient(config: config)

// Set custom headers
client.setHeader("X-Custom-Header", value: "value")
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

```swift
// List managed applications
let params: [String: Any] = [
    "page": 1,
    "page_size": 2,
    "application_type": "WEB",
    "site_type": 4,
    "status": 5,
    "keyword": "keyword"
]
let result = try await client.application.applicationsList(params: params)
print(result)
```

### application_domain

```swift
// List application domains
let applicationId = "1"
let params: [String: Any] = [
    "page": 1,
    "page_size": 2
]
let result = try await client.applicationDomain.applicationsDomainsList(applicationId: applicationId, params: params)
print(result)
```

### certificate

```swift
// List canonical certificates
let params: [String: Any] = [
    "page": 1,
    "page_size": 2,
    "domain_id": "00000000-0000-0000-0000-000000000001"
]
let result = try await client.certificate.certificatesList(params: params)
print(result)
```

### domain

```swift
// List tenant custom domain assets
let params: [String: Any] = [
    "page": 1,
    "page_size": 2
]
let result = try await client.domain.domainsList(params: params)
print(result)
```

### application_source_version

```swift
// List immutable application source versions
let applicationId = "1"
let params: [String: Any] = [
    "page_size": 1,
    "cursor": "cursor"
]
let result = try await client.applicationSourceVersion.applicationsSourceVersionsList(applicationId: applicationId, params: params)
print(result)
```

### application_deployment

```swift
// List application deployments
let applicationId = "1"
let params: [String: Any] = [
    "page_size": 1,
    "cursor": "cursor",
    "status": 3
]
let result = try await client.applicationDeployment.applicationsDeploymentsList(applicationId: applicationId, params: params)
print(result)
```

### certificate_distribution

```swift
// List certificate manifest convergence by server
let params: [String: Any] = [
    "page": 1,
    "page_size": 2
]
let result = try await client.certificateDistribution.certificatesDistributionList(params: params)
print(result)
```

### nginx

```swift
// Retrieve Nginx status
let result = try await client.nginx.statusRetrieve()
print(result)
```

### server

```swift
// List managed servers
let params: [String: Any] = [
    "page_size": 1,
    "cursor": "cursor"
]
let result = try await client.server.serversList(params: params)
print(result)
```

### server_file

```swift
// List Server Files deployment nodes
let result = try await client.serverFile.serverFilesNodesList()
print(result)
```

### agent

```swift
// Retrieve the Nginx configuration and certificate bundle
let params: [String: Any] = [
    "if_sync_version": "if-sync-version"
]
let result = try await client.agent.retrieve(params: params)
print(result)
```

### audit

```swift
// List audit logs
let params: [String: Any] = [
    "page_size": 1,
    "cursor": "cursor",
    "target_type": "target-type",
    "action": "action",
    "operator_id": "1",
    "start_date": "2026-04-10T00:00:00Z",
    "end_date": "2026-04-10T00:00:00Z"
]
let result = try await client.audit.logsList(params: params)
print(result)
```

## Error Handling

```swift
do {
    try await client.nginx.statusRetrieve()
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
