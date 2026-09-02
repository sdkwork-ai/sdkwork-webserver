# sdkwork-webserver-app-sdk (Dart)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
dart pub add sdkwork_webserver_app_sdk
```

## Quick Start

```dart
import 'package:sdkwork_webserver_app_sdk/sdkwork_webserver_app_sdk.dart';

final client = SdkworkAppClient(
  config: const SdkConfig(
    baseUrl: 'http://localhost:3800',
  ),
);
client.setAuthToken('your-auth-token');
client.setAccessToken('your-access-token');

// Use the SDK
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.domain.domainsList(params);
print(result);
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```dart
final client = SdkworkAppClient.withBaseUrl(baseUrl: 'http://localhost:3800');
client.setHeader('X-Custom-Header', 'value');
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

```dart
// 获取应用列表
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
  'status': 0,
  'application_type': 'WEB',
  'site_type': 1,
  'keyword': 'keyword',
};
final result = await client.application.applicationsList(params);
print(result);
```

### domain

```dart
// 获取证书可签发域名列表
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.domain.domainsList(params);
print(result);
```

### certificate

```dart
// List certificates active on the domain listener
final applicationId = '1';
final domainId = '1';
final params = <String, dynamic>{
  'page': 1,
  'page_size': 2,
};
final result = await client.certificate.applicationsDomainsListenerCertificateBindingsList(applicationId, domainId, params);
print(result);
```

### source_version

```dart
// 获取应用源码版本
final applicationId = '1';
final params = <String, dynamic>{
  'page_size': 1,
  'cursor': 'cursor',
};
final result = await client.sourceVersion.applicationsSourceVersionsList(applicationId, params);
print(result);
```

### deployment

```dart
// 获取部署历史
final applicationId = '1';
final params = <String, dynamic>{
  'page_size': 1,
  'cursor': 'cursor',
  'status': 0,
};
final result = await client.deployment.applicationsDeploymentsList(applicationId, params);
print(result);
```

### env_variable

```dart
// 获取环境变量列表
final applicationId = '1';
final params = <String, dynamic>{
  'environment': 'environment',
};
final result = await client.envVariable.applicationsEnvVariablesList(applicationId, params);
print(result);
```

### monitor

```dart
// 获取健康检查配置
final applicationId = '1';
final result = await client.monitor.applicationsHealthChecksList(applicationId);
print(result);
```

## Error Handling

```dart
try {
  final params = <String, dynamic>{
    'page': 1,
    'page_size': 2,
  };
  final result = await client.domain.domainsList(params);
  print(result);
} catch (error) {
  print('Error: $error');
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

> Ensure `dart pub publish --dry-run` passes before release publish.

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
