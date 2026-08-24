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
| `env/<environment>.env` | Per-environment deployment configuration |
| `env/<environment>.env.example` | Per-environment deployment template |
| `postgres/init/` | Built-in multi-identity bootstrap |
| `postgres/external-schema.sql` | External postgres schema provisioning |
| `nginx/README.md` | WSL site generation notes (no dual-authority confs) |
| `scripts/` | Container entrypoint and WSL deployment helpers |

Declarative web server authority: [`../webserver/`](../webserver/) (`SDKWORK_WEBSERVER_SPEC.md`).
WSL host nginx sites are generated into `/etc/nginx/sites-enabled/sdkwork/<domain>.conf`.

## Quick Start (WSL External Mode)

```bash
# 1. Clone sdkwork-space on the Ubuntu host (once)
sudo bash deployments/docker/scripts/setup-host-space-clone.sh

# 2. One-command deployment (provisions DBs, deploys all envs, configures nginx)
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

# 3. Configure nginx and hosts
sudo bash deployments/docker/scripts/install-wsl-nginx.sh
sudo bash deployments/docker/scripts/install-wsl-hosts.sh

# 4. Verify
curl http://server-dev.sdkwork.com/healthz
curl http://server-test.sdkwork.com/healthz
curl http://server.sdkwork.com/healthz
```

## Domain Access (WSL nginx → Docker)

Ubuntu nginx on port **80** reverse-proxies domains to Docker host ports (no conflict with host PostgreSQL **5432** / Redis **6379**):

| Domain | Docker host port |
| --- | --- |
| `server-dev.sdkwork.com` (+ app/admin dev) | 13800 |
| `server-test.sdkwork.com` (+ app/admin test) | 18888 |
| `server.sdkwork.com` (+ app/admin prod) | 18080 |
| `sdkwork.com`, `app.sdkwork.com` | 18080 (prod) |

Module domains (`im-dev.sdkwork.com`, …) are served by the container's
**module-imports data plane** (`serve-imports`): the entrypoint symlinks each
enabled sibling module's rendered nginx sidecar
(`deployments/webserver/nginx.<profile>.<environment>.conf`) into
`/etc/sdkwork/webserver/imports.d/*.conf`. Runtime `[webserver] include` loads
those `.conf` files (TOML import descriptors under `imports.d/*.toml` remain
supported). Declared listener ports are remapped via
`SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS` (default `80=8080,443=8430`).

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
Outputs land in `apps/sdkwork-<code>-{pc,h5}/dist/{dev,test,staging,prod}/`.

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
re-resolves `apps/*/dist/<envAlias>/` static roots for the active lifecycle
tier.

Authority: `PNPM_SCRIPT_SPEC.md` §4.2–§4.3, `SDKWORK_WEBSERVER_SPEC.md` §17.1.

## Port And Domain Matrix

| Environment | Management (3800) | Import HTTP (8080) | Import HTTPS (8430) | Domains | DB identity | Redis DB |
| --- | --- | --- | --- | --- | --- | --- |
| development | 13800 | 13808 | 18430 | `server-dev.sdkwork.com` (+ `web-app-dev` / `web-admin-dev`) + module `*-dev.*` hosts | `sdkwork_ai_dev` | 0 |
| test | 18888 | 18898 | 28430 | `server-test.sdkwork.com` (+ `web-app-test` / `web-admin-test`) + module `*-test.*` hosts | `sdkwork_ai_test` | 1 |
| production | 18080 | 18098 | 38430 | `server.sdkwork.com` (+ `web-app` / `web-admin`) + module bare hosts | `sdkwork_ai_prod` | 2 |

Host PostgreSQL (`5432`) and Redis (`6379`) stay on Ubuntu/WSL native services in external mode; containers reach them via `host.docker.internal`.

Space clone defaults: host path `SDKWORK_SPACE_HOST_PATH=/opt/deploy` bind-mounted to container `/opt/deploy`; checkout `SDKWORK_SPACE_CLONE_URL=https://github.com/sdkwork-ai/sdkwork-space.git` at `/opt/deploy/sdkwork-space`. Multiple environment clusters on one Ubuntu host share the same checkout and use distinct host ports (`13800` / `18888` / `18080`). Module imports auto-discover from `sdkwork-*/deployments/webserver/` unless `SDKWORK_SPACE_MODULES` is set; the entrypoint writes one nginx-style import file per module under `/etc/sdkwork/webserver/imports.d/` and the runtime config loads them through `[webserver] include`. See `SDKWORK_WEBSERVER_SPEC.md` §17.

Base domain: `sdkwork.com` only (registered in topology `cloudPublicHosts`).

## Cloud Website Image

See `Dockerfile` for the Kubernetes website data-plane image contract.
