# App Publishing Domain Fallback (appDomainFallback)

## Purpose

Every user app gets automatically publishable default domains
(`<slug>.app[-<env>].<suffix>`) and can additionally bind user custom
domains. The Web Server data plane only knows its local configuration
(virtual hosts, website runtime bindings). When a request host matches no
local configuration, the data plane now resolves the server through the
**sdkwork-deployments control plane** and serves the app's latest compiled
website runtime descriptor — instead of returning 404 immediately.

## Platform domain catalog

The default app domain catalog is the same 14-domain inventory the
IM/drive/knowledgebase modules use:

```
sdkwork.com  sdkwork.cn  birdcoder.com  birdcoder.cn  dtupay.com  dtupay.cn
skubc.com    skubc.cn    zowalk.com     zowalk.cn     offer86.com offer86.cn
86offer.com  86offer.cn
```

Default hostnames per lifecycle environment (production uses `app`, other
environments use `app-<env>` so every environment is publishable):

| Environment | Hostname |
| --- | --- |
| production | `<slug>.app.<suffix>` |
| development | `<slug>.app-dev.<suffix>` |
| test | `<slug>.app-test.<suffix>` |
| staging | `<slug>.app-staging.<suffix>` |

## Deploy control plane (sdkwork-deployments)

- `sdkwork-deploy-core::app_domains` owns the catalog, hostname generation
  and parsing (`PLATFORM_APP_DOMAIN_SUFFIXES`,
  `default_app_hostname(slug, suffix, environment)`,
  `parse_default_app_hostname`).
- App creation auto-provisions default publishing domains
  (`DeployService::provision_app_default_domains`): one platform DNS zone
  per suffix (`app.<suffix>`, idempotent), 14 EXACT `deploy_domain` rows per
  app (auto-`VERIFIED` because the platform owns the apex) and 14 `SERVE`
  `deploy_app_binding` rows (first suffix canonical). Hostnames are unique
  across tenants (`uk_deploy_domain_active_hostname`); a slug claimed by
  another tenant fails provisioning with a clear conflict.
- `DeployService::resolve_server_by_hostname(hostname, environment)`
  resolves an ACTIVE binding to the app's latest VALID compiled revision
  (`deploy_app_revision.descriptor_json` + `descriptor_sha256`) — the
  lookup never recompiles.
- Repository integration tests: `tests/platform_app_domains.rs`.

## Web Server data plane (sdkwork-webserver)

### Configuration (`appDomainFallback` section of the app config)

```jsonc
"appDomainFallback": {
  "enabled": true,
  "suffixes": ["sdkwork.com", "sdkwork.cn", /* …14 suffixes… */, "86offer.cn"],
  "lookup": { "mode": "embedded" },          // embedded | http
  "timeoutMs": 2000,
  "cacheTtlMs": 60000,
  "negativeCacheTtlMs": 5000
}
```

- `suffixes` defaults to the 14-suffix platform catalog.
- `lookup.mode = embedded` resolves through the shared Deploy database
  (standalone deployment, same process as the deploy control plane).
- `lookup.mode = http` is the cloud control-plane API channel (endpoint +
  optional `authTokenFile`).
- Schema: `specs/sdkwork.webserver.config.schema.json`; semantic validation
  in `crates/sdkwork-webserver-core/src/config/validate.rs`.
- Example: `etc/data-plane/website.cloud.config.json`,
  `etc/examples/sdkwork.webserver.config.json`.

### Request path

`DeployFallbackResolver` (`crates/sdkwork-api-webserver-standalone-gateway/src/deploy_fallback.rs`):

1. Host classification: `<slug>.app[-<env>].<suffix>` over the configured
   suffixes is a default app domain; everything else is a custom domain.
2. Cache (positive TTL / negative TTL) keyed by normalized hostname.
3. Lookup through `DeployServerLookup` (embedded repository adapter or HTTP
   client) → site descriptor.
4. The descriptor is compiled into a single-site website runtime set and
   activated on a dedicated fallback `WebsiteRuntimeRegistry` (monotonic
   generations, no-op when the snapshot is already current).
5. The request is served by the fallback `WebsiteDeliveryExecutor` (shared
   Drive/Knowledgebase provider registry): bindings, variants, mounts
   (STATIC/SPA/WIKI), redirects, range and conditional requests all reuse
   the website delivery machinery.

Hook points (`data_plane/handler.rs`): the website-delivery path falls back
on 404; the app-config path falls back when `select_route` returns nothing.
Only GET/HEAD participate; other methods keep the regular 404. Resolver or
lookup failures degrade to 404 (never 5xx) and never affect gateway startup.

### Wiring

- Built in `website.rs` (`build_deploy_fallback`) when the config section is
  enabled and the shared PostgreSQL pool is available; threaded through
  `run_website_data_plane_*` → `ListenerState.deploy_fallback`.
- The embedded lookup uses `DeployRepository::new_lookup(pool)` +
  `resolve_server_by_hostname_lookup` (read-only, no control-plane service
  dependency).
- TLS: TLS listeners must cover the app domains (wildcard certificates for
  `*.app[-<env>].<suffix>`, provisioned through the existing certificate
  material flow); plaintext listeners need no certificates.

## Behavior matrix

| Request host | Local config | Fallback | Result |
| --- | --- | --- | --- |
| `myapp.app.sdkwork.com` | miss | deploy ACTIVE binding + VALID revision | site content |
| `mysite.example.com` (custom) | miss | deploy ACTIVE binding | site content |
| `myapp.app.sdkwork.com` | miss | no binding / invalid revision | 404 |
| any host | hit | not consulted | local route |
| non-GET/HEAD | miss | not consulted | 404 (or local route) |
| fallback disabled / no DB | miss | skipped | 404 |

## Verification

- Deploy: `cargo test -p sdkwork-deploy-core app_domains`;
  repository integration tests `tests/platform_app_domains.rs`
  (require `SDKWORK_DATABASE_TEST_POSTGRES_URL`).
- Web Server: `cargo test -p sdkwork-webserver-core --test webserver_config
  app_domain_fallback`; `cargo test -p sdkwork-api-webserver-standalone-gateway
  --lib deploy_fallback`.
- Config schema: `etc/data-plane/website.cloud.config.json` and
  `etc/examples/sdkwork.webserver.config.json` validate against
  `specs/sdkwork.webserver.config.schema.json`.

# SaaS Traffic Usage Metering (usageMetering)

## Purpose

The SaaS Web Server records per-domain / per-server-IP traffic usage for
every served request, attributed to the serving tenant and app when
known, and ingests the facts into the sdkwork-deployments billing tables
(`deploy_usage_event` + daily rollups). Tenants and platform operators can
query usage per app, per domain, per server IP and per tenant — the basis
for per-tenant / per-app billing.

## Dimensions and attribution

| Dimension | Source |
| --- | --- |
| domain (hostname) | normalized request `Host` |
| server IP / port | local socket (`transport_peer`) |
| listener | `listener_id` |
| tenant / organization | Deploy attribution (binding tenant) |
| app id / slug | Deploy attribution (`deploy_app` via site) |
| site / binding | website runtime route identity or Deploy fallback resolution |

Attribution resolution:
- website-runtime-served traffic: the outcome's route identity
  (`site_uuid`, `binding_uuid`); the control plane resolves the tenant from
  the binding at ingest (`deploy_app_binding.tenant_id`).
- app-domain-fallback-served traffic: the fallback resolver caches
  `tenant_id` / `app_id` / `binding_uuid` from the Deploy resolution.
- locally configured (non-Deploy) hosts: unmanaged, attributed to tenant 0
  with hostname + server IP.

## Facts and rollups

- `deploy_usage_event` rows per window per
  (tenant, site, binding, hostname, server IP, dimension): dimensions
  `traffic.requests` (unit `REQUEST`), `traffic.ingress_bytes` /
  `traffic.egress_bytes` (unit `BYTE`). Deduplicated on
  `(tenant_id, deduplication_key)` with a deterministic window key
  (`traffic:<window>:<dim>:<sha256 fingerprint>`).
- `deploy_app_usage_daily` (per site + binding, `0002_usage_metering`
  migration adds `binding_id`) and `deploy_tenant_usage_daily` (per
  tenant, new table) are rebuilt from facts by
  `POST /backend/v3/api/usage/reconcile`.
- Entitlements can enforce traffic dimensions
  (`traffic.requests`, `traffic.ingress_bytes`, `traffic.egress_bytes`)
  from the tenant daily rollup.

## Data plane

`UsageMeteringAggregator` (gateway `usage_metering.rs`) buckets requests
per (domain, server IP, tenant, site, binding, status class, window),
flushes only fully closed windows on `flushIntervalMs` (a mid-window flush
would split one window into two events with the same deduplication key and
drop the second as a duplicate) and re-queues failed ingests. Deduplication
keys scope per node, window and dimension so multi-node deployments never
deduplicate each other's traffic.
Channels: `embedded` (shared Deploy database via `DeployRepository`),
`http` (`POST <endpoint>/backend/v3/api/usage/ingest` on the control
plane). Recording points: the website delivery layer (exact outcome bytes
and route identity), the app-domain fallback path (Deploy attribution) and
the app-config response path (hostname + server IP).

## Configuration

```jsonc
"usageMetering": {
  "enabled": true,
  "windowSeconds": 60,
  "flushIntervalMs": 30000,
  "channel": { "mode": "embedded" }   // embedded | http { endpoint, authTokenFile }
}
```

Schema: `specs/sdkwork.webserver.config.schema.json`; examples:
`etc/data-plane/website.cloud.config.json`,
`etc/examples/sdkwork.webserver.config.json`.

## Verification

- `cargo test -p sdkwork-api-webserver-standalone-gateway --lib usage_metering`
- `cargo test -p sdkwork-webserver-core --test webserver_config usage_metering`
- Deploy repository PG integration tests (`tests/usage_metering_postgres.rs`,
  `tests/platform_app_domains.rs`) require `SDKWORK_DATABASE_TEST_POSTGRES_URL`.
