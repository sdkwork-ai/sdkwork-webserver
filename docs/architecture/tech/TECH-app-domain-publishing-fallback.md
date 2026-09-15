# App Publishing Domain Fallback (appDomainFallback)

## Purpose

Every user app gets automatically publishable default domains
(`<appLabel>.app[-<env>].<suffix>`) and can additionally bind user custom
domains. The Web Server data plane only knows its local configuration
(virtual hosts, website runtime bindings). When a request host matches no
local configuration, the data plane resolves the server through the
**sdkwork-deployments control plane** and serves the app's latest compiled
website runtime descriptor — instead of returning 404 immediately.

## Platform domain catalog

The default app domain catalog is the same 14-domain inventory the
IM/drive/knowledgebase modules use, i.e. the registered base domains of
`APP_RUNTIME_TOPOLOGY_NAMING.md` §9.3:

```
sdkwork.com  sdkwork.cn  birdcoder.com  birdcoder.cn  dtupay.com  dtupay.cn
skubc.com    skubc.cn    zowalk.com     zowalk.cn     offer86.com offer86.cn
86offer.com  86offer.cn
```

`PLATFORM_APP_DOMAIN_SUFFIXES` (`sdkwork-deploy-core::app_domains`) is the only
definition of this catalog. The Web Server does not hardcode it: its
`appDomainFallback.suffixes` default **delegates** to
`sdkwork_deploy_core::platform_app_domain_suffixes()`, and config validation
rejects any suffix outside the catalog, so a retired brand domain (for example
`noaper.com`) can never be re-introduced through configuration.

## App label (`<appId>`) and custom prefix

`deploy_app.app_domain_label` (a single DNS label) is the `<appLabel>` the
default hostnames are built from. `NULL` falls back to `deploy_app.slug`,
which keeps the historical catalog working. Setting it replaces the prefix
with any label — including the app's own uuid — which is the supported way to
publish on `<appId>.app.<suffix>` when the slug is taken or when a short
stable id is preferred. `deploy_app.app_domain_suffixes` optionally narrows
the catalog per app; `NULL`/empty means the platform catalog.

Default hostnames per lifecycle environment (production uses `app`, other
environments use `app-<env>` so every environment is publishable):

| Environment | Hostname |
| --- | --- |
| production | `<appLabel>.app.<suffix>` |
| development | `<appLabel>.app-dev.<suffix>` |
| test | `<appLabel>.app-test.<suffix>` |
| staging | `<appLabel>.app-staging.<suffix>` |
| demo | `<appLabel>.app-demo.<suffix>` |

## App nginx configuration

`deploy_app.nginx_conf` carries the app's nginx-compatible configuration
(app-level base), with `nginx_conf_sha256` and `nginx_conf_updated_at` as its
digest and audit stamp. `deploy_nginx_config` provides environment- and/or
hostname-scoped overrides; the lookup resolves the precedence

```
hostname + environment  >  environment  >  app base (`deploy_app.nginx_conf`)
```

and returns the winning pair on the resolved server. The data plane then hands
it to the edge through `NginxSiteSink`
(`crate::deploy_nginx_sink::EdgeNginxSiteSink`, built from the edge node
environment): the candidate is validated with `nginx -t` and activated by an
atomic rename under the edge deployment lock, exactly like any other edge
deployment material. Application happens once per `(hostname, configuration
digest)` — a materialized configuration is not rewritten, and a re-published
one (new digest) supersedes it — and never changes the served response. A
failed materialization is logged and swallowed, then retried at most once per
30 s per host (`DeployFallbackResolver`'s `NGINX_SITE_APPLY_RETRY_BACKOFF`):
a transient failure recovers without an operator restarting the node, while a
permanently invalid configuration cannot cost one `nginx -t` subprocess per
request. When the node does not materialize nginx site files
(`SDKWORK_WEBSERVER_NGINX_ENABLED=false`) the configuration is still resolved,
cached and logged, and nothing is written.


## Deploy control plane (sdkwork-deployments)

- `sdkwork-deploy-core::app_domains` owns the catalog, hostname generation
  and parsing (`PLATFORM_APP_DOMAIN_SUFFIXES`,
  `effective_app_domain_label`, `effective_app_domain_suffixes`,
  `default_app_hostname(label, suffix, environment)`,
  `parse_default_app_hostname`, `app_domain_label(environment)`,
  `environment_for_app_domain_label(label)`).
- App creation provisions default publishing domains for **all five**
  lifecycle environments (`DeployService::provision_app_default_domains_all_environments`,
  called from `create_app`), so `<appLabel>.app-dev|app-test|app-staging|app-demo.<suffix>`
  resolve from the moment the app exists. Per environment: one platform DNS
  zone per suffix (`app.<suffix>`, idempotent), 14 EXACT `deploy_domain` rows
  (auto-`VERIFIED` because the platform owns the apex) and 14 `SERVE`
  `deploy_app_binding` rows (first suffix canonical). Hostnames are unique
  across tenants (`uk_deploy_domain_active_hostname`); a label claimed by
  another tenant fails provisioning with a clear conflict.
- `DeployService::resolve_server_by_hostname(hostname, environment)` resolves
  an ACTIVE binding of an `app_status = 'ACTIVE'` app to the newest `VALID`
  revision **of that same environment** (`deploy_app_revision`, filtered on
  `revision.environment = binding.environment`) — the app-level
  `current_revision_id` pointer is deliberately not used, because it is
  written by whichever environment converged its runtime assignments last.
  The lookup never recompiles.
- The compiled descriptor's binding set is read back from `deploy_app_binding`
  for the same environment, so the routing bindings and the lookup rows are
  the same rows: "the domain is ACTIVE in the control plane" and "the hostname
  is routable" cannot disagree, and the auto-provisioned platform domains are
  compiled into the descriptor like any declared binding.
- Repository integration tests: `tests/platform_app_domains.rs`.

## Web Server data plane (sdkwork-webserver)

### Configuration (`appDomainFallback` section of the app config)

```jsonc
"appDomainFallback": {
  "enabled": true,
  "suffixes": ["sdkwork.com", "sdkwork.cn", /* …14 suffixes… */, "86offer.cn"],
  "lookup": { "mode": "embedded" },          // only "embedded" exists
  "timeoutMs": 2000,
  "cacheTtlMs": 60000,
  "negativeCacheTtlMs": 5000
}
```

- `suffixes` defaults to the 14-suffix platform catalog
  (`sdkwork_deploy_core::platform_app_domain_suffixes()`); every configured
  entry must be a member of that catalog.
- `lookup.mode` has exactly one variant, `embedded`: the shared Deploy
  database (standalone deployment, same process as the deploy control plane).
  There is no HTTP lookup channel: the variant is not declared, the schema
  pins `mode` to `const: "embedded"` (`additionalProperties: false`, so the
  retired `endpoint` / `authTokenFile` keys are rejected too), and the enum
  carries `deny_unknown_fields`. Any other `mode` therefore fails config
  validation with a `/appDomainFallback/lookup/mode` diagnostic instead of
  silently disabling the fallback.
- Schema: `specs/sdkwork.webserver.config.schema.json`; semantic validation
  in `crates/sdkwork-webserver-core/src/config/validate.rs`.
- Example: `etc/data-plane/website.cloud.config.json`,
  `etc/examples/sdkwork.webserver.config.json`.

### Request path

`DeployFallbackResolver` (`crates/sdkwork-api-webserver-standalone-gateway/src/deploy_fallback.rs`):

1. Host classification: `<appLabel>.app[-<env>].<suffix>` over the configured
   suffixes is a default app domain; everything else is a custom domain. This
   is load-bearing, not a log label: a platform hostname whose encoded
   lifecycle environment is not the one this node serves is refused outright
   (the lookup could only miss, and a match would serve one environment's
   revision on another environment's hostname). Both the accepted label set
   and the label → environment mapping come from `sdkwork-deploy-core`, so
   they can never drift from what the control plane provisions.
2. Cache (positive TTL / negative TTL) keyed by normalized hostname.
3. Lookup through `DeployServerLookup` (embedded repository adapter) →
   environment-scoped site descriptor + the app's nginx configuration.
4. The app's nginx configuration is handed to the edge sink if one is
   installed (see "App nginx configuration" above), once per
   `(hostname, digest)` and at most once per 30 s after a failed attempt.
5. Compiled-site fast path: descriptors already compiled (LRU keyed by
   `descriptor_sha256`, 32 entries) skip straight to serving. Otherwise the
   descriptor is compiled into a single-site website runtime set and
   activated on a dedicated fallback `WebsiteRuntimeRegistry` (monotonic
   generations) under the activation lock, which covers only
   compile/activate — request execution runs after the lock is released.
6. The request executes against the pinned compiled set
   (`WebsiteDeliveryExecutor::execute_compiled`), so a concurrent activation
   of a different host can never swap the served runtime set between route
   selection and provider dispatch. Bindings, variants, mounts
   (STATIC/SPA/WIKI), redirects, range and conditional requests all reuse
   the website delivery machinery (shared Drive/Knowledgebase provider
   registry).

Hook points (`data_plane/handler.rs`):

- **website-runtime path** — any 404 from `serve_website_request` falls back.
- **app-config path** — the fallback runs when `select_route` returns nothing
  *and* when a resource-backed route (`Static` / `Drive` / `Knowledgebase`)
  ends in 404, i.e. the local resource could not be resolved. Deliberate
  `Respond` routes and upstream `Proxy` replies are never second-guessed:
  their status is authoritative. The two paths are therefore semantically
  consistent — a host that is declared locally but whose resource is missing
  still has a chance of recovery through Deploy.

Only GET/HEAD participate; other methods keep the regular 404. Resolver or
lookup failures degrade to 404 (never 5xx) and never affect gateway startup.

### Wiring

- Built in `website.rs` (`build_deploy_fallback`) when the config section is
  enabled and the shared PostgreSQL pool is available; threaded through
  `run_website_data_plane_*` → `ListenerState.deploy_fallback`.
- The embedded lookup uses `DeployRepository::new_lookup(pool)` +
  `resolve_server_by_hostname_lookup` (read-only, no control-plane service
  dependency).
- The same builder installs `EdgeNginxSiteSink` (`deploy_nginx_sink.rs`) from
  the edge node environment so `deploy_app.nginx_conf` reaches the edge; see
  "App nginx configuration".
- TLS: TLS listeners must cover the app domains (wildcard certificates for
  `*.app[-<env>].<suffix>`, provisioned through the existing certificate
  material flow); plaintext listeners need no certificates.

## Behavior matrix

| Request host | Local config | Fallback | Result |
| --- | --- | --- | --- |
| `myapp.app.sdkwork.com` | miss | deploy ACTIVE binding + VALID revision | site content |
| `mysite.example.com` (custom) | miss | deploy ACTIVE binding | site content |
| `myapp.app.sdkwork.com` | miss | no binding / invalid revision / app not ACTIVE | 404 |
| `myapp.app-dev.sdkwork.com` on a production node | miss | refused by classification | 404 (no lookup) |
| any host | hit, resource resolves | not consulted | local route |
| any host | hit, resource-backed route 404s | consulted | site content or 404 |
| any host | hit, `Respond`/`Proxy` 404 | not consulted | local 404 |
| non-GET/HEAD | miss | not consulted | 404 (or local route) |
| fallback disabled / no DB | miss | skipped | 404 |

## Verification

- Deploy: `cargo test -p sdkwork-deploy-core app_domains`;
  `cargo test -p sdkwork-deploy-runtime-compiler`;
  repository integration tests `tests/platform_app_domains.rs` and
  `tests/app_composition.rs` (require `SDKWORK_DATABASE_TEST_POSTGRES_URL`).
  `tests/app_composition.rs` asserts that a composition replace preserves
  other environments' bindings and the auto-provisioned default domains.
- Web Server: `cargo test -p sdkwork-webserver-core --test webserver_config
  app_domain_fallback`; `cargo test -p sdkwork-api-webserver-standalone-gateway
  --lib deploy_fallback`; `tests/app_domain_fallback_contract.rs` feeds a
  Deploy-compiled descriptor into the Web Server drive/knowledgebase
  providers.
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
