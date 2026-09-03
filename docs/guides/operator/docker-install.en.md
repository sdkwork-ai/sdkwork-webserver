# sdkwork-webserver Docker Install & Deployment Guide (Three Environments · Foolproof)

> Version: 2026-09-03 · Applies to: `sdkwork-webserver` standalone unified install image
> Spec basis: `sdkwork-specs/DEPLOYMENT_SPEC.md` §6, `SDKWORK_WEBSERVER_SPEC.md` §17, `sdkwork-specs/PNPM_SCRIPT_SPEC.md` §4.4
> 中文版本：[docker-install.md](./docker-install.md)

**Core idea: one image for everything.** The image is environment-neutral (no domain/database/credentials baked in); development / test / production all run the **same image tag**. The environment is purely a deployment-time input (the env file). Changing environments = changing an env file, never rebuilding.

---

## 0. Ten-Minute Quick Start (TL;DR)

### Scenario A: fresh Docker host with an install bundle (fastest path)

```bash
# 1. Unpack the bundle and enter the directory
tar -xzf sdkwork-webserver-install-<version>.bundle.tar.gz
cd sdkwork-webserver-install-<version>.bundle

# 2. One command to deploy (embedded postgres/redis; auto image load, env generation, migrations)
bash deploy.sh --environment development   # or test / production

# 3. Verify
curl http://127.0.0.1:13800/healthz        # expect {"status":"ok"}

# 4. Open in a browser
#    dev http://127.0.0.1:13800  test http://127.0.0.1:18888  production http://127.0.0.1:18080
```

> The first run generates `env/<environment>.env` from the `.env.example` — **replace every `<CHANGE_ME>` with real secrets before exposing anything beyond localhost**.

### Scenario B: build a new image in the repository + deploy three environments (build machine path)

From the repository root (WSL ext4 environment, see §2.4):

```bash
# 1. Build the new image (tag version comes from sdkwork.app.config.json currentVersion)
pnpm build:container:standalone -- --skip-platform-gateway

# 2. Deploy all three environments (development/test/production in one go)
bash scripts/docker/deploy-docker-environment.sh all --validate

# 3. Verify (expect all 200)
for p in 13800 18888 18080; do curl -s --noproxy '*' http://127.0.0.1:$p/healthz; echo; done
```

### Access entry points after deployment

| Environment | Management plane (SPA+API same-origin, recommended) | healthz | Data plane (domain-Host routing) |
| --- | --- | --- | --- |
| Development | http://127.0.0.1:13800 | `/healthz` | `http://server-dev.sdkwork.com:80` (Host: server-dev.sdkwork.com) |
| Test | http://127.0.0.1:18888 | `/healthz` | `http://server-test.sdkwork.com:18898` (Host: server-test.sdkwork.com) |
| Production | http://127.0.0.1:18080 | `/healthz` | `http://server.sdkwork.com:18098` (Host: server.sdkwork.com) |

> For the domain form, point the three domains to `127.0.0.1` (or the WSL IP) in your hosts file. The data plane routes by domain; a direct request without a Host header lands on the default site. Management ports have no such requirement and are the easiest test entry.

---

## 1. Prerequisites & One-Time Preparation

| Dependency | Requirement | Check |
| --- | --- | --- |
| Docker | 24+, daemon reachable (inside WSL is fine) | `docker version` |
| Node.js | 22+ (build machine only) | `node -v` |
| pnpm | 10+ (build machine only) | `pnpm -v` |
| Rust toolchain | cargo 1.8x (image build only) | `cargo --version` |
| Ports | 13800/18888/18080 + data-plane ports free | `ss -ltn \| grep -E '13800\|18888\|18080'` |

One-time preparation:

```bash
# 1. Env files: copy from the examples (bundle deploy.sh does this automatically; repo chain is manual)
cd deployments/docker/env
cp development.env.example development.env
cp test.env.example test.env
cp production.env.example production.env

# 2. Replace every <CHANGE_ME> with real secrets (database passwords, session keys, ...)
grep -n 'CHANGE_ME' development.env test.env production.env

# 3. Host mount directory (module import plane)
sudo mkdir -p /opt/deploy
```

---

## 2. Building a New Image Package

### 2.1 Where the version lives

The image tag version comes from **`sdkwork.app.config.json` → `release.currentVersion`**. Bump that one place for a new release:

```bash
# Example: 0.1.0 → 0.1.1
node -e "const f='sdkwork.app.config.json';const j=require('./'+f);j.release.currentVersion='0.1.1';require('fs').writeFileSync(f,JSON.stringify(j,null,2)+'\n')"
```

### 2.2 Route A: repository release chain (standalone image)

```bash
# All-in-one: release build + standalone image (tag = registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version>)
pnpm build:container:standalone

# Skip the embedded gateway when the gateway runs as a separate container (attach/docker):
node scripts/docker/build-standalone-image.mjs --skip-platform-gateway

# Re-run only the release archive (tar.gz + SBOM), no image build:
node scripts/webserver-release.mjs package --deployment-profile standalone
```

Artifacts:

```text
dist/release/sdkwork-webserver-linux-x64-standalone-server-<version>.tar.gz   # install archive + SBOM
docker image registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version> # unified image
```

### 2.3 Route B: self-contained install bundle (deliver to any Docker host)

```bash
pnpm build:container:install                       # build image + package bundle
pnpm build:container:install -- --skip-image-build # reuse a built image, repackage only
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

### 2.4 WSL / DrvFS build notes (important — mandatory on Windows-mounted sources)

If the source lives under `/mnt/<drive>` (a Windows drive mount), **never run pnpm/cargo there directly**: the pnpm store's SQLite over the 9p filesystem always fails with `disk I/O error`. The correct approach is an rsync copy on the WSL ext4 filesystem:

```bash
# 1. Sync the repo plus every sibling repository referenced by pnpm-workspace.yaml
#    (a missing one yields ERR_PNPM_WORKSPACE_PKG_NOT_FOUND; see pnpm-workspace.yaml)
mkdir -p ~/sdkwork-build && rsync -a \
  --exclude target --exclude 'node_modules*' --exclude dist --exclude .git \
  /mnt/e/sdkwork-space/sdkwork-webserver/ ~/sdkwork-build/sdkwork-webserver/
#    Also sync each ../<repo> sibling referenced by pnpm-workspace.yaml + ../sdkwork-specs + ../sdkwork-github-workflow

# 2. Install dependencies inside the ext4 copy (first run ~5min)
cd ~/sdkwork-build/sdkwork-webserver && pnpm install

# 3. Rust dependency closure: whenever cargo reports a missing sibling repo,
#    rsync that single repo from /mnt/e and retry
#    (add [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env" at script top, else cargo ENOENT)

# 4. Browser static assets: rsync excluded dist, so build the UI dependency closure first
pnpm --filter "@sdkwork/webserver-pc..." --filter "@sdkwork/webserver-h5..." build

# 5. Run the §2.2 packaging commands inside ~/sdkwork-build/sdkwork-webserver
```

Copy artifacts back into the repository directory (compose volume mounts depend on repo paths):

```bash
cp ~/sdkwork-build/sdkwork-webserver/target/release/sdkwork-api-webserver-standalone-gateway \
   /mnt/e/sdkwork-space/sdkwork-webserver/target/release/
```

### 2.5 Artifact verification

```bash
sha256sum dist/release/*.tar.gz                     # record in the release report
docker images | grep sdkwork-webserver-standalone  # confirm the new tag exists
```

---

## 3. Environment Configuration (environment = env file)

Env file locations: repo chain `deployments/docker/env/<env>.env`; bundle chain `env/<env>.env` (generated by deploy.sh). Full key reference: comments in each `.env.example` and [CONFIG_PATHS.md](./CONFIG_PATHS.md).

### 3.1 Required keys cheat sheet

| Key | Purpose | Default / example |
| --- | --- | --- |
| `SDKWORK_WEBSERVER_IMAGE_TAG` | Image tag (identical across environments = "one image") | `0.1.0` |
| `SDKWORK_WEBSERVER_*_HOST_PORT` | Management port per environment | dev 13800 / test 18888 / prod 18080 |
| `SDKWORK_WEBSERVER_*_IMPORT_HTTP_HOST_PORT` | Data-plane HTTP port per environment | dev 80 / test 18898 / prod 18098 |
| `WEBSERVER_POSTGRES_*` / `PG_MAX_CONNECTIONS` | Embedded postgres (embedded mode) | `<CHANGE_ME>` must be replaced |
| `WEBSERVER_POSTGRES_HOST` / `WEBSERVER_REDIS_HOST` | External instances (external mode) | `host.docker.internal` |
| `SDKWORK_WEBSERVER_PRIMARY_DOMAIN` | Primary domain (data-plane routing / CORS basis) | `sdkwork.com` |
| `SDKWORK_CORS_ALLOWED_ORIGINS` | Allowed CORS origins | includes all three environments |
| `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT` | /api/ gateway mode | `bundled` (in-image) \| `docker` (sibling container) \| external (attach) |
| `SDKWORK_DATABASE_SEED_LOCALE` | Seed-data locale | `zh-CN` |

### 3.2 Attaching an independently deployed gateway

Module `/api/` reverse proxying **literally connects to** `sdkwork-api-cloud-gateway:8080` (never rewritten, SDKWORK_WEBSERVER_SPEC §17.3). An independent gateway fleet must provide: network alias `sdkwork-api-cloud-gateway` + in-container listener on 8080. In the env file:

```bash
SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT=external
SDKWORK_MODULE_API_GATEWAY_HOST=sdkwork-api-cloud-gateway
SDKWORK_MODULE_API_GATEWAY_PORT=8080
# Attach network: must match the actual gateway fleet network name
# (bundle fleets use sdkwork-api-cloud-gateway-<env>)
SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK=sdkwork-api-cloud-gateway-development   # same for test/production
```

Self-check: `docker exec sdkwork-webserver-development getent hosts sdkwork-api-cloud-gateway` must resolve; `docker exec <gateway container> curl -s http://127.0.0.1:8080/readyz` must return 200.

---

## 4. One-Command Deployment (Three Environments)

### 4.1 Repo chain: `deploy-docker-environment.sh` (recommended on build machines)

```bash
bash scripts/docker/deploy-docker-environment.sh development --validate
bash scripts/docker/deploy-docker-environment.sh test        --validate
bash scripts/docker/deploy-docker-environment.sh production  --validate
# Or all three at once (development/test/production):
bash scripts/docker/deploy-docker-environment.sh all --validate

# Other operations
bash scripts/docker/deploy-docker-environment.sh staging          # single-target deploy
bash scripts/docker/deploy-docker-environment.sh all --down       # stop all three
bash scripts/docker/deploy-docker-environment.sh all --pull       # pull images before up
```

Rules:

- A missing env file fails fast (with copy-from-example hints); no half-deployed state.
- `--validate` checks env completeness before compose up.
- Success prints `deployed <env> (sdkwork-webserver-<env>) -> http://127.0.0.1:<port>/healthz`.

### 4.2 Bundle chain: `deploy.sh` (any Docker host, multi-instance capable)

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
- If the image is not loaded yet, the script runs `docker load image.tar.gz` automatically.
- Instance 1 starts first and waits until healthy (including database migrations); instances 2..N start afterwards, avoiding migration races.

Repository-root equivalent: `pnpm deploy:apply:standalone:docker -- --environment production --replicas 3`

---

## 5. Post-Deployment Verification (3 minutes)

### 5.1 Health checks (copy-paste ready)

```bash
# ① All containers should be healthy
docker ps --format '{{.Names}}\t{{.Status}}' | grep sdkwork-webserver

# ② Management healthz should all be 200 {"status":"ok"}
for p in 13800 18888 18080; do
  echo -n "$p -> "; curl -s --noproxy '*' http://127.0.0.1:$p/healthz; echo
done

# ③ Data-plane SPA (domain-Host routing) should all be 200 with HTML
curl -s --noproxy '*' -o /dev/null -w 'dev  %{http_code}\n' -H 'Host: server-dev.sdkwork.com'  http://127.0.0.1/
curl -s --noproxy '*' -o /dev/null -w 'test %{http_code}\n' -H 'Host: server-test.sdkwork.com' http://127.0.0.1:18898/
curl -s --noproxy '*' -o /dev/null -w 'prod %{http_code}\n' -H 'Host: server.sdkwork.com'      http://127.0.0.1:18098/

# ④ Gateway attach contract (external mode)
docker exec sdkwork-webserver-development getent hosts sdkwork-api-cloud-gateway
```

> On WSL hosts with an `http_proxy` configured, curl MUST use `--noproxy '*'`, otherwise results are unreliable.

### 5.2 Multi-Instance Topology (supported for every environment)

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

- Per-instance node identity: `SDKWORK_WEBSERVER_NODE_UUID=standalone-<env>-i<i>`.
- Load balancing across instances: balance the per-instance management ports; the 80/443 edge lives only on instance 1.
- Multi-instance prerequisite: shared PostgreSQL / Redis (embedded or external); instance 1 performs migrations.

### 5.3 Per-Instance Configuration (optional)

```text
env/production.env            # environment-level base config (shared by all instances)
env/production.i1.env         # instance-1 overrides (optional)
env/production.i2.env         # instance-2 overrides (optional)
```

When deploy.sh detects `env/<environment>.i<N>.env`, it layers it as a second `--env-file` (the later file wins). Typical uses: different primary domain, clone URL, or TLS/ACME profile per instance. Management ports and node identity are always assigned by the script per instance.

---

## 6. Day-2 Operations

### 6.1 Upgrading to a new image

```bash
# 1. Build the new image per §2 (bump currentVersion → build)
# 2. Update SDKWORK_WEBSERVER_IMAGE_TAG=<new version> in all three env files
# 3. Recreate in place (data volumes preserved; migrations run at container start)
bash scripts/docker/deploy-docker-environment.sh all --validate
# 4. Re-run the §5.1 checks
```

> Repo-chain note: `deployments/docker/docker-compose.<env>.yml` bind-mounts the host `target/release/sdkwork-api-webserver-standalone-gateway` read-only into the container (hybrid mode). If you use this path, copy the newly built binary to that location on upgrade (last step of §2.4). Pure-image deployment (bundle chain) has no such step.

### 6.2 Stop / clean up

```bash
bash scripts/docker/deploy-docker-environment.sh all --down   # stop (volumes kept)
# Full cleanup (careful: deletes data volumes)
docker compose -p sdkwork-webserver-development down --volumes
```

### 6.3 Rollback

Point `SDKWORK_WEBSERVER_IMAGE_TAG` back to the old tag in the env file and `up` again (instant if the old image is still local; database rollback follows the `database/` migration policy separately).

---

## 7. Troubleshooting (symptom → cause → fix)

| Symptom | Cause | Fix |
| --- | --- | --- |
| `pnpm install` fails with `disk I/O error` | pnpm run on DrvFS (/mnt/e) | Build in an ext4 copy per §2.4 |
| `ERR_PNPM_WORKSPACE_PKG_NOT_FOUND @sdkwork/...` | ext4 copy missing a sibling repo | rsync the missing `../<repo>` per pnpm-workspace.yaml |
| `spawnSync cargo ENOENT` | non-login shell lacks PATH | Add `. "$HOME/.cargo/env"` at script top |
| TS cannot find `@sdkwork/ui-pc-react` types | rsync excluded dist | Run `pnpm --filter "@sdkwork/webserver-pc..." build` first |
| `port is already allocated` | old fleet holds the port | Change the env port or `--down` the old stack first |
| `network ... not found` | attach network name mismatch | Check `docker network ls`, update `SDKWORK_MODULE_API_GATEWAY_ATTACH_NETWORK` in env |
| webserver won't start; mount point became a directory | target/release binary missing; docker created a same-named dir | `rmdir` it, then copy the binary per §2.4 |
| Gateway container logs `invalid reference format` | env has unfilled `GATEWAY_IMAGE=...:<VERSION>` placeholder | Fill the real image tag (e.g. `:local`) |
| Production gateway crash loop, logs contain `must contain sslmode=require` | P0-12 production DB enforces TLS | Add `sslmode=require` to the DB URL (embedded postgres has ssl on); or set `GATEWAY_POSTGRES_SSL_MODE=require` |
| Gateway logs `requires username "sdkwork_ai_prod"` | DB URL username ≠ embedded postgres `POSTGRES_USER` | Use username `sdkwork_ai_prod`; password from `GATEWAY_POSTGRES_PASSWORD` (URL-encode special chars) |
| Occasional curl 000/timeout despite healthy service | host proxy (http_proxy=127.0.0.1:7897) intercepts | curl with `--noproxy '*'` |
| dev/test data-plane 443 connects then closes | only the production sidecar declares a 443 listener; dev/test 443 is a dead mapping | Expected; use the HTTP data-plane ports for dev/test |
| TLS client with ALPN h2-only gets EOF | rustls data plane lacks h2 fallback (known issue) | Client ALPN `h2,http/1.1`; curl with `--no-alpn` |
| Leftover containers after `--down` | dynamic discovery of multi-instance projects | `docker ps -a \| grep <app>-<env>-i`, then `docker rm -f` each |

---

## 8. Design Overview

`pnpm build:container:install` produces **one unified install bundle** (self-contained install bundle):

- **One image**: the image is environment-neutral. Nothing about the lifecycle environment, domain, database, or credentials is baked at build time. The environment and the instance count are **deployment-time inputs**, resolved by the container entrypoint at start.
- **Any environment**: development, test, and production all run the same image tag; the environment is selected through the env file at deploy time.
- **Every environment supports multi-instance**: N instances share one network and one set of secrets/data volumes; each instance owns a distinct compose project name, node identity, and management port. Only instance 1 publishes the 80/443 edge ports and runs database migrations first.

## 9. Relation to Existing Commands

| Command | Purpose |
| --- | --- |
| `pnpm build:container:standalone` | Build the unified install image only (no bundle) |
| `pnpm build:container:install` | Build + package the self-contained install bundle |
| `pnpm deploy:apply:standalone:docker` | Run the bundle deploy.sh from the repository root |
| `scripts/docker/deploy-docker-environment.sh` | Repo-chain one-command deploy for the three environments (external layout) |
| `build:container:*` / `deploy:apply:*` | New automation MUST use these entrypoints |

## 10. Webserver Spec Compliance (SDKWORK_WEBSERVER_SPEC.md)

| Spec point | Implementation |
| --- | --- |
| §17 space mounts | The space root `/opt/deploy` is mounted read-only; the `sdkwork-space` checkout subtree is a read-write overlay (the entrypoint clone/pull target) |
| §17 module import plane | `SDKWORK_SPACE_AUTO_DISCOVER` / `SDKWORK_SPACE_MODULES` / `MODULE_IMPORT_REQUIRED` / `PROBE_UPSTREAMS` are passed through; module static assets resolve from the checkout at `apps/*-{pc,h5}/dist/standalone/<envAlias>/` |
| §17.3 import sets | `SDKWORK_WEBSERVER_IMPORT_PROFILE` defaults to `cloud` (dual imports.d sets, materialized at start); `SDKWORK_WEBSERVER_IMPORT_LISTENER_PORTS` passes through (identity 80/443 by default) |
| §17 multi-cluster / multi-instance | Every container listens on gateway port 3800 internally; host ports differ per environment/instance |
| §17.4 standalone-only | The image/bundle is packaged from the standalone release artifact only (`webserver-release.mjs --deployment-profile standalone`) |
| §8.1 gateway upstream | Module `/api/` reverse proxying goes through the reserved upstream `gateway`; `SDKWORK_MODULE_API_GATEWAY_DEPLOYMENT` selects `bundled`/`docker`/`external` |

Verified deployment report: [docs/reports/2026-09-03-webserver-docker-packaging-verification.md](../../reports/2026-09-03-webserver-docker-packaging-verification.md)
