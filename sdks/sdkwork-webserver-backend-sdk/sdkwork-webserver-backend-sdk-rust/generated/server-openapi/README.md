# sdkwork-webserver-backend-sdk (Rust)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
cargo add sdkwork-webserver-backend-sdk-generated-rust
```

## Quick Start

```rust
use sdkwork_webserver_backend_sdk_generated_rust::{SdkworkBackendClient, SdkworkConfig};


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SdkworkBackendClient::new(SdkworkConfig::new("http://localhost:3800"))?;
    client.set_auth_token("your-auth-token");
client.set_access_token("your-access-token");

    let result = client.nginx().status_retrieve().await?;
    println!("{result:?}");
    Ok(())
}
```

## Authentication

```text
Authorization: Bearer <authToken>
Access-Token: <accessToken>
```


## Configuration (Non-Auth)

```rust
let client = SdkworkBackendClient::new(SdkworkConfig::new("http://localhost:3800"))?;
client.set_header("X-Custom-Header", "value");
```

## API Modules

- `client.application()` - application API
- `client.application_domain()` - application_domain API
- `client.certificate()` - certificate API
- `client.domain()` - domain API
- `client.application_source_version()` - application_source_version API
- `client.application_deployment()` - application_deployment API
- `client.certificate_distribution()` - certificate_distribution API
- `client.nginx()` - nginx API
- `client.server()` - server API
- `client.server_file()` - server_file API
- `client.agent()` - agent API
- `client.audit()` - audit API

## Usage Examples

### application

```rust
use std::collections::HashMap;
// List managed applications
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
query.insert("application_type".to_string(), serde_json::json!("WEB"));
query.insert("site_type".to_string(), serde_json::json!(4));
query.insert("status".to_string(), serde_json::json!(5));
query.insert("keyword".to_string(), serde_json::json!("keyword"));
let result = client.application().applications_list(Some(&query)).await?;
println!("{result:?}");
```

### application_domain

```rust
use std::collections::HashMap;
// List application domains
let application_id = "1";
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
let result = client.application_domain().applications_domains_list(application_id, Some(&query)).await?;
println!("{result:?}");
```

### certificate

```rust
use std::collections::HashMap;
// List canonical certificates
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
query.insert("domain_id".to_string(), serde_json::json!("00000000-0000-0000-0000-000000000001"));
let result = client.certificate().certificates_list(Some(&query)).await?;
println!("{result:?}");
```

### domain

```rust
use std::collections::HashMap;
// List tenant custom domain assets
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
let result = client.domain().domains_list(Some(&query)).await?;
println!("{result:?}");
```

### application_source_version

```rust
use std::collections::HashMap;
// List immutable application source versions
let application_id = "1";
let mut query = HashMap::new();
query.insert("page_size".to_string(), serde_json::json!(1));
query.insert("cursor".to_string(), serde_json::json!("cursor"));
let result = client.application_source_version().applications_source_versions_list(application_id, Some(&query)).await?;
println!("{result:?}");
```

### application_deployment

```rust
use std::collections::HashMap;
// List application deployments
let application_id = "1";
let mut query = HashMap::new();
query.insert("page_size".to_string(), serde_json::json!(1));
query.insert("cursor".to_string(), serde_json::json!("cursor"));
query.insert("status".to_string(), serde_json::json!(3));
let result = client.application_deployment().applications_deployments_list(application_id, Some(&query)).await?;
println!("{result:?}");
```

### certificate_distribution

```rust
use std::collections::HashMap;
// List certificate manifest convergence by server
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
let result = client.certificate_distribution().certificates_distribution_list(Some(&query)).await?;
println!("{result:?}");
```

### nginx

```rust
// Retrieve Nginx status
let result = client.nginx().status_retrieve().await?;
println!("{result:?}");
```

### server

```rust
use std::collections::HashMap;
// List managed servers
let mut query = HashMap::new();
query.insert("page_size".to_string(), serde_json::json!(1));
query.insert("cursor".to_string(), serde_json::json!("cursor"));
let result = client.server().servers_list(Some(&query)).await?;
println!("{result:?}");
```

### server_file

```rust
// List Server Files deployment nodes
let result = client.server_file().server_files_nodes_list().await?;
println!("{result:?}");
```

### agent

```rust
use std::collections::HashMap;
// Retrieve the Nginx configuration and certificate bundle
let mut query = HashMap::new();
query.insert("if_sync_version".to_string(), serde_json::json!("if-sync-version"));
let result = client.agent().retrieve(Some(&query)).await?;
println!("{result:?}");
```

### audit

```rust
use std::collections::HashMap;
// List audit logs
let mut query = HashMap::new();
query.insert("page_size".to_string(), serde_json::json!(1));
query.insert("cursor".to_string(), serde_json::json!("cursor"));
query.insert("target_type".to_string(), serde_json::json!("target-type"));
query.insert("action".to_string(), serde_json::json!("action"));
query.insert("operator_id".to_string(), serde_json::json!("1"));
query.insert("start_date".to_string(), serde_json::json!("2026-04-10T00:00:00Z"));
query.insert("end_date".to_string(), serde_json::json!("2026-04-10T00:00:00Z"));
let result = client.audit().logs_list(Some(&query)).await?;
println!("{result:?}");
```

## Error Handling

```rust
use sdkwork_webserver_backend_sdk_generated_rust::{SdkworkBackendClient, SdkworkConfig};


let client = SdkworkBackendClient::new(SdkworkConfig::new("http://localhost:3800"))?;

let outcome: Result<(), _> = async {
    client.nginx().status_retrieve().await?;
    Ok(())
}.await;

match outcome {
    Ok(()) => println!("request completed"),
    Err(error) => eprintln!("request failed: {error}"),
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

> Set cargo registry credentials before `cargo publish` and use `--dry-run` first.

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
