# Web Server Deploy Configuration

Authority: `SDKWORK_WEBSERVER_SPEC.md`, `NGINX_SPEC.md`, `SDKWORK_DEPLOY_SPEC.md`.

| File | Role |
| --- | --- |
| `server.common.toml` | Shared baseline (hosts, certificates, locations, upstream skeleton) — reverse-proxy only for this product |
| `server.standalone.toml` | Standalone upstream targets (`127.0.0.1:3800` = topology `defaults.gatewayBind`) |
| `server.cloud.toml` | Cloud upstream targets (platform `sdkwork-api-cloud-gateway`) |
| `nginx.standalone.conf` | Rendered sidecar of the standalone merge (do not hand-edit) |
| `nginx.cloud.conf` | Rendered sidecar of the cloud merge (do not hand-edit) |
| `app-roots.example.toml` | Process Adaptive Web `[app_roots]` catalog (copy into runtime `config.toml`; not nginx) |
| `static/` | Process `static-fallback` content packaged to `/usr/share/sdkwork/webserver/web/static/` |

## Nginx surface (`[nginx]`)

Canonical block:

```toml
[nginx]
enabled = true
profile = "http-core-v1"
unknownDirectivePolicy = "error"
strict = true
confFile = "nginx.conf"
```

- Root `enabled = false` turns the whole module web surface off (placeholders omit `[nginx]`).
- `[nginx].enabled` (`nginx.enabled`) gates sidecar W16 and nginx.conf import activation only.
- `strict` / `confFile` are deploy-validator keys (W16); the Rust `NginxConfig` runtime model carries `enabled` / `profile` / `unknownDirectivePolicy`.

Gap catalog (implemented / partial / missing vs `http-core-v1`):
[`specs/nginx-gap.catalog.json`](../../specs/nginx-gap.catalog.json).
Checks: `pnpm check:nginx-gap`, `pnpm check:webserver-toml`, `pnpm check:webserver-toml:all`.

Adaptive Web nginx snippets for **other** modules (`mode: web` / `web+api`) live in
`sdkwork-specs/examples/webserver/adaptive-snippets/` — not in this product tree
(validator W23).

## Edge nginx is reverse-proxy only

`deploy.yaml` public-ingress expose items use `mode: api`. Edge nginx for
`server.sdkwork.com` (and non-production public-ingress hosts) terminates TLS
when configured and reverse-proxies **all** public paths — including `/` —
to the `gateway` upstream (`sdkwork-webserver` process).

Console Adaptive Web (mobile → H5 → PC → static; desktop → PC → H5 → static)
is owned by process `AdaptiveAppShell` via:

| Surface | Env / TOML |
| --- | --- |
| PC SPA | `SDKWORK_WEBSERVER_PC_STATIC_ROOT` / `[app_roots].pc_static_root` or `[app_roots.pc_static_by_environment]` |
| H5 SPA | `SDKWORK_WEBSERVER_H5_STATIC_ROOT` / `[app_roots].h5_static_root` or `[app_roots.h5_static_by_environment]` |
| Ordinary static | `SDKWORK_WEBSERVER_STATIC_FALLBACK_ROOT` / `[app_roots].static_fallback_root` or by-environment map |
| Tablet preference | `SDKWORK_WEBSERVER_TABLET_SURFACE` / `[app_roots].tablet_surface` (`pc` default, or `h5`) |

Source builds: `apps/sdkwork-webserver-{pc,h5}/dist/{dev,test,staging,prod}/`.
Installed: `/usr/share/sdkwork/webserver/web/{pc,h5,static}/`. Same public origin;
device class selects the terminal root (`SDKWORK_WEBSERVER_SPEC.md` §13.6).

Packaged topology profiles declare one `gateway-static` delivery per architecture
(`pc-web` and `h5`) with the matching `runtimeRootEnv`; the process selects the
surface at request time (`SDKWORK_DEPLOY_SPEC.md` §8 exception).

## Host Registry

Role host `web` (`applicationCode` remains `webserver`):

| Environment | public-ingress | app-http | backend-http |
| --- | --- | --- | --- |
| production | `server.sdkwork.com` | `server-app.sdkwork.com` | `server-admin.sdkwork.com` |
| development | `server-dev.sdkwork.com` | `server-app-dev.sdkwork.com` | `server-admin-dev.sdkwork.com` |
| test | `server-test.sdkwork.com` | `server-app-test.sdkwork.com` | `server-admin-test.sdkwork.com` |
| staging | `server-staging.sdkwork.com` | `server-app-staging.sdkwork.com` | `server-admin-staging.sdkwork.com` |

Retired nicknames (must not reappear): `server.sdkwork.com`, `web-*.sdkwork.com`, `testserver.sdkwork.com`.

## Validation

```bash
node ../sdkwork-specs/tools/check-webserver-toml-standard.mjs --root .
# or: pnpm check:webserver-toml
```

## Sidecar Regeneration

```bash
pnpm nginx:render
```

Edge nginx site install path (reverse-proxy / operator sites): `/etc/nginx/sites-enabled/sdkwork/<domain>.conf` (`NGINX_SPEC.md`).

## Imported Sibling Modules

The standalone gateway can validate other SDKWork modules' layout-v2
`deployments/webserver/` directories before startup.

### Runtime TOML (`config.toml`)

Relative paths (resolved from the runtime config file, app root, or cwd):

```toml
[[webserver.imports]]
id = "iam"
path = "../sdkwork-iam/deployments/webserver"
required = true
probe_upstreams = true

# Module root is also accepted; `deployments/webserver/` is discovered automatically.
[[webserver.imports]]
id = "commerce"
path = "../sdkwork-commerce"
required = true
```

Absolute paths (used as-is; module roots are auto-discovered the same way):

```toml
[[webserver.imports]]
id = "iam"
path = "/opt/sdkwork/sdkwork-iam/deployments/webserver"

[[webserver.imports]]
id = "commerce"
path = "E:/sdkwork-space/sdkwork-commerce"
```

### Environment override

```bash
# Comma-separated id=path pairs (relative or absolute)
export SDKWORK_WEBSERVER_MODULE_IMPORTS="iam=../sdkwork-iam,commerce=/opt/sdkwork/sdkwork-commerce"

# Or JSON array (same fields as runtime TOML entries; preferred for Windows absolute paths)
export SDKWORK_WEBSERVER_MODULE_IMPORTS='[{"id":"iam","path":"E:/sdkwork-space/sdkwork-iam"}]'
```

Paths may be **absolute** or **relative**. Relative paths are tried against the
runtime config directory, `SDKWORK_APP_ROOT`, `SDKWORK_WEBSERVER_APP_ROOT`, and
the process working directory. Each value may point at `deployments/webserver/`
or at a module repository root containing that directory. Each import is materialized with
`load_server_toml_app` for the active deployment profile (`standalone` or `cloud`).
Required imports fail closed when layout-v2 validation or upstream TCP probing fails.

```bash
cargo run -p sdkwork-api-webserver-standalone-gateway -- validate-module-imports
```
