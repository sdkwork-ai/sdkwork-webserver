# sdkwork-webserver-backend-sdk (Go)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
go get github.com/sdkwork/sdkwork-webserver-backend-sdk
```

## Quick Start

```go
package main

import (
    "fmt"
    "github.com/sdkwork/sdkwork-webserver-backend-sdk"
    sdkhttp "github.com/sdkwork/sdkwork-webserver-backend-sdk/http"

)

func main() {
    cfg := sdkhttp.NewDefaultConfig("http://localhost:3800")
    client := github.com/sdkwork/sdkwork-webserver-backend-sdk.NewSdkworkBackendClientWithConfig(cfg)
    client.SetAuthToken("your-auth-token")
client.SetAccessToken("your-access-token")
    
    // Use the SDK
    result, err := client.Nginx.StatusRetrieve()
    if err != nil {
        panic(err)
    }
    fmt.Println(result)
}
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```go
cfg := sdkhttp.NewDefaultConfig("http://localhost:3800")
client := github.com/sdkwork/sdkwork-webserver-backend-sdk.NewSdkworkBackendClientWithConfig(cfg)

// Set custom headers
client.SetHeader("X-Custom-Header", "value")
```

## API Modules

- `client.Application` - application API
- `client.ApplicationDomain` - application_domain API
- `client.Certificate` - certificate API
- `client.Domain` - domain API
- `client.ApplicationSourceVersion` - application_source_version API
- `client.ApplicationDeployment` - application_deployment API
- `client.CertificateDistribution` - certificate_distribution API
- `client.Nginx` - nginx API
- `client.Server` - server API
- `client.ServerFile` - server_file API
- `client.Agent` - agent API
- `client.Audit` - audit API

## Usage Examples

### application

```go
// List managed applications
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
    "application_type": "WEB",
    "site_type": 4,
    "status": 5,
    "keyword": "keyword",
}
result, err := client.Application.ApplicationsList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### application_domain

```go
// List application domains
applicationId := "1"
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
}
result, err := client.ApplicationDomain.ApplicationsDomainsList(applicationId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### certificate

```go
// List canonical certificates
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
    "domain_id": "domain_id",
}
result, err := client.Certificate.CertificatesList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### domain

```go
// List tenant custom domain assets
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
}
result, err := client.Domain.DomainsList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### application_source_version

```go
// List immutable application source versions
applicationId := "1"
params := map[string]interface{}{
    "page_size": 1,
    "cursor": "cursor",
}
result, err := client.ApplicationSourceVersion.ApplicationsSourceVersionsList(applicationId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### application_deployment

```go
// List application deployments
applicationId := "1"
params := map[string]interface{}{
    "page_size": 1,
    "cursor": "cursor",
    "status": 3,
}
result, err := client.ApplicationDeployment.ApplicationsDeploymentsList(applicationId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### certificate_distribution

```go
// List certificate manifest convergence by server
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
}
result, err := client.CertificateDistribution.CertificatesDistributionList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### nginx

```go
// Retrieve Nginx status
result, err := client.Nginx.StatusRetrieve()
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### server

```go
// List managed servers
params := map[string]interface{}{
    "page_size": 1,
    "cursor": "cursor",
}
result, err := client.Server.ServersList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### server_file

```go
// List Server Files deployment nodes
result, err := client.ServerFile.ServerFilesNodesList()
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### agent

```go
// Retrieve the Nginx configuration and certificate bundle
params := map[string]interface{}{
    "if_sync_version": "if_sync_version",
}
result, err := client.Agent.Retrieve(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### audit

```go
// List audit logs
params := map[string]interface{}{
    "page_size": 1,
    "cursor": "cursor",
    "target_type": "target_type",
    "action": "action",
    "operator_id": "operator_id",
    "start_date": "start_date",
    "end_date": "end_date",
}
result, err := client.Audit.LogsList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

## Error Handling

```go
_, err := client.Nginx.StatusRetrieve()
if err != nil {
    // Handle error
    fmt.Println("Error:", err)
    return
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

> Set `GO_RELEASE_TAG` (or `SDKWORK_RELEASE_TAG`) and push tag if needed.

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
