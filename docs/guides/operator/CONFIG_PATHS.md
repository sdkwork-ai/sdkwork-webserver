# Web Server Configuration Paths (Operator)

**Authority:** [`sdkwork-specs/APPLICATION_DEPLOY_LAYOUT_SPEC.md`](../../../sdkwork-specs/APPLICATION_DEPLOY_LAYOUT_SPEC.md),
[`RUNTIME_DIRECTORY_SPEC.md`](../../../sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md) §4.1,
[`SDKWORK_WEBSERVER_SPEC.md`](../../../sdkwork-specs/SDKWORK_WEBSERVER_SPEC.md) §13.6,
[`PACKAGING_SPEC.md`](../../../sdkwork-specs/PACKAGING_SPEC.md) §5.5.

Two configuration systems work together:

| System | Role | Where |
| --- | --- | --- |
| **SDKWork process runtime** | Adaptive Web PC/H5, API, secrets, DB, ACME | `/etc/sdkwork/webserver/config.toml` (+ env overrides) |
| **Nginx / nginx-compat edge** | TLS terminate, reverse-proxy, stream | `deployments/webserver/server.*.toml` → sidecars, or stock `/etc/nginx/sites-enabled/sdkwork/*.conf`, or `serve-nginx` |

Edge stays proxy-only for this product (`expose.mode: api`). Built-in console pages are served by process `AdaptiveAppShell`, not by nginx `root`.

---

## 1. Package families

| Family | Builder | Artifact location | Install |
| --- | --- | --- | --- |
| Release tarball (standalone) | `pnpm release:package:standalone` / `scripts/webserver-release.mjs` | `dist/release/sdkwork-webserver-linux-*-standalone-server-*.tar.gz` | Input to deb/rpm/docker image |
| Ubuntu/Debian `.deb` | `pnpm release:deb:test` / `release:deb:production` | `dist/installers/sdkwork-webserver{,-test}_*_amd64.deb` | `sudo apt install ./….deb` |
| RHEL-family `.rpm` | `pnpm release:rpm:test` / `release:rpm:production` | `dist/installers/sdkwork-webserver{,-test}-*.x86_64.rpm` | `sudo rpm -Uvh …` / `dnf install` |
| Docker standalone image | `scripts/docker/build-standalone-image.mjs` + compose | `registry…/sdkwork-webserver-standalone:<tag>` | `deployments/docker/docker-compose.*.yml` |

Environment packages (same layout, different ingress/DB):

| Package | Environment | Domain | Process bind | Host / edge |
| --- | --- | --- | --- | --- |
| `sdkwork-webserver-test` | test | `server-test.sdkwork.com` | `0.0.0.0:8888` | direct or `:80` → `8888` |
| `sdkwork-webserver` | production | `server.sdkwork.com` | `0.0.0.0:8080` | nginx `:443` → `8080` |

Guides: [`deb-install.md`](./deb-install.md), [`bare-metal-install.md`](./bare-metal-install.md), [`WSL_DOCKER_DEPLOY.md`](./WSL_DOCKER_DEPLOY.md).

---

## 2. Linux FHS install layout (`applicationCode` / runtime code `webserver`)

| Purpose | Path | Owner (typical) |
| --- | --- | --- |
| **Runtime TOML (primary)** | `/etc/sdkwork/webserver/config.toml` | `root:sdkwork` `0640` |
| Legacy/migration TOML | `/etc/sdkwork/webserver/sdkwork-webserver.toml` | `root:sdkwork` |
| Data-plane JSON (optional) | `/etc/sdkwork/webserver/sdkwork.webserver.config.json` | `root:sdkwork` |
| Secrets directory | `/etc/sdkwork/webserver/secrets/` | `sdkwork:sdkwork` `0750` |
| DB password | `/etc/sdkwork/webserver/secrets/database.secret` | `0600` |
| Encryption master key | `/etc/sdkwork/webserver/secrets/encryption-key` | `0600` |
| Deploy encryption key | `/etc/sdkwork/webserver/secrets/deploy-encryption-key` | `0600` |
| Internal API tokens | `/etc/sdkwork/webserver/secrets/*-internal-api-ingress-token` | `0600` |
| Credential-entry bootstrap token | `/etc/sdkwork/webserver/secrets/credential-entry-bootstrap-access-token` | `0600` |
| Binaries + module assets | `/usr/lib/sdkwork/webserver/` | `root:root` |
| Gateway binary | `/usr/lib/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway` | `0755` |
| App identity | `/usr/lib/sdkwork/webserver/sdkwork.app.config.json` | |
| DB contract (Web) | `/usr/lib/sdkwork/webserver/database/` | |
| Specs | `/usr/lib/sdkwork/webserver/specs/` | |
| **PC SPA** | `/usr/share/sdkwork/webserver/web/pc/` | packaged build |
| **H5 SPA** | `/usr/share/sdkwork/webserver/web/h5/` | packaged build |
| **Static fallback** | `/usr/share/sdkwork/webserver/web/static/` | packaged build |
| Docs / examples | `/usr/share/doc/sdkwork/webserver/` | |
| Durable data (IAM/Drive/…) | `/var/lib/sdkwork/webserver/` | `sdkwork:sdkwork` |
| Logs | `/var/log/sdkwork/webserver/` | |
| Cache | `/var/cache/sdkwork/webserver/` | |
| Runtime / PID | `/run/sdkwork/webserver/` | |
| systemd unit (test) | `/usr/lib/systemd/system/sdkwork-webserver-test.service` | |
| systemd unit (prod) | `/usr/lib/systemd/system/sdkwork-webserver.service` | |
| Certificate worker unit | `/usr/lib/systemd/system/sdkwork-webserver-certificate-worker.service` | |

### Runtime TOML sections (process)

Copied from postinst / container entrypoint into `config.toml`:

| Section | Purpose |
| --- | --- |
| `[profile]` | `deployment_profile`, `environment`, `profile_id`, `node_id` |
| `[ingress]` | `bind`, `management_expose_allowed`, public/app/backend URLs, CORS |
| `[app_roots]` | Adaptive Web PC/H5/static roots + module roots |
| `[deploy]` | Deployments / Drive / KB internal API URLs + token files |
| `[database]` | PostgreSQL (`password_file` only) |
| `[secrets]` | encryption key files + optional credential-entry token file |
| `[acme]` / `[tls]` | Certificate worker inputs |
| `[node]` / `[region]` | Node uuid, region/locale |
| `[[webserver.imports]]` | Optional sibling module `deployments/webserver/` imports |

Override: `SDKWORK_WEBSERVER_CONFIG_FILE` → alternate TOML path.

---

## 3. Cross-OS Adaptive Web share roots

| Surface | Linux | Container | macOS | Windows |
| --- | --- | --- | --- | --- |
| PC | `/usr/share/sdkwork/webserver/web/pc/` | `/app/share/sdkwork/webserver/web/pc/` | `/Library/Application Support/sdkwork/webserver/web/pc/` | `%ProgramFiles%\sdkwork\webserver\web\pc\` |
| H5 | `/usr/share/sdkwork/webserver/web/h5/` | `/app/share/sdkwork/webserver/web/h5/` | `…/web/h5/` | `…\web\h5\` |
| Static | `/usr/share/sdkwork/webserver/web/static/` | `/app/share/sdkwork/webserver/web/static/` | `…/web/static/` | `…\web\static\` |

Source checkout builds (not install): `apps/sdkwork-webserver-{pc,h5}/dist/{dev,test,staging,prod}/`.
Catalog example: [`deployments/webserver/app-roots.example.toml`](../../../deployments/webserver/app-roots.example.toml).

Env equivalents: `SDKWORK_WEBSERVER_{PC,H5,STATIC_FALLBACK}_STATIC_ROOT`, `SDKWORK_WEBSERVER_TABLET_SURFACE`.

---

## 4. Nginx / layout-v2 edge (declarative + stock sites)

### Source-of-truth (repo)

| Path | Purpose |
| --- | --- |
| `deployments/webserver/server.common.toml` | Shared hosts, TLS refs, locations (proxy-only) |
| `deployments/webserver/server.standalone.toml` | Standalone upstream → `127.0.0.1:3800` (topology default; env binds differ) |
| `deployments/webserver/server.cloud.toml` | Cloud profile deltas |
| `deployments/webserver/nginx.*.conf` | Rendered sidecars (`pnpm nginx:render`) |
| `deployments/webserver/app-roots.example.toml` | Process roots (copy into `config.toml`, **not** into `server.*.toml`) |
| `deployments/deploy.yaml` | Deploy v2 expose (`mode: api`) |

Validate: `pnpm check:webserver-toml`.

### Installed / operator edge (WSL Ubuntu)

| Path | Purpose |
| --- | --- |
| `/etc/nginx/sites-available/sdkwork/<domain>.conf` | Generated operator sites |
| `/etc/nginx/sites-enabled/sdkwork/<domain>.conf` | Enabled sites (symlink) |
| `/etc/nginx/sites-available/sdkwork-webserver` | Production deb ACME/HTTPS site (postinst) |
| Stock nginx | `systemctl` nginx, or replaced by |
| `serve-nginx /etc/nginx/sites-enabled/sdkwork` | Same binary, nginx-compat data plane |

nginx-compat commands:

```bash
/usr/lib/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway validate-nginx /etc/nginx/sites-enabled/sdkwork
/usr/lib/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway serve-nginx /etc/nginx/sites-enabled/sdkwork
```

Docker WSL site helper (maps **Docker host ports**, not native deb bind):
`deployments/docker/scripts/install-wsl-nginx.sh`.

Native Adaptive Web edge helper (maps to process bind from `config.toml`):
`.sdkwork/wsl-point-server-sites-to-gateway.sh`.

---

## 5. Docker compose paths and ports

| Path | Purpose |
| --- | --- |
| `deployments/docker/Dockerfile.standalone` | Image contract + default SPA env roots |
| `deployments/docker/scripts/entrypoint-standalone.sh` | Writes `/etc/sdkwork/webserver/config.toml`, ensures secrets + SPA copy |
| `deployments/docker/env/{development,test,production}.env` | Host ports, DB/Redis, URLs |
| `deployments/docker/docker-compose.{development,test,production}.yml` | Per-env stacks |
| Volumes | `…_webserver-secrets-*` → `/etc/sdkwork/webserver/secrets` |
|  | `…_webserver-data-*` → `/var/lib/sdkwork/webserver` |

| Environment | Container listen | Host port | Domain |
| --- | --- | --- | --- |
| development | `3800` | `13800` | `server-dev.sdkwork.com` |
| test | `8888` | `18888` | `server-test.sdkwork.com` |
| production | `8080` | `18080` | `server.sdkwork.com` |

Inside the container, Adaptive Web roots default to `/app/share/sdkwork/webserver/web/{pc,h5,static}/`
(entrypoint may copy from `/app/share/sdkwork/webserver-{pc,h5}`).

---

## 6. Dual-stack operator note (WSL)

On one WSL host you can run **both**:

1. **Ubuntu `.deb`** — `sdkwork-webserver-test` on `0.0.0.0:8888` + edge `:80` → `127.0.0.1:8888` (process Adaptive Web).
2. **Docker** — host ports `13800` / `18888` / `18080` for the three container environments.

Do not point domain `:80` at Docker smoke ports when validating the native package console; use Docker ports or Docker-oriented `install-wsl-nginx.sh` only for container stacks.

Matrix smoke: `.sdkwork/wsl-install-matrix-smoke.sh`.

---

## 7. Verification commands

```bash
# Declarative edge TOML
pnpm check:webserver-toml

# Packaged Adaptive Web layout
pnpm check:browser-dist-layout
pnpm check:adaptive-web

# Container matrix
pnpm check:container-deployment
pnpm test:container-deployment

# Installed process
sudo systemctl status sdkwork-webserver-test   # or sdkwork-webserver
curl -fsS http://127.0.0.1:8888/healthz
curl -fsS -H 'Host: server.sdkwork.com' http://127.0.0.1/

# Docker ports
curl -fsS http://127.0.0.1:13800/healthz
curl -fsS http://127.0.0.1:18888/healthz
curl -fsS http://127.0.0.1:18080/healthz

# nginx-compat
…/sdkwork-api-webserver-standalone-gateway validate-nginx /etc/nginx/sites-enabled/sdkwork
```
