# sdkwork-webserver-backend-sdk (C#)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
dotnet add package SDKWork.Webserver.BackendSdk
```

Or add to your `.csproj`:

```xml
<PackageReference Include="SDKWork.Webserver.BackendSdk" Version="1.0.0" />
```

## Quick Start

```csharp
using SDKWork.Webserver.BackendSdk.Models;
using SDKWork.Webserver.BackendSdk;
using SDKwork.Common.Core;

var config = new SdkConfig("http://localhost:3800");
var client = new SdkworkBackendClient(config);
client.SetAuthToken("your-auth-token");
client.SetAccessToken("your-access-token");

var result = await client.Nginx.StatusRetrieveAsync();
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
var client = new SdkworkBackendClient(config);

// Set custom headers
client.SetHeader("X-Custom-Header", "value");
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

```csharp
// List managed applications
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
    ["application_type"] = "WEB",
    ["site_type"] = 4,
    ["status"] = 5,
    ["keyword"] = "keyword",
};
var result = await client.Application.ApplicationsListAsync(query);
Console.WriteLine(result);
```

### application_domain

```csharp
// List application domains
var applicationId = "1";
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
};
var result = await client.ApplicationDomain.ApplicationsDomainsListAsync(applicationId, query);
Console.WriteLine(result);
```

### certificate

```csharp
// List canonical certificates
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
    ["domain_id"] = "00000000-0000-0000-0000-000000000001",
};
var result = await client.Certificate.CertificatesListAsync(query);
Console.WriteLine(result);
```

### domain

```csharp
// List tenant custom domain assets
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
};
var result = await client.Domain.DomainsListAsync(query);
Console.WriteLine(result);
```

### application_source_version

```csharp
// List immutable application source versions
var applicationId = "1";
var query = new Dictionary<string, object>
{
    ["page_size"] = 1,
    ["cursor"] = "cursor",
};
var result = await client.ApplicationSourceVersion.ApplicationsSourceVersionsListAsync(applicationId, query);
Console.WriteLine(result);
```

### application_deployment

```csharp
// List application deployments
var applicationId = "1";
var query = new Dictionary<string, object>
{
    ["page_size"] = 1,
    ["cursor"] = "cursor",
    ["status"] = 3,
};
var result = await client.ApplicationDeployment.ApplicationsDeploymentsListAsync(applicationId, query);
Console.WriteLine(result);
```

### certificate_distribution

```csharp
// List certificate manifest convergence by server
var query = new Dictionary<string, object>
{
    ["page"] = 1,
    ["page_size"] = 2,
};
var result = await client.CertificateDistribution.CertificatesDistributionListAsync(query);
Console.WriteLine(result);
```

### nginx

```csharp
// Retrieve Nginx status
var result = await client.Nginx.StatusRetrieveAsync();
Console.WriteLine(result);
```

### server

```csharp
// List managed servers
var query = new Dictionary<string, object>
{
    ["page_size"] = 1,
    ["cursor"] = "cursor",
};
var result = await client.Server.ServersListAsync(query);
Console.WriteLine(result);
```

### server_file

```csharp
// List Server Files deployment nodes
var result = await client.ServerFile.ServerFilesNodesListAsync();
Console.WriteLine(result);
```

### agent

```csharp
// Retrieve the Nginx configuration and certificate bundle
var query = new Dictionary<string, object>
{
    ["if_sync_version"] = "if-sync-version",
};
var result = await client.Agent.RetrieveAsync(query);
Console.WriteLine(result);
```

### audit

```csharp
// List audit logs
var query = new Dictionary<string, object>
{
    ["page_size"] = 1,
    ["cursor"] = "cursor",
    ["target_type"] = "target-type",
    ["action"] = "action",
    ["operator_id"] = "1",
    ["start_date"] = "2026-04-10T00:00:00Z",
    ["end_date"] = "2026-04-10T00:00:00Z",
};
var result = await client.Audit.LogsListAsync(query);
Console.WriteLine(result);
```

## Error Handling

```csharp
try
{
    await client.Nginx.StatusRetrieveAsync();
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
