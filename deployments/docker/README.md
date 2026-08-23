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
# One-command deployment (provisions DBs, deploys all envs, configures nginx)
sudo bash deployments/docker/scripts/wsl-external-deploy.sh

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
```

## Port And Domain Matrix

| Environment | Container port | Host port | Host HTTPS | Domains | DB identity | Redis DB |
| --- | --- | --- | --- | --- | --- | --- |
| development | 3800 | 13800 | 18430 | `server-dev.sdkwork.com` (+ `web-app-dev` / `web-admin-dev`) | `sdkwork_ai_dev` | 0 |
| test | 8888 | 18888 | 28430 | `server-test.sdkwork.com` (+ `web-app-test` / `web-admin-test`) | `sdkwork_ai_test` | 1 |
| production | 8080 | 18080 | 38430 | `server.sdkwork.com` (+ `web-app` / `web-admin`) | `sdkwork_ai_prod` | 2 |

Host PostgreSQL (`5432`) and Redis (`6379`) stay on Ubuntu/WSL native services in external mode; containers reach them via `host.docker.internal`.

Space clone defaults: `SDKWORK_SPACE_ROOT=/opt/deploy`, `SDKWORK_SPACE_CLONE_URL=https://github.com/sdkwork-ai/sdkwork-space.git` (volume `webserver-opt-deploy-*`). Drive sandbox key `deploy.local.opt_deploy` scopes the Deployments Local Projects file browser to `/opt/deploy` only.

Base domain: `sdkwork.com` only (registered in topology `cloudPublicHosts`).

## Cloud Website Image

See `Dockerfile` for the Kubernetes website data-plane image contract.
