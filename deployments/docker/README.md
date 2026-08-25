# SDKWork Web Server Container Images

Reference: `sdkwork-api-cloud-gateway` (`docker-compose.yml` + `docker-compose.external.yml`).

## Dependency Modes

| Mode | Command | PostgreSQL | Redis |
| --- | --- | --- | --- |
| ① Built-in | `bash scripts/docker/deploy-docker-environment.sh development` | compose `postgres:16-alpine` | compose `redis:8-alpine` |
| ② External (standalone files) | `bash scripts/docker/deploy-docker-environment.sh all` | `WEBSERVER_POSTGRES_HOST` | `WEBSERVER_REDIS_HOST` |
| ③ Shared built-in (all envs) | `bash scripts/docker/deploy-docker-environment.sh all --embedded-shared` | one postgres | one redis |

Operator guide: [docs/guides/operator/WSL_EXTERNAL_DEPLOY.md](../../docs/guides/operator/WSL_EXTERNAL_DEPLOY.md)

## File Layout

| Path | Purpose |
| --- | --- |
| `docker-compose.yml` | Built-in postgres + redis + profiled gateway services |
| `docker-compose.external.yml` | Disables built-in deps, requires external hosts |
| `docker-compose.development.yml` | **Standalone** development environment (external deps) |
| `docker-compose.test.yml` | **Standalone** test environment (external deps) |
| `docker-compose.production.yml` | **Standalone** production environment (external deps) |
| `docker-compose.platform-api-gateway.yml` | Optional overlay: sibling `sdkwork-api-cloud-gateway` container |
| `env/<environment>.env` | Per-environment deployment configuration |
| `env/<environment>.env.example` | Per-environment deployment template |
| `postgres/init/` | Built-in multi-identity bootstrap |
| `postgres/external-schema.sql` | External postgres schema provisioning |
| `nginx/README.md` | Host nginx retired; Docker webserver owns reverse proxy |
| `scripts/` | Container entrypoint and WSL deployment helpers |

Declarative web server authority: [`../webserver/`](../webserver/) (`SDKWORK_WEBSERVER_SPEC.md`).
Host Ubuntu nginx is **not** used — uninstall with `uninstall-wsl-nginx.sh`.

## Quick Start (WSL External Mode)

```bash
# 1. Clone sdkwork-space on the Ubuntu host (once)
sudo bash deployments/docker/scripts/setup-host-space-clone.sh

# 2. One-command deployment (provisions DBs, deploys all envs, retires host nginx)
sudo bash deployments/docker/scripts/wsl-external-deploy.sh

# Rebuild frontend + release + Docker image, then redeploy all environments
# (use after admin UI or gateway fixes; does not drop PostgreSQL databases)
bash scripts/docker/redeploy-all-environments.sh

# Windows (repo on E: etc.) — delegates to WSL; auto-uses /tmp release staging on /mnt/*
pnpm deploy:rebuild:all:wsl

# WSL checkout on native ext4 — full rebuild including vite in Linux
bash scripts/docker/redeploy-all-environments.sh

# Or step by step:
# 1. Provision external dependencies
sudo bash deployments/docker/scripts/setup-host-external-deps.sh

# 2. Deploy all environments
bash scripts/docker/deploy-docker-environment.sh all

# 3. Hosts + uninstall host nginx (Docker owns reverse proxy)
sudo bash deployments/docker/scripts/install-wsl-hosts.sh
sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh

# 4. Verify (Docker published ports — development owns host 80/443)
curl --noproxy '*' http://127.0.0.1:13800/healthz
curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz
curl --noproxy '*' -H 'Host: api-dev.birdcoder.cn' http://127.0.0.1/healthz
bash scripts/docker/verify-platform-api-plane.sh development
```

## Domain Access (Docker :80 / :443 — no host nginx)

`sdkwork-webserver` Docker **module-imports data plane** (`serve-imports`) listens
on declared ports **80** (HTTP) and **443** (HTTPS) inside the container
(`SDKWORK_WEBSERVER_SPEC.md` module sidecars). Development publishes those binds
on the host:

| Surface | Development | Test | Production |
| --- | --- | --- | --- |
| Management console (`server-*`) | 13800 | 18888 | 18080 |
| Modules + platform API (HTTP) | **80** | 18898 | 18098 |
| Modules + platform API (HTTPS) | **443** | 28430 | 38430 |

| Domain examples | Host port |
| --- | --- |
| `server-dev.sdkwork.com` (+ app/admin) | 13800 |
| `im-dev.sdkwork.com`, `api-dev.sdkwork.com`, `api-dev.birdcoder.cn`, … | **80** / **443** |
| `api.sdkwork.com`, `api.birdcoder.com`, … (multi-cluster) | 18098 / 38430 |

Only one stack may bind host **80/443**. Development owns them by default; for a
production-only public edge set `SDKWORK_WEBSERVER_PROD_IMPORT_HTTP_HOST_PORT=80`
and `SDKWORK_WEBSERVER_PROD_HTTPS_HOST_PORT=443`.

Module `/api/` and the platform API plane (`api*.brand`) proxy to
**sdkwork-api-cloud-gateway**. Webserver startup does **not** wait for the gateway
(`SDKWORK_MODULE_API_GATEWAY_REQUIRED=false`).

Windows hosts point at `127.0.0.1`; WSL mirrors Docker `:80`/`:443` — no host
nginx and no portproxy side port.

```bash
sudo bash deployments/docker/scripts/uninstall-wsl-nginx.sh
curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz
curl --noproxy '*' -k -H 'Host: api-dev.sdkwork.com' https://127.0.0.1/healthz
```

### Module API gateway deployment modes

| Mode | Env | Compose |
| --- | --- | --- |
| **docker** (default) | `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker` | add `-f docker-compose.platform-api-gateway.yml` (auto when using `deploy-docker-environment.sh` or `compose.mjs`) |
| **docker + attach** | `SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK=<gateway-network>` | add `-f docker-compose.platform-api-gateway-attach.yml` — join an already-running independent gateway stack (no second container) |
| **bundled** | `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=bundled` | webserver container starts `sdkwork-api-cloud-gateway` as a second process on `127.0.0.1:3900` |
| **external** | `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=external` + `SDKWORK_MODULE_API_GATEWAY_HOST` | operator-managed gateway endpoint |

Platform API plane hosts (`api-dev.sdkwork.com`, `api.sdkwork.com`,
`api-dev.birdcoder.cn`, …) are imported from
`sdkwork-api-cloud-gateway/deployments/webserver/` into `imports.d/import.conf`
and reverse-proxied to the gateway upstream on the module-imports data-plane
ports (`13808` / `18898` / `18098`). Host nginx is not used.

Default path (docker sibling / attach): deploy webserver — import reverse proxy
is wired at boot without waiting for the gateway process:

```bash
cd ../sdkwork-api-cloud-gateway
pnpm build:container   # -> sdkwork-api-cloud-gateway:local

# env/*.env already defaults to SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker
bash scripts/docker/deploy-docker-environment.sh development
```

Independent gateway already running (attach to its Docker network):

```bash
# env/development.env
SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=docker
SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK=sdkwork-gateway-development_default
SDKWORK_MODULE_API_GATEWAY_HOST=gateway
SDKWORK_MODULE_API_GATEWAY_PORT=3900

bash scripts/docker/deploy-docker-environment.sh development
```

Or compose directly:

```bash
docker compose \
  -f deployments/docker/docker-compose.development.yml \
  -f deployments/docker/docker-compose.platform-api-gateway.yml \
  --env-file deployments/docker/env/development.env up -d
```

Optional bundled mode (in-container process) — set `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=bundled` and mount/build artifacts:

- `SDKWORK_MODULE_API_GATEWAY_BINARY_HOST_PATH` → `/app/bin/sdkwork-api-cloud-gateway`
- `SDKWORK_MODULE_API_GATEWAY_INSTALL_ROOT_HOST_PATH` → `/opt/sdkwork/api-gateway` (release tree with `database-modules/`)

```bash
cd ../sdkwork-api-cloud-gateway
cargo build --release -p sdkwork-api-cloud-gateway
pnpm build:container   # produces dist/container-image-build + :local image
```

One-shot setup inside WSL:

```bash
sudo bash deployments/docker/scripts/setup-wsl-domain-proxy.sh
```

Windows browser (run **PowerShell as Administrator** once):

```powershell
.\deployments\docker\scripts\setup-windows-port-forwarding-admin.ps1
```

Then open `http://server-dev.sdkwork.com` in the browser.

## Verification

```bash
pnpm check:container-deployment
pnpm test:container-deployment
node scripts/docker/validate-docker-deployment.mjs --matrix --compose
node ../sdkwork-specs/tools/sweep-browser-build-workspace.mjs --workspace ..
```

## Independent Module Browser Builds

Each sibling module under the mounted `sdkwork-space` checkout exposes the
standard PC/H5 build family (`pnpm build:pc:dev`, `pnpm build:h5:prod`, …).
Outputs land in `apps/sdkwork-<code>-{pc,h5}/dist/{standalone,cloud}/{dev,test,staging,prod}/`.

Build one module from the host checkout:

```bash
cd /opt/deploy/sdkwork-space/sdkwork-im
pnpm build:pc:dev
pnpm build:h5:prod
```

Build through the webserver operator scripts (same canonical runner):

```bash
# Default: rebuild all owned PC/H5 surfaces for development, then reload the container
pnpm build:container:module -- --module sdkwork-im --architecture all --environment dev --reload

# Production PC only, using the container toolchain
pnpm build:container:module -- --module sdkwork-im --architecture pc --environment prod --deployment-environment production --in-container

# Low-level single-surface host build
pnpm build:container:module:browser -- --module sdkwork-im --architecture h5 --environment dev
```

Build inside a running standalone container (toolchain preinstalled):

```bash
docker compose -f deployments/docker/docker-compose.development.yml \
  run --rm webserver build-browser \
  --module sdkwork-im --architecture all --environment dev --reload-static
```

After build, restart or reload the environment container so the entrypoint
re-resolves `apps/*/dist/<profile>/<envAlias>/` static roots for the active lifecycle
tier.

Authority: `PNPM_SCRIPT_SPEC.md` §4.2–§4.3, `SDKWORK_WEBSERVER_SPEC.md` §17.1.

## Port And Domain Matrix

| Environment | Management (3800) | Public HTTP (80) | Public HTTPS (443) | Domains | DB identity | Redis DB |
| --- | --- | --- | --- | --- | --- | --- |
| development | 13800 | **80** | **443** | `server-dev.sdkwork.com` (+ app/admin) + module/API `*-dev.*` | `sdkwork_ai_dev` | 0 |
| test | 18888 | 18898→80 | 28430→443 | `server-test.sdkwork.com` (+ app/admin) + module/API `*-test.*` | `sdkwork_ai_test` | 1 |
| production | 18080 | 18098→80 | 38430→443 | `server.sdkwork.com` (+ app/admin) + module/API bare hosts | `sdkwork_ai_prod` | 2 |

Container listeners are always **80** / **443** (no port remap). Host 80/443 are
owned by development by default; only one stack may publish them.

Host PostgreSQL (`5432`) and Redis (`6379`) stay on Ubuntu/WSL native services in external mode; containers reach them via `host.docker.internal`.

Space clone defaults: host path `SDKWORK_SPACE_HOST_PATH=/opt/deploy` bind-mounted to container `/opt/deploy`; checkout `SDKWORK_SPACE_CLONE_URL=https://github.com/sdkwork-ai/sdkwork-space.git` at `/opt/deploy/sdkwork-space`. Multiple environment clusters on one Ubuntu host share the same checkout and use distinct **management** host ports (`13800` / `18888` / `18080`). Module imports auto-discover from `sdkwork-*/deployments/webserver/` unless `SDKWORK_SPACE_MODULES` is set; the entrypoint writes one nginx-style import file per module under `/etc/sdkwork/webserver/imports.d/` and the runtime config loads them through `[webserver] include`. See `SDKWORK_WEBSERVER_SPEC.md` §17.

Base domain: `sdkwork.com` only (registered in topology `cloudPublicHosts`).

## Cloud Website Image

See `Dockerfile` for the Kubernetes website data-plane image contract.
