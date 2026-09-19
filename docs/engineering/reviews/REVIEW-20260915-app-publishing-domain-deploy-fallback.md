# REVIEW-20260915 App Publishing Domain And Deploy Fallback

Status: resolved
Owner: sdkwork-webserver
Date: 2026-09-15
Resolved: 2026-09-15
Scope: sdkwork-webserver, sdkwork-deployments
Specs: SDKWORK_WEBSERVER_SPEC.md, ENVIRONMENT_SPEC.md, DATABASE_SPEC.md, API_SPEC.md, CONFIG_SPEC.md, DEPLOYMENT_SPEC.md, CODE_REVIEW_SPEC.md

## 0. Resolution

All 15 findings are closed. The summary below is the authoritative index; the
finding sections are kept verbatim as the record of what was wrong.

| # | Finding | Resolution |
| --- | --- | --- |
| P0-1 | Composition replace destroys the auto-provisioned publishing domains | `replace_app_composition` now reconciles per environment: platform default domains are reconciled **first** in the same transaction (`reconcile_app_default_domains_tx`), stale auto-provisioned `appd-*` bindings are retired before insert (`retire_stale_default_bindings_tx`), and `delete_current_composition` is environment-scoped and preserves undeclared-but-platform-owned rows. Verified by the PG test `a development publish must not delete production bindings`. |
| P0-2 | Served revision is app-global | `resolve_active_app_by_hostname_repo` selects the newest `VALID` revision of the **binding's own environment** with a `LATERAL` subquery (`revision.environment = b.environment`); `deploy_app.current_revision_id` is no longer read. |
| P0-3 | Provider contract version mismatch | `sdkwork-deploy-runtime-compiler` owns `DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION` / `KNOWLEDGEBASE_WIKI_PUBLICATION_PROVIDER_CONTRACT_VERSION`, normalizes legacy spellings instead of emitting them, and validates every resource at compile time. Byte-equality with the Web Server adapters is asserted by `tests/app_domain_fallback_contract.rs`. |
| P1-4 | `demo` not representable | DDL CHECKs, `deploy_app.default_environment`, `app_domain_label`/`environment_for_app_domain_label`, the deploy service catalog (`SUPPORTED_APP_ENVIRONMENTS`) and the Web Server `WebsiteRuntimeEnvironment` catalog all carry the same five environments; the Web Server catalog is now a single `parse`/`as_str`/`ALL` authority. |
| P1-5 | `lookup.mode = http` declared but never implemented | The variant is deleted from `AppDomainFallbackLookup` (which now carries `deny_unknown_fields`, like `WebsiteBindingAction`), the published schema constrains `appDomainFallback.lookup.mode` to `const: "embedded"` and drops the never-implemented `endpoint`/`authTokenFile` keys (`additionalProperties: false`), so declaring the removed channel fails schema validation with a `/appDomainFallback/lookup/mode` diagnostic instead of silently disabling the fallback. `usageMetering.channel` legitimately keeps both modes and is untouched. The tech doc no longer documents it. |
| P1-6 | Two binding truth sources | The descriptor's binding set is read back from `deploy_app_binding` for the compiling environment (`load_environment_bindings`), so the lookup rows and the routing rows are the same rows and provisioned default domains are compiled like declared ones. |
| P1-7 | Suffix catalog drift (14 vs 16, `noaper.*`) | `sdkwork_deploy_core::platform_app_domain_suffixes()` is the single catalog; the Web Server default delegates to it and validation rejects out-of-catalog suffixes. All 220 `noaper.com`/`noaper.cn` hosts were retired from `etc/`, `deployments/` (nginx sidecars, `server.*.toml`, docker-compose, `deploy.yaml`) and `specs/topology.spec.json`, matching the `APP_RUNTIME_TOPOLOGY_NAMING.md` §9.3 registry. |
| P1-8 | `app_status` not checked | The lookup joins `deploy_app` with `s.app_status = 'ACTIVE'`. |
| P2-9 | No `deploy_app` nginx conf field; never read by the edge | `deploy_app` carries `app_domain_label`, `app_domain_suffixes`, `nginx_conf`, `nginx_conf_sha256`, `nginx_conf_updated_at`; the lookup applies the precedence hostname+environment → environment → app base; and the data plane materializes it through `NginxSiteSink` (`EdgeNginxSiteSink` over `sdkwork-webserver-edge-runtime`), validated with `nginx -t` and activated atomically, once per `(hostname, digest)` and retried at a bounded 30 s rate (`NGINX_SITE_APPLY_RETRY_BACKOFF`) after a failure. |
| P2-10 | No domain/prefix surface | Covered with P2-9: any DNS label (including the app uuid) can replace the `<appId>` prefix, and per-app suffix narrowing is supported. |
| P2-11 | `classify_host` is dead logic | It is load-bearing: a platform hostname whose label encodes another lifecycle environment is refused before any lookup, the label→environment mapping comes from `sdkwork-deploy-core`, and the test asserts the whole label set including `app-demo`. |
| P2-12 | Default domains for one environment only | `provision_app_default_domains_all_environments` reconciles all five environments on `create_app`, per-environment failures logged and the first returned. |
| P2-13 | Path is not part of the resolution API | The lookup documents and implements a deterministic order (root prefix first, then longest prefix, then newest row) and `ResolvedDeployServer.path_prefix` is returned; the descriptor remains authoritative for routing. |
| P2-14 | Local host hit + 404 does not fall back (app-config path) | The app-config path now consults Deploy when a `Static`/`Drive`/`Knowledgebase` route ends in 404; `Respond` and `Proxy` replies are never second-guessed. Both hook points are documented in the tech doc. |
| P2-15 | Stale tests and docs | The PG seeds match the DDL, the stale `deploy_app.site_id` claim in `MIG-2026-0070-application-resource-model.md` is replaced with the real `deploy_app` model (the `deploy_site` carrier was folded away by `0007_deploy_app_delivery`), and `docs/guides/operator/deploy-0.1.4-test-urls.md` no longer lists `noaper`. |

## 1. Executive Result

The local-configuration-miss → sdkwork-deployments fallback chain exists and its
data-plane mechanics (negative cache, pinned compiled set, monotonic generations,
GET/HEAD-only, 404 degradation) are sound. The **control-plane half is not
coherent**: the domain→app lookup, the served revision, and the resource
contract do not agree with each other, and the requested deploy_app-level
domain/nginx-conf surface does not exist.

| # | Area | Result | Severity |
| --- | --- | --- | --- |
| 1 | Live composition replace deletes every app binding (incl. auto-provisioned publishing domains) | Broken | P0 |
| 2 | Hostname resolution ignores the lifecycle environment for the revision | Broken | P0 |
| 3 | Drive / Knowledgebase provider contract version differs between the two repositories | Broken | P0 |
| 4 | `demo` environment is not representable in the Deploy schema | Broken | P1 |
| 5 | `lookup.mode = http` channel is declared but never implemented | Missing | P1 |
| 6 | Two independent binding truth sources (DB rows vs. compiled descriptor) | Unsound | P1 |
| 7 | Platform suffix catalog differs in four places (14 vs 16, `noaper.*`) | Drift | P1 |
| 8 | App lifecycle state (`app_status`) is not checked by the lookup | Missing | P1 |
| 9 | `deploy_app` has no nginx-compatible conf field; `deploy_nginx_config` is never read by the edge | Missing | P2 |
| 10 | No `deploy_app` domain field / custom prefix replacement of the app label | Missing | P2 |
| 11 | `classify_host` / `HostClass` is dead logic (log label only) | Debt | P2 |
| 12 | Default domains are provisioned for one environment only | Missing | P2 |
| 13 | Local host hit + 404 does not fall back on the app-config path | Inconsistent | P2 |
| 14 | Deploy PG integration tests and one webserver migration doc are stale vs. the DDL | Drift | P2 |

## 2. Current Chain (verified)

```text
request Host
  └─ data_plane/handler.rs
       ├─ website-runtime path: serve_website_request() → 404 → serve_deploy_fallback()
       └─ app-config path: select_route() == None → serve_deploy_fallback()
            └─ DeployFallbackResolver::serve()            (deploy_fallback.rs:199)
                 ├─ normalize_authority_host + classify_host (log label only)
                 ├─ positive/negative hostname cache
                 ├─ DeployServerLookup::resolve_server(host, webserver_environment)
                 │    └─ EmbeddedDeployServerLookup                  (deploy_fallback.rs:484)
                 │         └─ DeployRepository::resolve_server_by_hostname_lookup
                 │              └─ resolve_active_app_by_hostname_repo
                 │                   (platform_app_domains.rs:277)
                 ├─ compile descriptor → single-site runtime set → activate
                 └─ WebsiteDeliveryExecutor::execute_compiled
```

Wiring: `website.rs:2591 build_deploy_fallback` (gated on `#[cfg(feature =
"management")]` + shared PostgreSQL pool). Config:
`etc/data-plane/website.cloud.config.json` (`enabled: true`, `lookup.mode =
embedded`).

## 3. Findings

> The findings below are the original record, kept verbatim. See §0 for the
> resolution of each one.

### P0-1 — Composition replace destroys the auto-provisioned publishing domains

`replace_app_composition` unconditionally runs `delete_current_composition`
before re-inserting:

```rust
// sdkwork-deployments/.../app_composition.rs:100
delete_current_composition(&mut transaction, app.id).await?;
//        :407-426
("deploy_app_binding", ...), ("deploy_app_variant_rule", ...),
("deploy_app_mount", ...), ("deploy_app_variant", ...),
("deploy_app_resource", ...)   // DELETE FROM {table} WHERE app_id = $1
```

Consequences:

1. The 14 auto-provisioned `SERVE` bindings written by
   `provision_app_default_domains_repo` (`platform_app_domains.rs:134-270`) are
   deleted, so `<slug>.app.<suffix>` stops resolving after **any** composition
   update until the client re-declares every hostname in the request.
2. The delete is **not environment-scoped**. Changing the `development`
   composition also deletes the `production` bindings/variants/mounts/resources,
   leaving already-published `production` descriptors pointing at variant and
   resource UUIDs that no longer exist.
3. Nothing re-provisions the default domains after a composition replace:
   `provision_app_default_domains` is only called from `create_app`
   (`sdkwork-intelligence-deploy-service/src/app_delivery.rs:131`).

Fix direction: replace the delete-then-insert with an environment-scoped
reconcile (upsert by `binding_key` / `mount_key` / `resource_key`, delete only
rows absent from the request), and re-provision the platform default domains
inside the same transaction when the request does not declare them.

### P0-2 — The served revision is app-global, not per environment

```sql
-- .../platform_app_domains.rs:288-299
FROM deploy_app_binding b
JOIN deploy_app app s ON s.id = b.app_id AND s.deleted_at IS NULL
JOIN deploy_app_revision r ON r.id = s.current_revision_id
WHERE b.hostname_ascii = $1 AND b.environment = $2
```

`b.environment` filters the binding, but the revision comes from the single
app-level pointer `s.current_revision_id`, which is written whenever the runtime
assignments of *any* environment converge:

```rust
// .../runtime_assignments.rs:644-657
UPDATE deploy_app SET current_revision_id = $3 ... WHERE desired_revision_id = $3
```

Revision numbers are also app-global (`MAX(revision_no)+1 WHERE app_id`,
`app_composition.rs:828-842`), and `deploy_app_revision.environment` is only
consumed by the node-target runtime-set build
(`app_composition.rs:917-923`). Net effect: with five environments
(development/test/staging/demo/production) the app can hold exactly one "current
version"; `shop.app-dev.<suffix>` can serve the production descriptor and vice
versa, and the descriptor's embedded bindings/mounts are the ones compiled for
whichever environment published last.

Fix direction: resolve the revision per `(app, environment)` — from
`deploy_app_environment.current_release_id`, or from a new per-environment
revision pointer, or from the newest `VALID` revision whose `environment`
matches the matched binding (and filtering on `r.environment = $2`). The
`deploy_deployment` / `deploy_release` rows of that environment should be the
documented source of "the deployed version".

### P0-3 — Provider contract version mismatch across repositories

| Producer | Value |
| --- | --- |
| Deploy HTTP content-provider port | `sdkwork.drive.website-root.v1` (`sdkwork-deploy-content-provider-port/src/sdk.rs:167`) |
| Deploy memory content-provider port | `sdkwork.drive.website-root.v1` (`.../memory.rs:27`) |
| Deploy HTTP content-provider port (KB) | `sdkwork.knowledgebase.wiki-publication.v1` (`sdk.rs:209`) |
| Deploy memory content-provider port (KB) | `sdkwork.knowledgebase.wiki-publication.v1` (`memory.rs:38`) |
| Deploy compiler test fixture | `sdkwork.drive.website.v1` (`runtime-compiler/src/lib.rs:866`) |
| Web Server Drive provider (required) | `drive.website-root.v1` (`drive-provider/src/adapter.rs:26`) |
| Web Server Knowledgebase provider (required) | `knowledgebase.wiki-publication.v1` (`knowledgebase-provider/src/adapter.rs:22`) |

`validate_reference` rejects anything else:

```rust
// drive-provider/src/adapter.rs:333-346
if reference.provider_type != WebsiteProviderType::Drive
    || reference.provider_contract_version != DRIVE_WEBSITE_ROOT_PROVIDER_CONTRACT_VERSION
```

The Deploy compiler only validates that the field is non-empty and ≤64 bytes
(`runtime-compiler/src/lib.rs:585`), so the mismatch is written straight into
`descriptor_json` and surfaces as a `ContractMismatch` when the fallback site
serves Drive-backed static files — i.e. exactly the "resolve resources for that
version" step. The standalone profile enables the memory port
(`SDKWORK_DEPLOY_USE_MEMORY_CONTENT_PROVIDER=true`,
`etc/topology/standalone.production.env:42`), so this is the default local path.

Fix direction: one shared constant (or a versioned catalog validated by the
compiler) owned by a single side; make the compiler reject unknown provider
contract versions at compile time instead of failing at serve time.

### P1-4 — `demo` cannot be represented in the Deploy schema

- `etc/topology/standalone.demo.env` sets `SDKWORK_ENVIRONMENT=demo`,
  `SDKWORK_WEBSERVER_ENVIRONMENT=demo`, `SDKWORK_DEPLOY_ENVIRONMENT=demo`.
- Web Server `environment_name()` maps `WebsiteRuntimeEnvironment::Demo` to
  `"demo"` (`deploy_fallback.rs:95-103`).
- Deploy service accepts `demo` (`sdkwork-intelligence-deploy-service/src/app_domains.rs:11`).
- But the DDL CHECK constraints allow only four environments:
  `deploy_app_binding` (`:866`) and `deploy_app_revision` (`:990`) —
  `CHECK (environment IN ('development','test','staging','production'))`. The
  string `demo` does not appear anywhere in `0001_deploy_baseline.sql`.
- `app_domain_label("demo")` falls through to `"app"`
  (`sdkwork-deploy-core/src/app_domains.rs:36-43`), i.e. the same label
  production uses.

Net effect: in the demo deployment the fallback lookup queries
`environment='demo'` and can never match; `create_app` with
`default_environment=demo` passes service validation and then fails the binding
INSERT with a CHECK violation.

### P1-5 — The `http` lookup channel does not exist

`AppDomainFallbackLookup::Http { endpoint, auth_token_file }` is declared
(`config/model.rs:1729-1741`) and validated (`config/validate.rs:1000`), and the
tech doc documents it as the cloud control-plane channel, but
`build_deploy_fallback` ignores `config.lookup` entirely and always builds the
embedded repository adapter; without a shared pool it disables the fallback:

```rust
// website.rs:2591-2625
let Some(pool) = process_shared_database_pool() else {
    tracing::warn!("app-domain fallback is enabled but the shared database pool is unavailable; ...");
    return None;
};
let lookup = Arc::new(EmbeddedDeployServerLookup::new(DeployRepository::new_lookup(pool)));
```

`EmbeddedDeployServerLookup` (`deploy_fallback.rs:484`) is the only
`DeployServerLookup` implementation in either repository. An edge that does not
share the Deploy database silently loses the whole feature.

### P1-6 — Two binding truth sources that can disagree

- The lookup needs a `deploy_app_binding` row (`hostname_ascii`, `environment`,
  `status='ACTIVE'`) to find the app.
- Actually serving is decided by the compiled descriptor's own `bindings`
  (`website_runtime/compiled.rs:221-223`, `select_binding` at `:309-319`), which
  are compiled **only** from `command.request.bindings`
  (`app_composition.rs:118-119`, `insert_bindings` at `:632-696`).
- `provision_app_default_domains_repo` writes binding rows but is not part of
  any compilation input.

Therefore "the domain is ACTIVE in the control plane" does not imply "the
hostname is routable": a descriptor without a binding for the requested host
returns `Ok(None)` → `NotFound`, and the auto-provisioned rows are required for
the lookup while contributing nothing to what is served. The two sources must be
reconciled (compile the provisioned default bindings into the descriptor, or
make the descriptor's binding set authoritative for the lookup too).

### P1-7 — Platform suffix catalog drift (14 vs 16, `noaper.*`)

| Source | Count | `noaper.*` |
| --- | --- | --- |
| `sdkwork-deploy-core/src/app_domains.rs:15-30` | 14 | absent |
| Web Server `default_app_domain_suffixes()` (`config/model.rs:1768-1790`) | 16 | **present** |
| `etc/data-plane/website.cloud.config.json:86-103` | 14 | absent |
| `etc/topology/standalone.production.env:7` (CORS) and `deployments/webserver/nginx.*.conf` | 16 | **present** |
| `docs/architecture/tech/TECH-app-domain-publishing-fallback.md:19-22` | 14 | absent |

The Web Server default (used whenever `suffixes` is omitted) classifies
`<slug>.app.noaper.com` as a default app domain and the local nginx sidecar
serves `server.noaper.com`, but the Deploy control plane never provisions
`app.noaper.com` zones/domains — a guaranteed 404. Because
`classify_host`'s result is only a log label (see P2-11), the drift is currently
cosmetic for routing but is a documented-contract violation and will produce
misleading "default-app" log lines and TLS/DNS planning errors.

### P1-8 — App lifecycle state is not enforced by the lookup

`resolve_active_app_by_hostname_repo` filters `s.deleted_at IS NULL` but not
`s.app_status = 'ACTIVE'`. Compare `load_other_descriptors`
(`app_composition.rs:917-923`), which does filter `s.app_status = 'ACTIVE'`.
`PAUSED` / `ARCHIVED` apps keep serving through the fallback, while
`deploy_app.activated_at` / `paused_at` / `archived_at` are likewise ignored.
(`deploy_app_revision.validation_status = 'VALID'` *is* honoured.)

### P2-9 / P2-10 — The requested deploy_app surface does not exist

Requirement: each `deploy_app` carries a nginx-compatible conf, a domain, the
default `<appId>.app.<suffix>` hostname, plus a custom full domain and a custom
prefix replacing `<appId>`.

Reality:

- `deploy_app` (`0001_deploy_baseline.sql:1343-1381`) has no nginx-conf column,
  no domain column, no prefix column. Columns are `name`, `slug`, `app_kind`,
  `runtime_config`, `default_variant_id`, `current_revision_id`,
  `desired_revision_id`, `default_environment`, status timestamps.
- The nginx conf lives in a **separate table** `deploy_nginx_config`
  (`:146-181`, `app_id` + `domain_id` + `config_content`) — not a `deploy_app`
  field, and it has **no `environment` column**, so one conf cannot be scoped per
  lifecycle environment.
- The Web Server never reads `deploy_nginx_config` (grep: only the Deploy
  repository/service/routes reference it). It has its own near-identical table
  `webserver_nginx_config` (`database/ddl/baseline/postgres/0001_webserver_baseline.sql:298-319`,
  `site_id` instead of `app_id`, also no `environment`), written by
  `WebRepository::*_nginx_configs_repo` and materialised through the local edge
  runtime (`sdkwork-webserver-edge-runtime::deploy_nginx_config` /
  `EdgeRuntime::deploy_app_config(domain, content)`).

So the "local file configuration" the fallback is supposed to complement is a
third artifact set, and the deployment-side conf never reaches the edge.
Target: a single authority — either `deploy_app.nginx_conf` (+ environment) read
by the fallback, or the Deploy nginx table promoted into the lookup with an
`environment` + `hostname` key and consumed by the data plane.

Domain configuration: `deploy_domain` + `deploy_app_binding` already support an
arbitrary **custom full domain** (WILDCARD/EXACT + verification,
`tests/platform_app_domains.rs:200-254`), so that requirement is met. The
**custom prefix replacing the app label** is not: `default_app_hostname` only
concatenates `slug` (`deploy-core/src/app_domains.rs:47-49`),
`parse_default_app_hostname` only accepts the four fixed labels
(`:65-98`), and no `deploy_app` column can carry an alternative label. Note the
doc's own wording "app slug (app id)" vs. the requested "appId = deploy_app.id":
the hostname label is the **slug**, not the `uuid`, and the slug must be
globally unique for the default catalog or app creation fails with a conflict
(`platform_app_domains.rs:176-190`) — another reason a custom prefix / short
stable id is needed.

### P2-11 — `classify_host` / `HostClass` is dead logic

`classify_host` returns `DefaultApp { slug, suffix }` or `Custom`
(`deploy_fallback.rs:79-93`), but the only consumer is the log label
(`:211`, `class_label` at `:459-464`). Both classes call the same
`lookup.resolve_server(hostname, environment)`. The parsed slug/suffix are never
used, and the platform label set is hardcoded to `app|app-dev|app-test|app-staging`.
Either delete the classification or make it load-bearing (it is the natural place
to implement custom-prefix parsing).

### P2-12 — Default domains are provisioned for one environment only

`create_app` calls `provision_app_default_domains(context, &app.id,
&app.default_environment)` (`app_delivery.rs:129-132`). No other call site
exists, so `app-dev` / `app-test` / `app-staging` (and the impossible `demo`)
hostnames are never auto-created; the tech doc's per-environment hostname table
implies otherwise.

### P2-13 — Path is not part of the resolution API

`DeployServerLookup::resolve_server(hostname, environment)` has no path, and the
SQL orders by `b.id DESC LIMIT 1`, so with several `path_prefix` bindings on one
hostname only the newest row is returned; `ResolvedDeployServer::path_prefix` and
`action_type` are then ignored by the resolver (routing comes from the descriptor
instead). Benign today because the descriptor carries the binding set, but the
API contract is ambiguous and will break if the descriptor ever lags the DB.

### P2-14 — Local host hit + missing file does not fall back (app-config path)

- website-runtime path: any `404` from `serve_website_request` triggers the
  fallback (`handler.rs:271-297`).
- app-config path: the fallback only runs when `select_route` returns `None`
  (`handler.rs:312-338`). A host declared in local config whose static root or
  provider resource is missing returns 404/500 (`handler.rs:558-607`) without
  consulting Deploy.

The requirement is literally "no matching server configuration", so this matches
the letter of the ask, but the two paths are semantically inconsistent and a
locally-declared-but-broken host can never recover.

### P2-15 — Stale tests and docs vs. the DDL

- `crates/sdkwork-intelligence-deploy-repository-sqlx/tests/platform_app_domains.rs:21-42`
  seeds `deploy_app` with `site_type`, `status`, and `app_id` columns — none of
  which exist in `0001_deploy_baseline.sql` — and omits the NOT NULL, defaultless
  `app_kind`. The same pattern appears in
  `tests/runtime_assignment_outbox.rs:267`. These are the only PG integration
  tests that exercise the fallback lookup, and they cannot run against the
  current schema.
- `docs/migrations/MIG-2026-0070-application-resource-model.md:55-56` claims
  `webserver_application.site_id` mirrors "the `deploy_app.site_id` model" — `deploy_app`
  has no `site_id` column (the `deploy_site` table was folded away by migration
  `0007_deploy_app_delivery`; only a `-- source: migrations/001_create_deploy_site.sql`
  comment remains).

## 4. Requirement Conformance

All rows met after the resolution in §0.

| Requirement | Status | Evidence |
| --- | --- | --- |
| Local server-config miss falls back to sdkwork-deployments | Met | `handler.rs` website-runtime 404 hook + app-config hook (`select_route` miss and resource-backed 404) |
| Find the `deploy_app` matching the requested domain | Met (exact hostname + environment + `app_status`) | `platform_app_domains.rs::resolve_active_app_by_hostname_repo` |
| Resolve the deployed version of that `deploy_app` and serve its resources | Met | per-`(app, environment)` newest `VALID` revision; canonical provider contract versions enforced at compile time; asserted by `tests/app_domain_fallback_contract.rs` |
| Each `deploy_app` carries a nginx-compatible conf in a field | Met | `deploy_app.nginx_conf` / `nginx_conf_sha256` / `nginx_conf_updated_at`, environment/hostname overrides via `deploy_nginx_config`, materialized by `EdgeNginxSiteSink` |
| `deploy_app` supports configuring its domain | Met | `deploy_domain` + `deploy_app_binding` (custom full domain) and `deploy_app.app_domain_label` / `app_domain_suffixes` |
| Default `appId.app.xxx.com` resolution | Met | `<appLabel>.app[-<env>].<suffix>` over the single 14-suffix catalog, provisioned for all five environments |
| Custom full domain | Met | domain verification + binding |
| Custom prefix replacing the app label | Met | `deploy_app.app_domain_label` (any DNS label, uuid included), parsed by the load-bearing `classify_host` |

## 5. Suggested Order Of Work

1. P0-3 provider contract version: single constant + compiler-time rejection.
   Lowest effort, unblocks all Drive-backed fallback serving.
2. P0-1 environment-scoped reconcile instead of delete-all + re-provision default
   domains in the same transaction.
3. P0-2 per-environment revision resolution (binding environment → revision
   environment / `deploy_app_environment.current_release_id`).
4. P1-4/5/7/8: add `demo` (or drop it) consistently across the DDL CHECKs and
   `app_domain_label`; implement or remove the `http` lookup channel; align the
   suffix catalog to one source; add `app_status='ACTIVE'` to the lookup.
5. P2-9/10: decide the single nginx-conf authority and add the
   `deploy_app` domain/prefix fields (or an explicit `deploy_app_domain` child
   table) before implementing custom-prefix parsing in `classify_host`.
6. P2-15: repair the two stale PG test seeds and the MIG-2026-0070 statement.

## 6. Verification Expectations

- Deploy: `cargo test -p sdkwork-deploy-core app_domains`;
  `cargo test -p sdkwork-deploy-runtime-compiler`;
  PG integration `tests/platform_app_domains.rs` (after the seed fix) plus a new
  case asserting that a composition replace preserves other environments'
  bindings and the auto-provisioned default domains.
- Web Server: `cargo test -p sdkwork-webserver-core --test webserver_config
  app_domain_fallback`; `cargo test -p sdkwork-api-webserver-standalone-gateway
  --lib deploy_fallback`; a cross-repository contract test that feeds a
  Deploy-compiled descriptor into the Web Server drive/knowledgebase providers.
- Five-environment smoke: `*.app.<suffix>` (production) and `*.app-dev.<suffix>`
  (development) must serve their own revision, and `demo` must either work
  end-to-end or be rejected explicitly.
