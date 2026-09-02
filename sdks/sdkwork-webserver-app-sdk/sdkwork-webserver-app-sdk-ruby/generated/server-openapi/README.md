# sdkwork-webserver-app-sdk (Ruby)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
gem install sdkwork-webserver-app-sdk
```

## Quick Start

```ruby
require 'sdkwork/app_sdk'

config = Sdkwork::AppSdk::SdkConfig.new(base_url: 'http://localhost:3800')
client = Sdkwork::AppSdk::SdkworkAppClient.new(config)
params = { 'page' => 1, 'page_size' => 2, 'status' => 0, 'application_type' => 'WEB', 'site_type' => 1, 'keyword' => 'keyword' }
result = client.application.applications_list(params: params)


puts result.inspect
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```ruby
config = Sdkwork::AppSdk::SdkConfig.new(base_url: 'http://localhost:3800')
client = Sdkwork::AppSdk::SdkworkAppClient.new(config)

# Set custom headers
client.set_header('X-Custom-Header', 'value')
```

## API Modules

- `client.application` - application API
- `client.domain` - domain API
- `client.certificate` - certificate API
- `client.source_version` - source_version API
- `client.deployment` - deployment API
- `client.env_variable` - env_variable API
- `client.monitor` - monitor API

## Usage Examples

### application

```ruby
# 获取应用列表
params = { 'page' => 1, 'page_size' => 2, 'status' => 0, 'application_type' => 'WEB', 'site_type' => 1, 'keyword' => 'keyword' }
result = client.application.applications_list(params: params)
puts result.inspect
```

### domain

```ruby
# 获取证书可签发域名列表
params = { 'page' => 1, 'page_size' => 2 }
result = client.domain.domains_list(params: params)
puts result.inspect
```

### certificate

```ruby
# List certificates active on the domain listener
application_id = '1'
domain_id = '1'
params = { 'page' => 1, 'page_size' => 2 }
result = client.certificate.applications_domains_listener_certificate_bindings_list(application_id, domain_id, params: params)
puts result.inspect
```

### source_version

```ruby
# 获取应用源码版本
application_id = '1'
params = { 'page_size' => 1, 'cursor' => 'cursor' }
result = client.source_version.applications_source_versions_list(application_id, params: params)
puts result.inspect
```

### deployment

```ruby
# 获取部署历史
application_id = '1'
params = { 'page_size' => 1, 'cursor' => 'cursor', 'status' => 0 }
result = client.deployment.applications_deployments_list(application_id, params: params)
puts result.inspect
```

### env_variable

```ruby
# 获取环境变量列表
application_id = '1'
params = { 'environment' => 'environment' }
result = client.env_variable.applications_env_variables_list(application_id, params: params)
puts result.inspect
```

### monitor

```ruby
# 获取健康检查配置
application_id = '1'
result = client.monitor.applications_health_checks_list(application_id)
puts result.inspect
```

## Error Handling

```ruby
begin
  params = { 'page' => 1, 'page_size' => 2, 'status' => 0, 'application_type' => 'WEB', 'site_type' => 1, 'keyword' => 'keyword' }
  client.application.applications_list(params: params)
rescue StandardError => e
  warn("Error: #{e.message}")
end
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

> Configure RubyGems registry credentials before release publish.

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
