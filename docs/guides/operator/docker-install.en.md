# Docker Install Bundle Deployment Guide (docker-install)

> Version: 2026-08-30 · Applies to: `sdkwork-webserver` standalone unified install image
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6, `sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4

## 1. Design Overview

`pnpm build:container:install` produces **one unified install bundle** (self-contained install bundle):

- **One image**: the image is environment-neutral. Nothing about the lifecycle environment, domain, database, or credentials is baked at build time. The environment (development / test / production) and the instance count are **deployment-time inputs**, resolved by the container entrypoint at start.
- **Any environment**: development, test, and production all run the same image tag; the environment is selected through the env file at deploy time.
- **Every environment supports multi-instance**: N instances share one network and one set of secrets/data volumes; each instance owns a distinct compose project name, node identity (`SDKWORK_WEBSERVER_NODE_UUID`), and management port. Only instance 1 publishes the 80/443 edge ports and runs database migrations first.

## 2. Packaging the Bundle

```bash
# Repository root (sdkwork-webserver)
pnpm build:container:install                       # build image + package bundle
pnpm build:container:install -- --skip-image-build # reuse an already-built image
pnpm build:container:install -- --tag 0.1.0 --out dist/docker-install --dry-run
```

From the sdkwork-space root (WSL / CI), the `bin` wrapper is available:

```bash
bash bin/build-webserver-docker.sh                 # version from the app manifest
bash bin/build-webserver-docker.sh --out /opt/deploy/packages
```

Bundle layout:

```text
dist/docker-install/sdkwork-webserver-install-<version>.bundle/
├── image.tar.gz / image.sha256 / image.env   # image archive + checksum + tag
├── compose/
│   ├── docker-compose.bundle.yml             # environment-neutral multi-instance template
│   └── docker-compose.bundle-edge.yml        # instance-1 80/443 edge overlay
├── env/
│   ├── development.env.example
│   ├── test.env.example
│   └── production.env.example
├── deploy.sh                                 # generic deploy script (single entrypoint)
├── manifest.json                             # version / image / sha256 metadata
└── README.md
```

## 3. Deploying (any Docker host)

Copy the bundle directory to the target host; `deploy.sh` is the single entrypoint:

```bash
# Embedded postgres/redis (default)
bash deploy.sh --environment development

# Production with 3 instances
bash deploy.sh --environment production --replicas 3

# External postgres/redis with 2 instances
bash deploy.sh --environment production --external --replicas 2

# Other operations
bash deploy.sh --environment test --ps
bash deploy.sh --environment test --logs 2        # follow instance 2 logs
bash deploy.sh --environment test --down
bash deploy.sh --environment test --down --purge  # also delete volumes/network
```

Key rules:

- `--environment` is required (development | test | production); a missing or unknown value fails **before any side effect**. Re-running apply is idempotent (updates the existing stack in place).
- The first run generates `env/<environment>.env` from `env/<environment>.env.example` — **fill in the secrets before exposing anything beyond localhost**.
- If the image is not loaded yet, the script runs `docker load image.tar.gz` automatically.
- Instance 1 starts first and waits until healthy (including database migrations); instances 2..N start afterwards, avoiding migration races.

Repository-root equivalent:

```bash
pnpm deploy:apply:standalone:docker -- --environment production --replicas 3
```

## 4. Environment Configuration (environment selected at deploy time)

Edit `env/<environment>.env` (it inherits every key from the existing `deployments/docker/env/*.env.example`):

| Key | Purpose |
| --- | --- |
| `SDKWORK_WEBSERVER_IMAGE_TAG` | Image tag (the bundle `image.env` also provides it) |
| `SDKWORK_WEBSERVER_ENVIRONMENT` | Lifecycle environment (required by compose) |
| `WEBSERVER_POSTGRES_*` / `PG_MAX_CONNECTIONS` | Embedded postgres dependency |
| `SDKWORK_DATABASE_*`, `SDKWORK_WEBSERVER_REDIS_*` | Database / Redis connection (point at external instances with `--external`) |
| `SDKWORK_SPACE_HOST_PATH` | Host `/opt/deploy` bind mount (module import plane) |
| `SDKWORK_WEBSERVER_PRIMARY_DOMAIN`, `SDKWORK_CORS_ALLOWED_ORIGINS` | Domain and CORS |
| `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT` | `bundled` by default in the bundle (in-container gateway process); can be `docker`/`external` |

## 5. Multi-Instance Topology (supported for every environment)

```text
Host (one set per environment)
├─ network  sdkwork-webserver-<env>            (shared by instances and deps)
├─ volume   sdkwork-webserver-<env>-secrets    (shared: consistent keys/ACME accounts)
├─ volume   sdkwork-webserver-<env>-data       (shared: TLS material/runtime data)
├─ deps project   sdkwork-webserver-<env>-deps (embedded mode: postgres + redis)
└─ instance project sdkwork-webserver-<env>-i<i>
     ├─ i1: mgmt base+0 -> 3800, plus 80/443 edge; starts first, migrates first
     ├─ i2: mgmt base+1 -> 3800
     └─ iN: mgmt base+N-1 -> 3800
```

- Per-instance node identity: `SDKWORK_WEBSERVER_NODE_UUID=standalone-<env>-i<i>` (the in-container entrypoint also derives a unique hostname-based default).
- Load balancing across instances: balance the per-instance management ports (base..base+N-1); the 80/443 edge lives only on instance 1.
- Multi-instance prerequisite: shared PostgreSQL / Redis (embedded or external); instance 1 performs migrations.
- Volumes are **shared per environment**: all N instances of one environment reference the same secrets/data volumes, so keys stay consistent.

Default management port bases (overridable in the env file): development `13800`, test `18888`, production `18080`; edge ports dev `80/443`, test `18898/28430`, prod `18098/38430`.

## 6. Relation to Existing Commands

| Command | Purpose |
| --- | --- |
| `pnpm build:container:standalone` | Build the unified install image only (no bundle) |
| `pnpm build:container:install` | Build + package the self-contained install bundle (this guide) |
| `pnpm deploy:apply:standalone:docker` | Run the bundle deploy.sh from the repository root |
| `docker:build:standalone` and friends | Retained migration aliases; new automation MUST use `build:container:*` / `deploy:apply:*` |

## 7. Webserver Spec Compliance (SDKWORK_WEBSERVER_SPEC.md)

The bundle compose template and deploy script follow `SDKWORK_WEBSERVER_SPEC.md`:

| Spec point | Implementation |
| --- | --- |
| §17 space mounts | The space root `/opt/deploy` is mounted read-only; the `sdkwork-space` checkout subtree is a read-write overlay (the entrypoint clone/pull target) |
| §17 module import plane | `SDKWORK_SPACE_AUTO_DISCOVER` / `SDKWORK_SPACE_MODULES` / `MODULE_IMPORT_REQUIRED` / `PROBE_UPSTREAMS` are passed through; module static assets resolve from the checkout at `apps/*-{pc,h5}/dist/standalone/<envAlias>/` (§13.6 / §17.1) |
| §17.3 import sets | `SDKWORK_WEBSERVER_IMPORT_PROFILE` defaults to `cloud` (dual imports.d sets, materialized by the entrypoint at start); `SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS` passes through (identity 80/443 by default) |
| §17 multi-cluster / multi-instance | Every container listens on gateway port 3800 internally so module `server.standalone.toml` upstreams stay uniform across instances and hosts; host ports differ per instance |
| §17.4 standalone-only | The bundle is packaged from the standalone release artifact only (`webserver-release.mjs --deployment-profile standalone`); the manifest declares `deploymentProfile: standalone` |
| §8.1 gateway upstream | Module `/api/` reverse proxying goes through the reserved upstream `gateway`; the bundle defaults to `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=bundled` (in-container gateway process on port 3900), switchable to `docker`/`external` via the env file |

## 8. Configuring Multiple Webservers (per-instance overrides)

When one environment runs multiple instances, each instance can carry its own configuration override file:

```text
env/production.env            # environment-level base config (shared by all instances)
env/production.i1.env         # instance-1 overrides (optional)
env/production.i2.env         # instance-2 overrides (optional)
```

- When deploy.sh detects `env/<environment>.i<N>.env`, it layers it as a second `--env-file` (compose: the later file wins) and prints a notice.
- Typical uses: bind a different primary domain (`SDKWORK_WEBSERVER_PRIMARY_DOMAIN`) per instance, a different clone URL, a different TLS/ACME profile, or any other deployment input.
- Instances without an override file keep using the environment base config; management ports and node identity are always assigned by deploy.sh per instance and are unaffected by overrides.

Chinese version: [docker-install.md](./docker-install.md)
