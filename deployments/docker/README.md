# SDKWork Web Server Container Images

Reference: `sdkwork-api-cloud-gateway` (`docker-compose.yml` + `docker-compose.external.yml`).

## Dependency Modes

| Mode | Command | PostgreSQL | Redis |
| --- | --- | --- | --- |
| ① Built-in | `bash scripts/docker/deploy-docker-environment.sh development` | compose `postgres:16-alpine` | compose `redis:8-alpine` |
| ② External | `... development --external` | `WEBSERVER_POSTGRES_HOST` | `WEBSERVER_REDIS_HOST` |
| ③ Shared built-in (all envs) | `... all --embedded-shared` | one postgres | one redis |

Operator guide: [docs/guides/operator/WSL_DOCKER_DEPLOY.md](../../docs/guides/operator/WSL_DOCKER_DEPLOY.md)

## File Layout

| Path | Purpose |
| --- | --- |
| `docker-compose.yml` | Built-in postgres + redis + profiled gateway services |
| `docker-compose.external.yml` | Disables built-in deps, requires external hosts |
| `env/<environment>.env.example` | Per-environment deployment template |
| `postgres/init/` | Built-in multi-identity bootstrap |
| `postgres/external-schema.sql` | External postgres schema provisioning |
| `nginx/*.conf` | WSL host `:80` domain routing |
| `scripts/` | Container entrypoint |

## Quick Start (WSL)

```bash
pnpm docker:build:standalone
cp deployments/docker/env/development.env.example deployments/docker/env/development.env
bash scripts/docker/deploy-docker-environment.sh development --validate
sudo bash deployments/docker/scripts/install-wsl-nginx.sh
sudo bash deployments/docker/scripts/install-wsl-hosts.sh
curl http://server-dev.sdkwork.com/healthz
```

## Verification

```bash
pnpm check:container-deployment
pnpm test:container-deployment
node scripts/docker/validate-docker-deployment.mjs --matrix --compose
```

## Port And Domain Matrix

| Environment | Host port | Domains | DB identity | Redis DB |
| --- | --- | --- | --- | --- |
| development | 13800 | `server-dev.*` | `sdkwork_ai_dev` | 0 |
| test | 18888 | `server-test.*` | `sdkwork_ai_test` | 1 |
| production | 18080 | `server.*` | `sdkwork_ai_prod` | 2 |

## Cloud Website Image

Unchanged — see previous section in git history / `Dockerfile` for Kubernetes website data-plane.
