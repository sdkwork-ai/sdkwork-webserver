# SDKWork Web Server Source Configuration

`etc/sdkwork.deployment.config.json` is the source configuration entrypoint. It identifies
`sdkwork-webserver`, links `../specs/topology.spec.json`, and maps the supported profiles to
tracked environment files:

| Profile | Source |
| --- | --- |
| `standalone.development` | `topology/standalone.development.env` |
| `standalone.test` | `topology/standalone.test.env` |
| `standalone.staging` | `topology/standalone.staging.env` |
| `standalone.production` | `topology/standalone.production.env` |
| `cloud.development` | `topology/cloud.development.env` |
| `cloud.test` | `topology/cloud.test.env` |
| `cloud.staging` | `topology/cloud.staging.env` |
| `cloud.production` | `topology/cloud.production.env` |

`sdkwork.app.config.json` owns application identity and release declarations only. Concrete binds,
origins, API surface URLs, database selection, upstream targets, and deployment profile values are
owned by `etc/` and `specs/topology.spec.json`.

## Adaptive Web (process-owned)

Edge nginx for this product uses `deploy.yaml` `expose.mode: api` and reverse-proxies all public
paths to the gateway. Console PC/H5 selection lives in the process (`AdaptiveAppShell`):

| Root | Env / TOML |
| --- | --- |
| PC SPA | `SDKWORK_WEBSERVER_PC_STATIC_ROOT` / `[app_roots].pc_static_root` or `pc_static_by_environment` |
| H5 SPA | `SDKWORK_WEBSERVER_H5_STATIC_ROOT` / `[app_roots].h5_static_root` or `h5_static_by_environment` |
| Ordinary static | `SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT` / `[app_roots].static_fallback_root` or by-environment map |
| Tablet preference | `SDKWORK_WEBSERVER_TABLET_SURFACE` / `[app_roots].tablet_surface` (`pc` default, or `h5`) |

Source builds land in `apps/sdkwork-webserver-{pc,h5}/dist/{standalone,cloud}/{dev,test,staging,prod}/` (standalone is the default profile).
Install layout: `/usr/share/sdkwork/webserver/web/{pc,h5,static}/`. See
`etc/examples/config.toml.example` for the full per-environment catalog.

### Development Adaptive Web (one browser origin)

Development mirrors nginx Adaptive Web: **one** browser-visible origin selects PC/H5 by
device class (`APP_RUNTIME_TOPOLOGY_SPEC.md` §8.2):

| Role | Env | Example |
| --- | --- | --- |
| Browser-visible ingress | `SDKWORK_WEBSERVER_WEB_DEV_INGRESS_BIND` | `127.0.0.1:5182` |
| Private PC Vite renderer | `SDKWORK_WEBSERVER_PC_INTERNAL_DEV_PORT` | `5184` |
| Private H5 Vite renderer | `SDKWORK_WEBSERVER_H5_INTERNAL_DEV_PORT` | `5185` |

Open only `http://127.0.0.1:5182` in the browser. Do **not** use separate PC/H5 public ports
(`SDKWORK_WEBSERVER_PC_DEV_BIND` / `H5_DEV_BIND` are retired for adaptive profiles).

## Development Profiles

`pnpm dev` and `pnpm dev:standalone` select `standalone.development` with runtime target `server`.
The plan starts the application-owned standalone gateway on `127.0.0.1:3800`.

`standalone.test` is the test-environment installer profile: the gateway public ingress binds
`0.0.0.0:8888` and the host is bound to `server-test.sdkwork.com` (the test `.deb` package writes
`/etc/hosts`; see `docs/guides/operator/deb-install.md`). It uses the Let's Encrypt staging
directory and the `sdkwork_ai_test` database. `standalone.production` binds `0.0.0.0:8080` and
`server.sdkwork.com` with ACME production issuance and the `sdkwork_ai_prod` database.

`pnpm dev:cloud` selects `cloud.development` with runtime target `server`. The plan starts only the
local `sdkwork-webserver-node-daemon` client and resolves the deployed development surfaces from explicit
`https://*-dev.sdkwork.com` URLs. It does not start a gateway, API listener, database, migration,
seed process, or deployed-service worker.

`node-daemon/development.env.example` is the canonical non-secret Node Daemon environment example.
`agent/development.env.example` and `SDKWORK_WEBSERVER_AGENT_*` remain wire/runtime compatibility aliases
for the v3 Agent contract; conflicting canonical and compatibility values fail startup.

`worker/development.env.example` configures the durable certificate operation worker. API issue and
renew commands persist work before returning `202`; the worker claims that work with an expiring
lease and fencing token, executes bounded ACME/material activation through the service, and writes
the terminal aggregate state transactionally. Each replica uses a distinct stable
`SDKWORK_WEBSERVER_CERT_WORKER_ID`. Browser polling cancellation only stops the client query loop and does
not cancel persisted server work.

## Runtime And Secrets

Authority: [`sdkwork-specs/APPLICATION_DEPLOY_LAYOUT_SPEC.md`](../../sdkwork-specs/APPLICATION_DEPLOY_LAYOUT_SPEC.md).

Installed Linux default: `/etc/sdkwork/webserver/config.toml` + `sdkwork.webserver.config.json` + `secrets/`.

### Module Imports (`imports.d/`, nginx include style)

Sibling-module `deployments/webserver/` imports are declared **one file per
module** under `/etc/sdkwork/webserver/imports.d/<module-id>.toml` and loaded
through the runtime config `[webserver] include` pattern (nginx-style):

```toml
[webserver]
include = ["/etc/sdkwork/webserver/imports.d/*.toml"]
```

Each included file carries a `[[webserver.imports]]` entry (`id`, `path`,
`enabled`, `required`, `probe_upstreams`); relative `path` values resolve
against the runtime config directory. Include semantics mirror nginx: matched
files load in sorted order after the inline `[[webserver.imports]]` entries, a
later same-`id` entry replaces the earlier one, a glob that matches nothing is
skipped, and an explicit (non-glob) pattern naming a missing file fails
startup. Glob matching is limited to the final path component (`*`, `?`).
Environment selection per import (`development`/`test`/`staging`/`production`)
is unchanged: each import loads its `server.<environment>.toml` for the active
`SDKWORK_WEBSERVER_ENVIRONMENT` (`SDKWORK_WEBSERVER_SPEC.md` §2.2 / §17).

The Docker standalone entrypoint generates these files automatically from the
sdkwork-space checkout; the `.deb` installer creates the (empty) directory and
include pattern so operators can drop in module import files.

### Merged Module Data Plane (`serve-imports`)

At startup the gateway merges every enabled module import into **one
data-plane configuration** (`merge(common, server.<environment>.toml,
server.<profile>.toml)` per module, standard layout v3 merge semantics) and
serves it with the `serve-imports` operation: module domains
(`[[http.server]] serverName`), servers (`listen`), and resources
(locations/upstreams/static roots) become live listeners with Host/SNI
routing. The Docker standalone entrypoint runs the management API in the
background and `serve-imports` in the foreground when `imports.d/` is
non-empty.

- **Listener port remap** — declared module ports stay authoritative; the
  container binds unprivileged ports via
  `SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS` (default `80=8080,443=8430`,
  comma-separated `declared=actual` pairs).
- **Bootstrap certificates** — when a referenced certificate has no material
  on disk, the runtime auto-generates a self-signed placeholder (SAN covers
  the certificate's server names plus a `*.<name>` wildcard so one
  placeholder stays valid across lifecycle environments) and logs a warning;
  operators replace it with ACME-issued or uploaded material without a
  restart. A half-present pair (certificate without key or vice versa) fails
  closed with a precise diagnostic.
- **Certificates inventory** — `list-import-certificates` prints the merged
  certificate names for provisioning; material lives under
  `/etc/sdkwork/certs/letsencrypt/<name>/` (or `certs://` inventory paths).

The data-plane JSON file is discovered in this order: an explicit config argument, then
`SDKWORK_WEBSERVER_SERVER_CONFIG_FILE`, then the canonical OS system-scope directory for
application code `webserver` joined with `sdkwork.webserver.config.json`: Linux
`/etc/sdkwork/webserver`, macOS `/Library/Application Support/sdkwork/webserver`, Windows
`%ProgramData%\sdkwork\webserver`. A missing canonical default fails closed with the expected
path and override variable.

Tracked files contain no access tokens, Node Tokens, passwords, private keys, or database
credentials. Use process environment overrides, protected secret files, or the deployment
platform's secret manager. Local overrides and materialized runtime state belong under ignored
`.sdkwork/runtime/` or approved operator paths; they must not be committed.

Production images carry the fail-closed website listener base policy at
`/app/etc/data-plane/website.cloud.config.json`; it trusts no forwarding metadata. Kubernetes
renders the reviewed direct-ingress CIDRs into an immutable per-Node ConfigMap mounted at
`/etc/sdkwork/webserver/sdkwork.webserver.config.json`. Mutable Node identity, provider-event subscriptions,
and credentials are mounted read-only under `/run/secrets/sdkwork-webserver-node/`. The Kubernetes
migration Job obtains database URLs, independent encryption roots, and the ACME contact through the
`sdkwork-webserver-runtime` secret reference documented in `../deployments/kubernetes/README.md`.

`examples/sdkwork.webserver.config.json` is the safe standalone data-plane example. It is validated
against `../specs/sdkwork.webserver.config.schema.json`; certificate and private-key values are file
references rather than embedded secrets.

`data-plane/website.development.env.example` is the non-secret standalone/development
website/Wiki data-plane example and explicitly selects the `file` assignment source.
`data-plane/website.cloud.env.example` is the production cloud fragment and selects the
authenticated Web Internal API assignment source. Both examples point credentials at protected
secret files; no credential value belongs in source configuration. Each data-plane process is
explicitly bound to one Web Node identity and one 64-character
`tenantScopeHash`; its provider credentials must authorize that same tenant, and a candidate
runtime-set containing another or multiple tenant scopes is rejected before activation. The token
files contain only deployment-provided ingress tokens and must never be committed. Production and
staging provider origins must use HTTPS. Provider resources are validated before initial activation
and every watched update with bounded concurrency; a failure retains the last-known-good set.
`SDKWORK_WEBSERVER_WEBSITE_PROVIDER_BUFFERED_CONTENT_BYTES` bounds the aggregate provider content bytes
admitted by one process while Drive or Knowledgebase generated SDK responses remain live. It is a
strict integer from 16777216 through 2147483648 bytes and defaults to 268435456 bytes. Admission is
conservative: every content request reserves the compiled route's `maximumObjectBytes`, so
under-reported metadata and `If-Range` fallback cannot weaken the bound. Saturation fails
immediately with a retryable unavailable result and
does not create a memory waiter queue. The permit remains owned by the HTTP response stream and is
released on completion, stream failure, or cancellation. This is a process memory-amplification
guard for the current generated `Vec<u8>` transports, not a claim of end-to-end provider streaming.
`SDKWORK_WEBSERVER_WEBSITE_PROVIDER_RESOLUTION_CACHE_ENTRIES` bounds the node-local Provider resolution
metadata cache. It is a strict integer from 1 through 1048576 and defaults to 16384. The cache stores
only public static/Wiki resolution metadata, Wiki redirects, and non-disclosing negatives; it never
stores response bytes, credentials, private/draft content, conditional responses, or activation
probes. Descriptor TTLs control positive, negative, and positive-only stale windows. Bounded
single-flight coalesces identical misses, LRU bounds retained entries, and capacity saturation
bypasses the cache without creating an origin waiter queue. Provider events invalidate this same
cache by exact path, Provider resource, or Provider type, with an epoch fence preventing stale
in-flight reinsertion. The loopback operations listener exports capacity, entries, in-flight work,
lookup outcomes, writes, evictions, revalidations, and invalidations through the fixed-cardinality
`sdkwork_web_data_plane_provider_resolution_cache_*` Prometheus family.
`SDKWORK_WEBSERVER_WEBSITE_RUNTIME_SET_RECOVERY_DIRECTORY` owns a dedicated node-local A/B slot
directory containing only complete, hash-verified `sdkwork.website-runtime-set.v1` snapshots.
Staging and production require this directory. Bootstrap selects the highest valid generation from
the source and recovery state, rejects same-generation hash conflicts and node/environment scope
mismatches, and can restart from the recovered snapshot while the source is unavailable. A source
older than the recovered generation cannot lower the replay barrier. Successful initial and watched
activations persist the inactive slot with bounded asynchronous I/O before the update is considered
durable. The directory is node data-plane state, not Web business persistence or a substitute for
authenticated Deploy runtime-set distribution; it must be writable only by the service identity,
must not share files with another subsystem, and belongs on durable host storage.

TLS termination is selected independently with `SDKWORK_WEBSERVER_TLS_RUNTIME_SOURCE`. `external` means
the reviewed load balancer, CDN, or ingress terminates TLS and is the explicit setting in the
current cloud profiles. `file` enables native Rustls termination and requires a listener declaring
`"tlsRuntime": "assignment"`, plus `SDKWORK_WEBSERVER_TLS_RUNTIME_SNAPSHOT_FILE`,
`SDKWORK_WEBSERVER_TLS_MATERIAL_ROOT`, and `SDKWORK_WEBSERVER_TLS_LISTENER_ID`. The snapshot follows
`../specs/sdkwork.tls-runtime.snapshot.schema.json`; every `materialReference` must be
`file:<opaque-version-id>` and resolves only to
`<material-root>/<opaque-version-id>/fullchain.pem` and `privkey.pem` after canonical boundary
checks. Snapshot JSON never contains PEM, a filesystem path, URL, token, or key.

`SDKWORK_WEBSERVER_TLS_RUNTIME_POLL_INTERVAL_MS` is bounded to 250..60000 milliseconds. Candidate
snapshots are schema/hash/node/policy checked before any material work, unchanged hashes skip
certificate parsing, and changed candidates validate SAN coverage, current validity, declared
validity evidence, leaf SHA-256, key match, SNI ownership, TLS version range, and listener ALPN.
Only a complete candidate replaces the shared Rustls context; existing connections keep their
original context and a rejected candidate leaves last-known-good active. Native TLS in staging and
production additionally requires `SDKWORK_WEBSERVER_TLS_RUNTIME_RECOVERY_DIRECTORY`, an exclusive
node-local A/B directory persisted before activation and used for restart recovery. The recovery
slots contain only bounded hash-verified snapshots; certificate material remains in the protected
material provider root. `data-plane/website.native-tls.config.json` and
`data-plane/website.native-tls.development.env.example` are the non-secret native TLS examples.

`data-plane/website-provider-events.development.json.example` is the provider-event ingress
instance selected by `SDKWORK_WEBSERVER_WEBSITE_PROVIDER_EVENT_CONFIG_FILE` and validated by
`../specs/sdkwork.website-provider-event-ingress.schema.json`. It binds only to loopback, maps each
subscription to an expected provider/channel/tenant/organization, references a protected signing
secret file, and writes dual-slot per-stream checkpoints under ignored runtime state. Drive accepts
only `/nodes/{nodeUuid}/provider-events/drive-website-events`, requires the path Node to match the
configured active Node, derives each channel verification token from the Node derivation secret,
and then derives the owner signing key. The unqualified `/provider-events/{subscriptionId}` route
accepts Knowledgebase only; Knowledgebase uses its outbox webhook secret directly.
Production and staging place an authenticated internal HTTPS ingress or sidecar in front of this
loopback listener and preserve the complete path; the public website listener never mounts
provider-event routes. Both owner
webhooks sign `delivery-time + "." + exact-body`, and the receiver enforces the configured clock
window before strict AsyncAPI parsing. A production/staging website runtime-set that uses either
provider fails bootstrap when this event-ingress configuration is absent.

The website data plane starts with:

```powershell
cargo run -p sdkwork-webserver-website-delivery-edge-runtime
```

The dedicated edge runtime loads `SDKWORK_WEBSERVER_SERVER_CONFIG_FILE` for listener/TLS limits and the assignment
source selected by `SDKWORK_WEBSERVER_RUNTIME_ASSIGNMENT_SOURCE` for immutable
Site/Binding/Variant/Mount routing. `cloud` is the production source: the generated Web Internal
SDK authenticates with the secret-file `SDKWORK_WEBSERVER_NODE_TOKEN_FILE`, conditionally pulls the
current assignment for `SDKWORK_WEBSERVER_NODE_UUID` and
`SDKWORK_WEBSERVER_WEBSITE_RUNTIME_ENVIRONMENT`, verifies assignment identity/hash and the complete
runtime-set, and reports `RECEIVED`, `VALIDATED`, `STAGED`, `ACTIVE`, or bounded `REJECTED`
observations. `file` is limited to standalone/development and reads
`SDKWORK_WEBSERVER_WEBSITE_RUNTIME_SET_FILE`. Both modes retain the durable last-known-good runtime-set
when an update is invalid, stale, terminally rejected, or requires an unavailable provider, and
recover it after restart when the source is temporarily unavailable. A cloud node with a valid
last-known-good snapshot can start during a temporary control-plane outage; a first-start node
without one fails closed.

For an HTTP listener behind a TLS terminator, the runtime uses `X-Forwarded-Proto` only when the
immediate TCP peer is covered by `trustedProxy.trustedCidrs`. It accepts exactly one `http` or
`https` value; duplicates, lists, whitespace variants, non-text values, and oversized trusted
headers fail with `400`. Untrusted peers cannot override the listener transport, and native TLS
cannot be downgraded by forwarding metadata.

## Validation

```powershell
node ..\sdkwork-specs\tools\check-source-config-standard.mjs --root .
pnpm topology:validate
pnpm exec sdkwork-app doctor
cargo run -p sdkwork-api-webserver-standalone-gateway -- validate etc/examples/sdkwork.webserver.config.json
```

Use `pnpm release:package:standalone` or `pnpm release:package:cloud` only on a Linux runner whose
architecture matches `SDKWORK_PACKAGE_ARCHITECTURE`. Release declarations remain disabled in the
application manifest until release evidence and publication authority are approved.

<!-- SDKWORK-DEPLOY-LAYOUT: v1 -->
## Installed Runtime Paths

Authority: `APPLICATION_DEPLOY_LAYOUT_SPEC.md` (`../sdkwork-specs/`).

| Item | Value |
| --- | --- |
| `appId` | `sdkwork-webserver` |
| `runtimeCode` | `webserver` |
| Config root | `/etc/sdkwork/webserver/` |
| Runtime TOML | `/etc/sdkwork/webserver/config.toml` |
| Secrets | `/etc/sdkwork/webserver/secrets/` |
| Override | `SDKWORK_WEBSERVER_CONFIG_FILE` |

Source profiles live under `etc/` (`sdkwork.deployment.config.json` index). Deploy manifest: `deployments/deploy.yaml`. Web data-plane source: `deployments/webserver/` (`SDKWORK_WEBSERVER_SPEC.md` layout v3).

```bash
node ../sdkwork-specs/tools/check-source-config-standard.mjs --root .
node ../sdkwork-specs/tools/check-application-deploy-layout.mjs --root .
node ../sdkwork-specs/tools/check-webserver-toml-standard.mjs --root deployments/webserver
```
<!-- /SDKWORK-DEPLOY-LAYOUT -->


