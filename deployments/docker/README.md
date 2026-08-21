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

## Verification

```bash
pnpm check:container-deployment
pnpm test:container-deployment
node scripts/docker/validate-docker-deployment.mjs --matrix --compose
```

## Port And Domain Matrix

| Environment | Container port | Host port | Domains | DB identity | Redis DB |
| --- | --- | --- | --- | --- | --- |
| development | 3800 | 13800 | `server-dev.sdkwork.com` (+ `web-app-dev` / `web-admin-dev`) | `sdkwork_ai_dev` | 0 |
| test | 8888 | 18888 | `server-test.sdkwork.com` (+ `web-app-test` / `web-admin-test`) | `sdkwork_ai_test` | 1 |
| production | 8080 | 18080 | `server.sdkwork.com` (+ `web-app` / `web-admin`) | `sdkwork_ai_prod` | 2 |

Base domain: `sdkwork.com` only (registered in topology `cloudPublicHosts`).

## Cloud Website Image

See `Dockerfile` for the Kubernetes website data-plane image contract.
