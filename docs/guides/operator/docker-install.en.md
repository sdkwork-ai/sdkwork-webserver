# sdkwork-webserver Docker Install And Deployment Guide

> Authority: `sdkwork-specs/DOCKER_SPEC.md` (image/install/config standard), `sdkwork-specs/MODULE_BIN_SPEC.md` (`bin/` entrypoint standard), `sdkwork-specs/DEPLOYMENT_SPEC.md` §6/§6.1 (container install and external dependencies).
> This is the single authoritative Docker install document; it supersedes and absorbs `WSL_DOCKER_DEPLOY.md`, `WSL_EXTERNAL_DEPLOY.md`, and `docker-remote-deploy.md/.en.md`. 中文版: [docker-install.md](./docker-install.md)

**Core idea: one image for every environment + one bin entrypoint family.** The image is environment-neutral (no baked domains/databases/credentials); the five lifecycle environments (development / test / staging / demo / production) share one image tag; all environment differences are deploy-time env inputs. Deployment goes exclusively through the `bin/` entrypoints (`MODULE_BIN_SPEC.md`) — there is no second deployment script family.

---

## 0. Quick Start (TL;DR)

### Scenario A — brand-new Ubuntu/WSL Docker host with an install bundle (fastest)

```bash
tar -xzf sdkwork-webserver-install-<version>.bundle.tar.gz
cd sdkwork-webserver-install-<version>.bundle
$EDITOR env/<environment>.env        # fill secrets; ports/domains have safe defaults
bash deploy.sh --environment <environment> [--replicas N]
```

### Scenario B — dev/operator workstation with the repo checkout (bin/ entrypoints)

```bash
bin/docker-image.sh build --image-tag <version>      # build the canonical image (--image-tag is optional; defaults to release.currentVersion)
bin/docker-deploy.sh install --environment demo      # local WSL deployment
bin/docker-deploy.sh install --environment demo --host ssh://ops@10.0.0.8   # remote Ubuntu
```

> Fully expanded per-environment commands (install / upgrade / rollback / status / logs / down): §5.

### Scenario C — install on a remote machine from the repo

```bash
bin/docker-image.sh save --image-tag <version> -o image.tar.gz   # optional offline media
bin/docker-deploy.sh install --environment <env> --host ssh://[user@]host[:port]
```

---

## 1. Topology And Roles

- `sdkwork-webserver` is the **only public edge** (standalone-only): serves its own PC/H5 SPAs (same-origin `/` SDK API base URLs) and reverse-proxies the platform API plus sibling-module hosts through `imports.d` (default `cloud` set).
- `sdkwork-api-cloud-gateway` is the API gateway and is **never exposed publicly**; it is reachable only through the webserver reverse proxy (container `:3900`, per-environment host health ports 3910-3914).
- **Never install host nginx** (`NGINX_SPEC.md` §0); the webserver container publishes public `:80/:443`.

## 2. Prerequisites (Ubuntu 22.04 / WSL Ubuntu)

| Item | Requirement |
| --- | --- |
| Docker Engine + compose plugin | required |
| Host PostgreSQL | `5432` (system service; `setup-host-external-deps.sh` provisions per-environment databases) |
| Host Redis | `6379` (system service, no password, `bind 0.0.0.0`) |
| Space directory | `/opt/deploy` (module clone target `/opt/deploy/sdkwork-space`) |
| Drive cache | `/opt/deploy/drive` (`SDKWORK_DRIVE_WEBSITE_CACHE_ROOT`) |
| Certificate directory | `/etc/sdkwork/certs/letsencrypt/<cert-name>/` (TLS environments) |

External dependencies are the **default mode** (`DEPLOYMENT_SPEC.md` §6.1); embedded postgres/redis containers are an explicit opt-in only (`deploy.sh --embedded`). The retired `15432` port must not appear in any document or script.

## 3. Five-Environment Matrix

| Environment | Management port key (default) | Import HTTP (default) | HTTPS (default) | Domains | Database |
| --- | --- | --- | --- | --- | --- |
| development | `SDKWORK_WEBSERVER_DEV_HOST_PORT` (13800) | 80 | 443 | `server-dev.*`, `*-dev.*` | `sdkwork_ai_dev` |
| test | `…_TEST_HOST_PORT` (18888) | 18898 | 28430 | `server-test.*`, `*-test.*` | `sdkwork_ai_test` |
| staging | `…_STAGING_HOST_PORT` (18081) | 18099 | 38431 | `server-staging.*`, `*-staging.*` | `sdkwork_ai_staging` |
| demo | `…_DEMO_HOST_PORT` (19080) | 19098 | 38432 | `server-demo.*`, `api-demo.*` | `sdkwork_ai_demo` |
| production | `…_PROD_HOST_PORT` (18080) | 18098 | 38430 | `server.*`, `api.*` | `sdkwork_ai_prod` |

- Containers always listen on **80/443** internally (no port remap); the webserver container gateway port is fixed at **3800**.
- `demo` is an independent demonstration tier: dedicated database, Redis key prefix, and `api-demo` domain family — persistence is never shared with other environments.
- Full port-key contract: `DOCKER_SPEC.md` §3.2.

## 4. Standard Install Flow (bin/ entrypoints, recommended)

```bash
# 0. Build the canonical image (once)
bin/docker-image.sh build --image-tag <version>
# → registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version>

# 1. Provision host PostgreSQL/Redis (all environments, once)
sudo bash deployments/docker/scripts/setup-host-external-deps.sh

# 2. Deploy an environment (idempotent; re-running converges)
bin/docker-deploy.sh install --environment development
bin/docker-deploy.sh install --environment test
bin/docker-deploy.sh install --environment demo
# production mutations require explicit confirmation:
bin/docker-deploy.sh install --environment production --yes

# Remote Ubuntu server:
bin/docker-deploy.sh install --environment demo --host ssh://ops@10.0.0.8
```

Notes:

- `install` syncs the install bundle to `/opt/deploy/sdkwork-webserver/bundle` on the target host and runs the bundle `deploy.sh --environment <env>`; it is idempotent.
- Multi-instance: `--replicas N`; only instance 1 publishes edge ports, others use the port stride (`DOCKER_SPEC.md` §3.2).
- Every mutating command supports `--dry-run`; production without `--yes` is refused (error code 68).
- `pnpm deploy:reapply:<env>` is a thin alias of the bin entrypoints above.
- **§5 expands every command for all five environments, copy-paste ready.**

## 5. Per-Environment Command Reference (copy-paste ready)

> Every block below can be copied and run as-is. Replace `<version>` with the
> real version (current value: `sdkwork.app.config.json` →
> `release.currentVersion`; **omit `--image-tag` to use it automatically**),
> replace `ops@10.0.0.8` with your remote Ubuntu host, and note that `--host`
> defaults to `wsl` (local WSL) when omitted. Any mutating command accepts
> `--dry-run` to print the plan without executing it.

### 5.1 development

Domains `server-dev.sdkwork.com` / `api-dev.*` · management `13800` · import HTTP `80` · HTTPS `443` · database `sdkwork_ai_dev`

```bash
bin/docker-image.sh build --image-tag <version>                    # shared by all environments; skip if already built
bin/docker-deploy.sh install  --environment development            # local WSL (idempotent)
bin/docker-deploy.sh install  --environment development --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment development --image-tag <new-version>
bin/docker-deploy.sh rollback --environment development            # no release.sh → idempotent re-install of the current bundle
bin/docker-deploy.sh status   --environment development
bin/docker-deploy.sh logs     --environment development
bin/docker-deploy.sh down     --environment development
bin/docker-deploy.sh stop    --environment development           # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment development           # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment development           # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment development --purge --yes
bin/docker-deploy.sh stop    --environment development           # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment development           # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment development           # restart app instances only (deps and gateway stay up)

curl --noproxy '*' http://127.0.0.1:13800/healthz                            # management plane
curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz   # import plane
```

### 5.2 test

Domains `server-test.*` / `api-test.*` · management `18888` · import HTTP `18898` · HTTPS `28430` · database `sdkwork_ai_test`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment test
bin/docker-deploy.sh install  --environment test --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment test --image-tag <new-version>
bin/docker-deploy.sh rollback --environment test                   # no release.sh → idempotent re-install of the current bundle
bin/docker-deploy.sh status   --environment test
bin/docker-deploy.sh logs     --environment test
bin/docker-deploy.sh down     --environment test
bin/docker-deploy.sh stop    --environment test                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment test                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment test                  # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment test --purge --yes
bin/docker-deploy.sh stop    --environment test                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment test                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment test                  # restart app instances only (deps and gateway stay up)

curl --noproxy '*' http://127.0.0.1:18888/healthz
curl --noproxy '*' -H 'Host: api-test.sdkwork.com' http://127.0.0.1:18898/healthz
```

### 5.3 staging

Domains `server-staging.*` / `api-staging.*` · management `18081` · import HTTP `18099` · HTTPS `38431` · database `sdkwork_ai_staging`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment staging
bin/docker-deploy.sh install  --environment staging --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment staging --image-tag <new-version>
bin/docker-deploy.sh rollback --environment staging                # no release.sh → idempotent re-install of the current bundle
bin/docker-deploy.sh status   --environment staging
bin/docker-deploy.sh logs     --environment staging
bin/docker-deploy.sh down     --environment staging
bin/docker-deploy.sh stop    --environment staging               # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment staging               # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment staging               # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment staging --purge --yes
bin/docker-deploy.sh stop    --environment staging               # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment staging               # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment staging               # restart app instances only (deps and gateway stay up)

curl --noproxy '*' http://127.0.0.1:18081/healthz
curl --noproxy '*' -H 'Host: api-staging.sdkwork.com' http://127.0.0.1:18099/healthz
```

### 5.4 demo

Domains `server-demo.*` / `api-demo.*` · management `19080` · import HTTP `19098` · HTTPS `38432` · database `sdkwork_ai_demo`

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment demo
bin/docker-deploy.sh install  --environment demo --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment demo --image-tag <new-version>
bin/docker-deploy.sh rollback --environment demo                   # no release.sh → idempotent re-install of the current bundle
bin/docker-deploy.sh status   --environment demo
bin/docker-deploy.sh logs     --environment demo
bin/docker-deploy.sh down     --environment demo
bin/docker-deploy.sh stop    --environment demo                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment demo                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment demo                  # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment demo --purge --yes
bin/docker-deploy.sh stop    --environment demo                  # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment demo                  # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment demo                  # restart app instances only (deps and gateway stay up)

curl --noproxy '*' http://127.0.0.1:19080/healthz
curl --noproxy '*' -H 'Host: api-demo.sdkwork.com' http://127.0.0.1:19098/healthz
```

### 5.5 production

Domains `server.*` / `api.*` · management `18080` · import HTTP `18098` · HTTPS `38430` · database `sdkwork_ai_prod`

> **Every mutating command requires `--yes`**, otherwise it is refused
> (error code 68). `down --purge` requires `--yes` in **every** environment.

```bash
bin/docker-image.sh build --image-tag <version>
bin/docker-deploy.sh install  --environment production --yes
bin/docker-deploy.sh install  --environment production --yes --host ssh://ops@10.0.0.8
bin/docker-deploy.sh upgrade  --environment production --yes --image-tag <new-version>
bin/docker-deploy.sh rollback --environment production --yes       # no release.sh → idempotent re-install of the current bundle
bin/docker-deploy.sh status   --environment production             # read-only, no --yes needed
bin/docker-deploy.sh logs     --environment production             # read-only, no --yes needed
bin/docker-deploy.sh down     --environment production
bin/docker-deploy.sh stop    --environment production            # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment production            # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment production            # restart app instances only (deps and gateway stay up)
bin/docker-deploy.sh down     --environment production --purge --yes
bin/docker-deploy.sh stop    --environment production            # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment production            # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment production            # restart app instances only (deps and gateway stay up)

curl --noproxy '*' http://127.0.0.1:18080/healthz
curl --noproxy '*' -H 'Host: api.sdkwork.com' http://127.0.0.1:18098/healthz
```

### 5.6 Multi-instance, dependency mode, and version rollback

```bash
# Multi-instance: only instance 1 publishes edge ports, others use the stride (DOCKER_SPEC.md §3.2)
bin/docker-deploy.sh install --environment demo --replicas 3

# Dependency mode: external host PostgreSQL/Redis is the default; embedded is an explicit opt-in
bin/docker-deploy.sh install --environment demo --deps external     # default
bin/docker-deploy.sh install --environment demo --deps embedded

# Roll back to a specific version (the webserver bundle has no release.sh: pin the old tag)
bin/docker-deploy.sh upgrade --environment demo --image-tag 0.1.0
```

> **Rollback semantics**: the `sdkwork-webserver` install bundle ships only
> `deploy.sh` — no `release.sh` — so `rollback` warns and degrades to an
> idempotent re-install of the current bundle. To return to a specific
> version, use `upgrade --image-tag <old-version>`.
> (The `sdkwork-api-cloud-gateway` bundle does ship `release.sh`, so
> `rollback --to <version>` is effective there.)

## 6. Offline Install (bundle)

For hosts without registry/repository access, use the release artifact:

```bash
tar -xzf sdkwork-webserver-install-<version>.bundle.tar.gz
cd sdkwork-webserver-install-<version>.bundle
docker load -i image.tar.gz            # or docker pull the canonical reference
$EDITOR env/<environment>.env          # fill secrets; ports/domains have safe defaults
bash deploy.sh --environment <environment> [--replicas N]
```

Bundle content contract (`deploy.sh`, `image.env`, five-environment env matrix, compose, sha256): `DOCKER_SPEC.md` §4.

## 7. Import Mode (selected at startup)

Declare in `deployments/docker/env/<environment>.env` (default `cloud`):

```sh
SDKWORK_WEBSERVER_IMPORT_PROFILE=cloud      # import each module's nginx.cloud.<env>.conf (api-* edge → gateway upstream)
# SDKWORK_WEBSERVER_IMPORT_PROFILE=standalone   # same-origin: import nginx.standalone.<env>.conf
```

- At startup the entrypoint materializes both import sets and activates the selected one as `imports.d/import.conf` (`SDKWORK_WEBSERVER_SPEC.md` §17.3.1).
- Runtime switch (no container recreate): `pnpm import:switch:cloud|standalone`, then restart `serve-imports`; `pnpm import:status` prints the active set.
- Module PC/H5 static sources follow the active set (cloud activation serves `dist/cloud/<alias>`, standalone serves `dist/standalone/<alias>`).

## 8. Verification

```bash
curl --noproxy '*' http://127.0.0.1:13800/healthz                                   # development management plane
curl --noproxy '*' -H 'Host: api-dev.sdkwork.com' http://127.0.0.1/healthz          # platform API plane
curl --noproxy '*' -H 'Host: server-demo.sdkwork.com' http://127.0.0.1:19098/healthz
pnpm check:container-deployment    # deployment contract matrix (validate-docker-deployment.mjs)
```

**Management/import-plane ports and full verification commands for every environment: §5.1–§5.5.**

Acceptance checklist: `sdkwork-specs/DOCKER_SPEC.md` §8.

## 9. Upgrade And Rollback

```bash
bin/docker-image.sh update --image-tag <new-version>       # pull/update the image, prune dangling layers
bin/docker-deploy.sh upgrade --environment <env> [--image-tag <new-version>]
bin/docker-deploy.sh rollback --environment <env>          # re-apply the previous bundle release (webserver degrades to an idempotent re-install, see §5.6)
bin/docker-deploy.sh status   --environment <env>
bin/docker-deploy.sh logs     --environment <env>
bin/docker-deploy.sh down     --environment <env> [--purge]     # --purge requires --yes in every environment
```

Data volumes and host-system databases are never touched by the deployment scripts; rollback = re-deploying the previous bundle.
**Fully expanded, copy-paste ready commands per environment: §5.**

## 9.1 Operations Lifecycle (logs / configuration / diagnostics / backup)

```bash
# Logs: bounded read by default; --follow streams
bin/docker-deploy.sh logs --environment <env> [--instance N] [--service webserver]
bin/docker-deploy.sh logs --environment <env> --tail 1000 --since 15m
bin/docker-deploy.sh logs --environment <env> --tail 5000 --export ./incident

# Configuration: secrets redacted by default (***REDACTED***); set/edit backs up
# and validates, production requires --yes
bin/config.sh list     --environment <env>
bin/config.sh show     --environment <env>
bin/config.sh get      --environment <env> --key <KEY> [--reveal]
bin/config.sh set      --environment <env> --key <KEY> --value '<VALUE>'
bin/config.sh diff     --environment <env>      # missing / undeclared / placeholder keys
bin/config.sh validate --environment <env>
EDITOR=vi bin/config.sh edit --environment <env>

# Diagnostics: read-only, 9 checks, exit code 70 when any check fails
bin/doctor.sh --environment <env> [--json] [--export ./incident]

# Backup and restore: sets live on the target at /opt/deploy/<module>/backups/
bin/backup.sh create  --environment <env> [--no-db] [--no-volumes]
bin/backup.sh list    --environment <env>
bin/backup.sh verify  --environment <env> [--set <name>]
bin/backup.sh restore --environment <env> --set <name> --yes
```

Per-environment commands and the full action paths live in `docs/runbooks/`
(deploy / log-reference / troubleshooting / backup-restore, Chinese and
English).

## 10. Spec Index

| Topic | Authority |
| --- | --- |
| Image naming/tags, bundle layout, five-environment matrix | `sdkwork-specs/DOCKER_SPEC.md` |
| `bin/` entrypoint contract and shared library | `sdkwork-specs/MODULE_BIN_SPEC.md` |
| Import mechanism and startup mode selection | `sdkwork-specs/SDKWORK_WEBSERVER_SPEC.md` §17.3/§17.3.1 |
| External dependency standard (PostgreSQL/Redis) | `sdkwork-specs/DEPLOYMENT_SPEC.md` §6.1 |
| Public edge authority (no host nginx) | `sdkwork-specs/NGINX_SPEC.md` §0 |
