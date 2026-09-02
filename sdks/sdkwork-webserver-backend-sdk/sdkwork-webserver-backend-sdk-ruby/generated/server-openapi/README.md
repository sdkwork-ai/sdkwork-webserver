# sdkwork-webserver-backend-sdk (Ruby)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
gem install sdkwork-webserver-backend-sdk
```

## Quick Start

```ruby
require 'sdkwork/backend_sdk'

config = Sdkwork::BackendSdk::SdkConfig.new(base_url: 'http://localhost:3800')
client = Sdkwork::BackendSdk::SdkworkBackendClient.new(config)
result = client.server_file.server_files_nodes_list()


puts result.inspect
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```ruby
config = Sdkwork::BackendSdk::SdkConfig.new(base_url: 'http://localhost:3800')
client = Sdkwork::BackendSdk::SdkworkBackendClient.new(config)

# Set custom headers
client.set_header('X-Custom-Header', 'value')
```

## API Modules

- `client.application` - application API
- `client.application_domain` - application_domain API
- `client.certificate` - certificate API
- `client.domain` - domain API
- `client.application_source_version` - application_source_version API
- `client.application_deployment` - application_deployment API
- `client.certificate_distribution` - certificate_distribution API
- `client.nginx` - nginx API
- `client.server` - server API
- `client.server_file` - server_file API
- `client.agent` - agent API
- `client.audit` - audit API

## Usage Examples

### application

```ruby
# List managed applications
params = { 'page' => 1, 'page_size' => 2, 'application_type' => 'WEB', 'site_type' => 4, 'status' => 5, 'keyword' => 'keyword' }
result = client.application.applications_list(params: params)
puts result.inspect
```

### application_domain

```ruby
# List application domains
application_id = '1'
params = { 'page' => 1, 'page_size' => 2 }
result = client.application_domain.applications_domains_list(application_id, params: params)
puts result.inspect
```

### certificate

```ruby
# List canonical certificates
params = { 'page' => 1, 'page_size' => 2, 'domain_id' => '00000000-0000-0000-0000-000000000001' }
result = client.certificate.certificates_list(params: params)
puts result.inspect
```

### domain

```ruby
# List tenant custom domain assets
params = { 'page' => 1, 'page_size' => 2 }
result = client.domain.domains_list(params: params)
puts result.inspect
```

### application_source_version

```ruby
# List immutable application source versions
application_id = '1'
params = { 'page_size' => 1, 'cursor' => 'cursor' }
result = client.application_source_version.applications_source_versions_list(application_id, params: params)
puts result.inspect
```

### application_deployment

```ruby
# List application deployments
application_id = '1'
params = { 'page_size' => 1, 'cursor' => 'cursor', 'status' => 3 }
result = client.application_deployment.applications_deployments_list(application_id, params: params)
puts result.inspect
```

### certificate_distribution

```ruby
# List certificate manifest convergence by server
params = { 'page' => 1, 'page_size' => 2 }
result = client.certificate_distribution.certificates_distribution_list(params: params)
puts result.inspect
```

### nginx

```ruby
# Retrieve Nginx status
result = client.nginx.status_retrieve()
puts result.inspect
```

### server

```ruby
# List managed servers
params = { 'page_size' => 1, 'cursor' => 'cursor' }
result = client.server.servers_list(params: params)
puts result.inspect
```

### server_file

```ruby
# List Server Files deployment nodes
result = client.server_file.server_files_nodes_list()
puts result.inspect
```

### agent

```ruby
# Retrieve the Nginx configuration and certificate bundle
params = { 'if_sync_version' => 'if-sync-version' }
result = client.agent.retrieve(params: params)
puts result.inspect
```

### audit

```ruby
# List audit logs
params = { 'page_size' => 1, 'cursor' => 'cursor', 'target_type' => 'target-type', 'action' => 'action', 'operator_id' => '1', 'start_date' => '2026-04-10T00:00:00Z', 'end_date' => '2026-04-10T00:00:00Z' }
result = client.audit.logs_list(params: params)
puts result.inspect
```

## Error Handling

```ruby
begin
  client.server_file.server_files_nodes_list()
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
