# sdkwork-web-app-sdk (Go)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
go get github.com/sdkwork/sdkwork-web-app-sdk
```

## Quick Start

```go
package main

import (
    "fmt"
    "github.com/sdkwork/sdkwork-web-app-sdk"
    sdkhttp "github.com/sdkwork/sdkwork-web-app-sdk/http"

)

func main() {
    cfg := sdkhttp.NewDefaultConfig("http://localhost:3800")
    client := github.com/sdkwork/sdkwork-web-app-sdk.NewSdkworkAppClientWithConfig(cfg)
    client.SetAuthToken("your-auth-token")
client.SetAccessToken("your-access-token")
    
    // Use the SDK
    params := map[string]interface{}{
        "page": 1,
        "page_size": 2,
    }
    result, err := client.Domain.DomainsList(params)
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
client := github.com/sdkwork/sdkwork-web-app-sdk.NewSdkworkAppClientWithConfig(cfg)

// Set custom headers
client.SetHeader("X-Custom-Header", "value")
```

## API Modules

- `client.Application` - application API
- `client.Domain` - domain API
- `client.Certificate` - certificate API
- `client.SourceVersion` - source_version API
- `client.Deployment` - deployment API
- `client.EnvVariable` - env_variable API
- `client.Monitor` - monitor API

## Usage Examples

### application

```go
// 获取应用列表
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
    "status": 0,
    "application_type": "WEB",
    "site_type": 1,
    "keyword": "keyword",
}
result, err := client.Application.ApplicationsList(params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### domain

```go
// 获取证书可签发域名列表
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

### certificate

```go
// List certificates active on the domain listener
applicationId := "1"
domainId := "1"
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
}
result, err := client.Certificate.ApplicationsDomainsListenerCertificateBindingsList(applicationId, domainId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### source_version

```go
// 获取应用源码版本
applicationId := "1"
params := map[string]interface{}{
    "page_size": 1,
    "cursor": "cursor",
}
result, err := client.SourceVersion.ApplicationsSourceVersionsList(applicationId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### deployment

```go
// 获取部署历史
applicationId := "1"
params := map[string]interface{}{
    "page_size": 1,
    "cursor": "cursor",
    "status": 0,
}
result, err := client.Deployment.ApplicationsDeploymentsList(applicationId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### env_variable

```go
// 获取环境变量列表
applicationId := "1"
params := map[string]interface{}{
    "environment": "environment",
}
result, err := client.EnvVariable.ApplicationsEnvVariablesList(applicationId, params)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

### monitor

```go
// 获取健康检查配置
applicationId := "1"
result, err := client.Monitor.ApplicationsHealthChecksList(applicationId)
if err != nil {
    panic(err)
}
fmt.Println(result)
```

## Error Handling

```go
params := map[string]interface{}{
    "page": 1,
    "page_size": 2,
}
_, err := client.Domain.DomainsList(params)
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
