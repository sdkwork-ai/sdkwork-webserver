# sdkwork-webserver-database-host

Domain: platform
Capability: webserver-database-lifecycle
Package type: Rust database provider crate
Status: active

## Public API

The crate exports `WebDatabaseHost`, `bootstrap_web_database`, and
`bootstrap_web_database_from_env`. It loads the application-owned database
module and executes SDKWork lifecycle initialization before repository traffic.

## Required SDK Surface

No generated HTTP SDK is consumed. Lifecycle behavior is provided by the
application-root SDKWork database framework crates.

## Configuration

Database engine, URL, pool limits, and lifecycle overrides are resolved through
the typed `SDKWORK_WEBSERVER_*` database environment profile. Concrete values belong
in source `etc/`, installed operator configuration, or secret injection, never
in `agent.app.config.json`.

## Deployment Profile And Runtime Target Behavior

PostgreSQL is the only authoritative server database for standalone and cloud
profiles (REQ-2026-0004); no server-side SQLite repository or release profile
exists. This crate is control-plane only and is not part of the HTTP/HTTPS
request data path.

## Security

The host delegates migrations, seed history, drift, URL masking, and pool
construction to SDKWork database framework components. Tests use only empty,
disposable databases and refuse non-empty PostgreSQL schemas.

## Extension Points

Add lifecycle behavior through the database module/SPI contract; do not execute
ad hoc DDL from HTTP handlers or repositories.

## Verification

```powershell
pnpm db:validate
$env:SDKWORK_DATABASE_TEST_POSTGRES_URL='<disposable-url>'; pnpm db:test:postgres
```
