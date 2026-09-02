# sdkwork-webserver-backend-sdk (Java)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to your `pom.xml`:

```xml
<dependency>
    <groupId>com.sdkwork</groupId>
    <artifactId>sdkwork-webserver-backend-sdk</artifactId>
    <version>1.0.0</version>
</dependency>
```

Or with Gradle:

```groovy
implementation 'com.sdkwork:sdkwork-webserver-backend-sdk:1.0.0'
```

## Quick Start

```java
import com.sdkwork.webserver.backend.sdk.SdkworkBackendClient;
import com.sdkwork.common.core.Types;
import com.sdkwork.webserver.backend.sdk.model.*;

public class Main {
    public static void main(String[] args) throws Exception {
        Types.SdkConfig config = new Types.SdkConfig("http://localhost:3800");
        SdkworkBackendClient client = new SdkworkBackendClient(config);
        client.setAuthToken("your-auth-token");
client.setAccessToken("your-access-token");

        // Use the SDK
        StatusRetrieveResponse result = client.getNginx().statusRetrieve();
        System.out.println(result);
    }
}
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```java
Types.SdkConfig config = new Types.SdkConfig("http://localhost:3800");
SdkworkBackendClient client = new SdkworkBackendClient(config);

// Set custom headers
client.getHttpClient().setHeader("X-Custom-Header", "value");
```

## API Modules

- `client.getApplication()` - application API
- `client.getApplicationDomain()` - application_domain API
- `client.getCertificate()` - certificate API
- `client.getDomain()` - domain API
- `client.getApplicationSourceVersion()` - application_source_version API
- `client.getApplicationDeployment()` - application_deployment API
- `client.getCertificateDistribution()` - certificate_distribution API
- `client.getNginx()` - nginx API
- `client.getServer()` - server API
- `client.getServerFile()` - server_file API
- `client.getAgent()` - agent API
- `client.getAudit()` - audit API

## Usage Examples

### application

```java
// List managed applications
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
params.put("application_type", "WEB");
params.put("site_type", 4);
params.put("status", 5);
params.put("keyword", "keyword");
ApplicationsListResponse result = client.getApplication().applicationsList(params);
System.out.println(result);
```

### application_domain

```java
// List application domains
String applicationId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
ApplicationsDomainsListResponse result = client.getApplicationDomain().applicationsDomainsList(applicationId, params);
System.out.println(result);
```

### certificate

```java
// List canonical certificates
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
params.put("domain_id", "00000000-0000-0000-0000-000000000001");
CertificatesListResponse result = client.getCertificate().certificatesList(params);
System.out.println(result);
```

### domain

```java
// List tenant custom domain assets
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
DomainsListResponse result = client.getDomain().domainsList(params);
System.out.println(result);
```

### application_source_version

```java
// List immutable application source versions
String applicationId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("page_size", 1);
params.put("cursor", "cursor");
ApplicationsSourceVersionsListResponse result = client.getApplicationSourceVersion().applicationsSourceVersionsList(applicationId, params);
System.out.println(result);
```

### application_deployment

```java
// List application deployments
String applicationId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("page_size", 1);
params.put("cursor", "cursor");
params.put("status", 3);
ApplicationsDeploymentsListResponse result = client.getApplicationDeployment().applicationsDeploymentsList(applicationId, params);
System.out.println(result);
```

### certificate_distribution

```java
// List certificate manifest convergence by server
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
CertificatesDistributionListResponse result = client.getCertificateDistribution().certificatesDistributionList(params);
System.out.println(result);
```

### nginx

```java
// Retrieve Nginx status
StatusRetrieveResponse result = client.getNginx().statusRetrieve();
System.out.println(result);
```

### server

```java
// List managed servers
Map<String, Object> params = new LinkedHashMap<>();
params.put("page_size", 1);
params.put("cursor", "cursor");
ServersListResponse result = client.getServer().serversList(params);
System.out.println(result);
```

### server_file

```java
// List Server Files deployment nodes
ServerFilesNodesListResponse result = client.getServerFile().serverFilesNodesList();
System.out.println(result);
```

### agent

```java
// Retrieve the Nginx configuration and certificate bundle
Map<String, Object> params = new LinkedHashMap<>();
params.put("if_sync_version", "if-sync-version");
RetrieveResponse result = client.getAgent().retrieve(params);
System.out.println(result);
```

### audit

```java
// List audit logs
Map<String, Object> params = new LinkedHashMap<>();
params.put("page_size", 1);
params.put("cursor", "cursor");
params.put("target_type", "target-type");
params.put("action", "action");
params.put("operator_id", "1");
params.put("start_date", "2026-04-10T00:00:00Z");
params.put("end_date", "2026-04-10T00:00:00Z");
AuditLogsListResponse result = client.getAudit().logsList(params);
System.out.println(result);
```

## Error Handling

```java
try {
    StatusRetrieveResponse result = client.getNginx().statusRetrieve();
    System.out.println(result);
} catch (Exception e) {
    System.err.println("Error: " + e.getMessage());
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

> Use Maven `settings.xml` credentials and optional `MAVEN_PUBLISH_PROFILE`.

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
