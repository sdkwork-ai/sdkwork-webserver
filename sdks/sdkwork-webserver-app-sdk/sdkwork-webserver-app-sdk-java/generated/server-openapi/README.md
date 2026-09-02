# sdkwork-webserver-app-sdk (Java)

Generated SDKWork v3 dual-token transport SDK.

## Installation

Add to your `pom.xml`:

```xml
<dependency>
    <groupId>com.sdkwork</groupId>
    <artifactId>sdkwork-webserver-app-sdk</artifactId>
    <version>1.0.0</version>
</dependency>
```

Or with Gradle:

```groovy
implementation 'com.sdkwork:sdkwork-webserver-app-sdk:1.0.0'
```

## Quick Start

```java
import com.sdkwork.webserver.app.sdk.SdkworkAppClient;
import com.sdkwork.common.core.Types;
import com.sdkwork.webserver.app.sdk.model.*;
import java.util.LinkedHashMap;
import java.util.Map;

public class Main {
    public static void main(String[] args) throws Exception {
        Types.SdkConfig config = new Types.SdkConfig("http://localhost:3800");
        SdkworkAppClient client = new SdkworkAppClient(config);
        client.setAuthToken("your-auth-token");
client.setAccessToken("your-access-token");

        // Use the SDK
        Map<String, Object> params = new LinkedHashMap<>();
        params.put("page", 1);
        params.put("page_size", 2);
        DomainsListResponse result = client.getDomain().domainsList(params);
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
SdkworkAppClient client = new SdkworkAppClient(config);

// Set custom headers
client.getHttpClient().setHeader("X-Custom-Header", "value");
```

## API Modules

- `client.getApplication()` - application API
- `client.getDomain()` - domain API
- `client.getCertificate()` - certificate API
- `client.getSourceVersion()` - source_version API
- `client.getDeployment()` - deployment API
- `client.getEnvVariable()` - env_variable API
- `client.getMonitor()` - monitor API

## Usage Examples

### application

```java
// 获取应用列表
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
params.put("status", 0);
params.put("application_type", "WEB");
params.put("site_type", 1);
params.put("keyword", "keyword");
ApplicationsListResponse result = client.getApplication().applicationsList(params);
System.out.println(result);
```

### domain

```java
// 获取证书可签发域名列表
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
DomainsListResponse result = client.getDomain().domainsList(params);
System.out.println(result);
```

### certificate

```java
// List certificates active on the domain listener
String applicationId = "1";
String domainId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("page", 1);
params.put("page_size", 2);
ApplicationsDomainsListenerCertificateBindingsListResponse result = client.getCertificate().applicationsDomainsListenerCertificateBindingsList(applicationId, domainId, params);
System.out.println(result);
```

### source_version

```java
// 获取应用源码版本
String applicationId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("page_size", 1);
params.put("cursor", "cursor");
ApplicationsSourceVersionsListResponse result = client.getSourceVersion().applicationsSourceVersionsList(applicationId, params);
System.out.println(result);
```

### deployment

```java
// 获取部署历史
String applicationId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("page_size", 1);
params.put("cursor", "cursor");
params.put("status", 0);
ApplicationsDeploymentsListResponse result = client.getDeployment().applicationsDeploymentsList(applicationId, params);
System.out.println(result);
```

### env_variable

```java
// 获取环境变量列表
String applicationId = "1";
Map<String, Object> params = new LinkedHashMap<>();
params.put("environment", "environment");
ApplicationsEnvVariablesListResponse result = client.getEnvVariable().applicationsEnvVariablesList(applicationId, params);
System.out.println(result);
```

### monitor

```java
// 获取健康检查配置
String applicationId = "1";
ApplicationsHealthChecksListResponse result = client.getMonitor().applicationsHealthChecksList(applicationId);
System.out.println(result);
```

## Error Handling

```java
try {
    Map<String, Object> params = new LinkedHashMap<>();
    params.put("page", 1);
    params.put("page_size", 2);
    DomainsListResponse result = client.getDomain().domainsList(params);
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
