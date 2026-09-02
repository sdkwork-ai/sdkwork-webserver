# sdkwork-webserver-app-sdk (Rust)

Generated SDKWork v3 dual-token transport SDK.

## Installation

```bash
cargo add sdkwork-webserver-app-sdk-generated-rust
```

## Quick Start

```rust
use sdkwork_webserver_app_sdk_generated_rust::{SdkworkAppClient, SdkworkConfig};
use std::collections::HashMap;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SdkworkAppClient::new(SdkworkConfig::new("http://localhost:3800"))?;
    client.set_auth_token("your-auth-token");
client.set_access_token("your-access-token");

    let mut query = HashMap::new();
    query.insert("page".to_string(), serde_json::json!(1));
    query.insert("page_size".to_string(), serde_json::json!(2));
    let result = client.domain().domains_list(Some(&query)).await?;
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
let client = SdkworkAppClient::new(SdkworkConfig::new("http://localhost:3800"))?;
client.set_header("X-Custom-Header", "value");
```

## API Modules

- `client.application()` - application API
- `client.domain()` - domain API
- `client.certificate()` - certificate API
- `client.source_version()` - source_version API
- `client.deployment()` - deployment API
- `client.env_variable()` - env_variable API
- `client.monitor()` - monitor API

## Usage Examples

### application

```rust
use std::collections::HashMap;
// 获取应用列表
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
query.insert("status".to_string(), serde_json::json!(0));
query.insert("application_type".to_string(), serde_json::json!("WEB"));
query.insert("site_type".to_string(), serde_json::json!(1));
query.insert("keyword".to_string(), serde_json::json!("keyword"));
let result = client.application().applications_list(Some(&query)).await?;
println!("{result:?}");
```

### domain

```rust
use std::collections::HashMap;
// 获取证书可签发域名列表
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
let result = client.domain().domains_list(Some(&query)).await?;
println!("{result:?}");
```

### certificate

```rust
use std::collections::HashMap;
// List certificates active on the domain listener
let application_id = "1";
let domain_id = "1";
let mut query = HashMap::new();
query.insert("page".to_string(), serde_json::json!(1));
query.insert("page_size".to_string(), serde_json::json!(2));
let result = client.certificate().applications_domains_listener_certificate_bindings_list(application_id, domain_id, Some(&query)).await?;
println!("{result:?}");
```

### source_version

```rust
use std::collections::HashMap;
// 获取应用源码版本
let application_id = "1";
let mut query = HashMap::new();
query.insert("page_size".to_string(), serde_json::json!(1));
query.insert("cursor".to_string(), serde_json::json!("cursor"));
let result = client.source_version().applications_source_versions_list(application_id, Some(&query)).await?;
println!("{result:?}");
```

### deployment

```rust
use std::collections::HashMap;
// 获取部署历史
let application_id = "1";
let mut query = HashMap::new();
query.insert("page_size".to_string(), serde_json::json!(1));
query.insert("cursor".to_string(), serde_json::json!("cursor"));
query.insert("status".to_string(), serde_json::json!(0));
let result = client.deployment().applications_deployments_list(application_id, Some(&query)).await?;
println!("{result:?}");
```

### env_variable

```rust
use std::collections::HashMap;
// 获取环境变量列表
let application_id = "1";
let mut query = HashMap::new();
query.insert("environment".to_string(), serde_json::json!("environment"));
let result = client.env_variable().applications_env_variables_list(application_id, Some(&query)).await?;
println!("{result:?}");
```

### monitor

```rust
// 获取健康检查配置
let application_id = "1";
let result = client.monitor().applications_health_checks_list(application_id).await?;
println!("{result:?}");
```

## Error Handling

```rust
use sdkwork_webserver_app_sdk_generated_rust::{SdkworkAppClient, SdkworkConfig};
use std::collections::HashMap;


let client = SdkworkAppClient::new(SdkworkConfig::new("http://localhost:3800"))?;

let outcome: Result<(), _> = async {
    let mut query = HashMap::new();
    query.insert("page".to_string(), serde_json::json!(1));
    query.insert("page_size".to_string(), serde_json::json!(2));
    client.domain().domains_list(Some(&query)).await?;
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
