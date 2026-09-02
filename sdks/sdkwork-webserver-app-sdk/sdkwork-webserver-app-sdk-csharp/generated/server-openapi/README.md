# sdkwork-webserver-app-sdk (C#)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
dotnet add package SDKWork.Webserver.AppSdk
```

Or add to your `.csproj`:

```xml
<PackageReference Include="SDKWork.Webserver.AppSdk" Version="1.0.0" />
```

## Quick Start

```csharp
using System.Collections.Generic;
using SDKWork.Webserver.AppSdk.Models;
using SDKWork.Webserver.AppSdk;
using SDKwork.Common.Core;

var config = new SdkConfig("http://localhost:3800");
var client = new SdkworkAppClient(config);
client.SetAuthToken("your-auth-token");
client.SetAccessToken("your-access-token");

var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
};
var result = await client.Domain.DomainsListAsync(query);
Console.WriteLine(result);
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```csharp
var config = new SdkConfig("http://localhost:3800");
var client = new SdkworkAppClient(config);

// Set custom headers
client.SetHeader("X-Custom-Header", "value");
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

```csharp
// 获取应用列表
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
    ["status"] = 0,
    ["application_type"] = "WEB",
    ["site_type"] = 1,
    ["keyword"] = "keyword",
};
var result = await client.Application.ApplicationsListAsync(query);
Console.WriteLine(result);
```

### domain

```csharp
// 获取证书可签发域名列表
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
};
var result = await client.Domain.DomainsListAsync(query);
Console.WriteLine(result);
```

### certificate

```csharp
// List certificates active on the domain listener
var applicationId = "1";
var domainId = "1";
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
};
var result = await client.Certificate.ApplicationsDomainsListenerCertificateBindingsListAsync(applicationId, domainId, query);
Console.WriteLine(result);
```

### source_version

```csharp
// 获取应用源码版本
var applicationId = "1";
var query = new Dictionary<string, object>
{
    ["page_size"] = 1,
    ["cursor"] = "cursor",
};
var result = await client.SourceVersion.ApplicationsSourceVersionsListAsync(applicationId, query);
Console.WriteLine(result);
```

### deployment

```csharp
// 获取部署历史
var applicationId = "1";
var query = new Dictionary<string, object>
{
    ["page_size"] = 1,
    ["cursor"] = "cursor",
    ["status"] = 0,
};
var result = await client.Deployment.ApplicationsDeploymentsListAsync(applicationId, query);
Console.WriteLine(result);
```

### env_variable

```csharp
// 获取环境变量列表
var applicationId = "1";
var query = new Dictionary<string, object>
{
    ["environment"] = "environment",
};
var result = await client.EnvVariable.ApplicationsEnvVariablesListAsync(applicationId, query);
Console.WriteLine(result);
```

### monitor

```csharp
// 获取健康检查配置
var applicationId = "1";
var result = await client.Monitor.ApplicationsHealthChecksListAsync(applicationId);
Console.WriteLine(result);
```

## Error Handling

```csharp
try
{
    var query = new Dictionary<string, object>
    {
        ["page"] = 1,
        ["page_size"] = 2,
    };
    await client.Domain.DomainsListAsync(query);
}
catch (HttpRequestException ex)
{
    Console.WriteLine($"Error: {ex.Message}");
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

> Configure NuGet registry credentials before release publish.

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
