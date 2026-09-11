# bin/ — standardized entrypoints (`sdkwork-specs/MODULE_BIN_SPEC.md`)

`sdkwork-webserver` is the standalone-only public edge
(`SDKWORK_WEBSERVER_SPEC.md` §17.4); it is also the only module that owns
browser surfaces. Shared behavior lives in
`sdkwork-specs/bin/lib/sdkwork-common.sh`; this directory only carries
identity and delegation.

| Script | Purpose |
| --- | --- |
| `docker-image.sh` | build / push / save / load / update / inspect `registry.sdkwork.com/apps/sdkwork-webserver-standalone:<version>` |
| `docker-deploy.sh` | install / upgrade / rollback / status / logs / down / start / stop / restart the Docker bundle on `wsl` or `ssh://[user@]host` |
| `apps-build.sh` | build `pc` / `h5` (canonical browser runner) or `server` (cargo) |
| `apps-package.sh` | package surfaces into `target/bin-packages/` (+ sidecar `.sha256`) |
| `apps-deploy.sh` | deploy packaged apps to WSL Ubuntu / remote Ubuntu |
| `apps-pkg-installer.sh` | package native OS installers (`server` + `linux` → `.deb`/`.rpm` via `webserver-deb.mjs` / `webserver-rpm.mjs`) into `target/bin-installers/` (+ sidecar `.sha256`) |
| `build-apps-static.sh` | build PC/H5 static dist for **any sibling workspace module** (`sdkwork-im`, `sdkwork-community`, …) via the canonical browser runner; implementation in `bin/lib/apps-static.sh` |
| `deploy-apps-static.sh` | build + package (tar.gz+sha256) + publish static dist to a target host (`wsl` / `ssh://…`), with extraction, integrity verification, atomic `current` switch and quick rollback; implementation in `bin/lib/apps-static-deploy.sh` |

Declared app types: `pc,h5,server`. Default image tag comes from
`sdkwork.app.config.json` → `release.currentVersion`.

## Sibling-module static builds (`build-apps-static.sh`)

Build the Adaptive Web static dist (PC / H5) of an independent sibling module
directly from this bin/ directory — the build always delegates to the
canonical runner (`sdkwork-specs/tools/build-browser-client.mjs`,
PNPM_SCRIPT_SPEC.md §4.2), so env materialization and the
`dist/<profile>/<envAlias>` layout match every other browser build:

```sh
bin/build-apps-static.sh --list                       # which modules own buildable pc/h5 apps
bin/build-apps-static.sh im                           # sdkwork-im, pc + h5, dev:standalone
bin/build-apps-static.sh sdkwork-im h5 prod           # h5 only, production standalone
bin/build-apps-static.sh im all test:cloud            # both archs, cloud profile
bin/build-apps-static.sh im --skip-typecheck          # fast iteration build (no vue-tsc)
bin/build-apps-static.sh im --out target/static --tar # copy dist + tar.gz (+ .sha256)
bin/build-apps-static.sh im --clean --dry-run         # plan only
```

Output lands in `<module>/apps/<app>/dist/<profile>/<envAlias>/`; `--out`
copies it to a self-describing folder
(`<module>-<arch>-<profile>-<env>/`). Module names accept the short form
(`im` → `sdkwork-im`).

## Static publish + rollback (`deploy-apps-static.sh`)

End-to-end: build (or `--no-build`) → `tar.gz` + sidecar `.sha256` → upload
to the target → `sha256sum -c` on the target → extract into an immutable
`releases/<UTC-timestamp>/` → prune (`--keep`, default 5) → atomic
`current` symlink switch. Target layout:
`<target-root>/<module>-<arch>-<profile>-<env>/{releases,current,incoming}`
(default root `/opt/deploy/sdkwork-static-apps`).

```sh
bin/deploy-apps-static.sh im h5 test --host wsl                     # build + publish (local WSL)
bin/deploy-apps-static.sh im all prod --host ssh://ops@10.0.0.8 --yes   # remote host
bin/deploy-apps-static.sh im h5 test --host wsl --no-build --keep 3     # reuse dist, tighter retention
bin/deploy-apps-static.sh im h5 prod status --host ssh://ops@10.0.0.8   # current + release list
bin/deploy-apps-static.sh im h5 prod rollback --host ssh://ops@10.0.0.8 --yes        # one step back
bin/deploy-apps-static.sh im h5 prod rollback --to 20260910T071500Z --host wsl --yes # pin a release
```

`rollback` re-points `current` with `ln -sfn` (atomic); production deploys
and rollbacks require `--yes`. Every run appends an evidence line to
`target/bin-evidence/evidence.log`.

## Container path (every environment)

```sh
bin/docker-image.sh build                                  # → build:container:standalone --tag <version>
bin/docker-image.sh save -o dist/sdkwork-webserver.tar.gz  # air-gapped bundle
bin/docker-deploy.sh install --environment production --yes --host ssh://ops@10.0.0.8
bin/docker-deploy.sh status  --environment demo
bin/docker-deploy.sh logs    --environment demo
bin/docker-deploy.sh rollback --environment demo
bin/docker-deploy.sh down    --environment demo --purge --yes
```

### Five environments — copy-paste ready

| Environment | Management | Import HTTP | HTTPS | Domains | Database |
| --- | --- | --- | --- | --- | --- |
| `development` | `13800` | `80` | `443` | `server-dev.*`, `*-dev.*` | `sdkwork_ai_dev` |
| `test` | `18888` | `18898` | `28430` | `server-test.*`, `*-test.*` | `sdkwork_ai_test` |
| `staging` | `18081` | `18099` | `38431` | `server-staging.*`, `*-staging.*` | `sdkwork_ai_staging` |
| `demo` | `19080` | `19098` | `38432` | `server-demo.*`, `api-demo.*` | `sdkwork_ai_demo` |
| `production` | `18080` | `18098` | `38430` | `server.*`, `api.*` | `sdkwork_ai_prod` |

```sh
bin/docker-deploy.sh install --environment development            # 13800
bin/docker-deploy.sh install --environment test                   # 18888
bin/docker-deploy.sh install --environment staging                # 18081
bin/docker-deploy.sh install --environment demo                   # 19080
bin/docker-deploy.sh install --environment production --yes       # 18080 (--yes required)
```

> **Universal import plane (default all five).** Every webserver instance,
> whatever environment it runs in, imports each sibling module's
> `nginx.<profile>.<environment>.conf` sidecar for **all five** lifecycle
> environments by default — so the `development` stack also routes
> `server-test.*` / `api-staging.*` etc. (`ENVIRONMENT_SPEC.md` §6.2.2,
> `SDKWORK_WEBSERVER_SPEC.md` §17.3.2). `SDKWORK_WEBSERVER_ENVIRONMENT` only
> selects the instance's default routing environment. To deliberately serve a
> subset, set `SDKWORK_WEBSERVER_IMPORT_ENVIRONMENTS` (comma-separated) in the
> env file; compose forwards it into the container
> (`docker-compose.bundle.yml`). Verify the actual imported set in a running
> container: `docker exec <c> cat /etc/sdkwork/webserver/imports.d/import.conf`.
> To run a second webserver slot at different ports for a different app, deploy
> with `--host-port <base> [--edge-http <p>] [--edge-https <p>]`.

> **Rollback**: the bundle ships `release.sh` (OPERATIONS_SPEC.md §1.2), so
> `rollback` goes back to the previous successful version recorded in the
> append-only release ledger, gated by `/healthz` probes on the management
> ports. To pin an explicit version use `--to <old-version>`. Release history:
> `bash release.sh history --environment <env>` on the target bundle.
> Per-environment command blocks (upgrade / rollback / status / logs / down /
> verification) are in
> [`docs/guides/operator/docker-install.md`](../docs/guides/operator/docker-install.md) §5.

The bundle is synced to `/opt/deploy/sdkwork-webserver/bundle` on the target
and `deploy.sh` is executed with that directory as the working directory.
`--image-tag`, `--replicas`, `--deps external|embedded` and `--purge` are
forwarded to the bundle entrypoint.

## Application path

```sh
bin/apps-build.sh   pc   demo:standalone        # → dist/standalone/demo
bin/apps-build.sh   h5   prod:cloud --dry-run
bin/apps-build.sh   server test                 # → cargo build --release
bin/apps-package.sh pc   demo:standalone --out target/bin-packages
bin/apps-package.sh server test                 # → dist/installers/*.deb (test|production only)
bin/apps-deploy.sh  server install test --host ssh://root@10.0.0.8
bin/apps-deploy.sh  server status  production

bin/apps-pkg-installer.sh server linux test                       # → dist/installers → target/bin-installers/*.deb
bin/apps-pkg-installer.sh server linux production --format rpm    # → target/bin-installers/*.rpm
bin/apps-pkg-installer.sh server linux production --arch arm64 --dry-run
```

- The host-native `.deb` installer channel covers **test and production
  only** (`webserver-deb.mjs`). Every other environment uses the container
  path above; the entrypoint fails fast with that guidance instead of
  substituting another environment's artifact.
- `apps-pkg-installer.sh` follows the same coverage rule for both `--format
  deb` (default) and `--format rpm`; Linux is the only native-installer
  platform this module ships — Windows/macOS server delivery is the container
  image channel, and `pc`/`h5` are static web bundles (no OS installer).
- `pc` / `h5` are served by the webserver static root, so `apps-deploy.sh`
  reports the delivery channel and performs no host mutation.
- The remote account for `apps-deploy.sh server` needs root privileges
  (`apt-get` + systemd); no `sudo` is injected.

## Windows (PowerShell, no WSL prerequisite)

Windows does not run `.sh`; the installer packaging contract ships as
`bin/apps-pkg-installer.ps1` for native Windows operators (PowerShell 5.1+,
`node` on PATH):

```powershell
# Linux .deb (test/production only; the packager bridges dpkg-deb via WSL itself)
pwsh bin/apps-pkg-installer.ps1 server linux test
pwsh bin/apps-pkg-installer.ps1 server linux production -Format rpm -Arch arm64

# Plan only
pwsh bin/apps-pkg-installer.ps1 server linux test -DryRun
```

Flags map 1:1 to the sh entrypoint (`-Arch` ↔ `--arch`, `-Format` ↔
`--format`, `-Out` ↔ `--out`, `-DryRun` ↔ `--dry-run`). Windows/macOS server
delivery stays on the container channel; `pc`/`h5` are static web bundles.

## Shared flags

`--environment development|test|staging|demo|production` ·
`--profile standalone|cloud` (as `<environment>:<profile>` for apps-\*) ·
`--host wsl|ssh://[user@]host[:port]` · `--image-tag <v>` ·
`--deps external|embedded` · `--host-port <BASE>` · `--edge-http <P>` ·
`--edge-https <P>` · `--domain <HOST>` · `--to <version>` · `--purge` ·
`--yes` · `--dry-run` · `-h|--help`

`--host-port/--edge-http/--edge-https/--domain` are runtime port/domain
overrides for `install|upgrade` (MODULE_BIN_SPEC.md §4.2). Precedence:
**CLI > env-file > built-in fallback**. They are forwarded to the bundle
`deploy.sh`, which composes on them and persists them into the env file on
apply — so a later `doctor`/`status`/`config` reads the same published ports.
Binding the same environment at different ports for different applications is a
deploy with a different base:

```sh
bin/docker-deploy.sh install --environment demo --host-port 19500 \
    --edge-http 19510 --domain demo.slot.example   # second app slot at :19500
```

`--dry-run` prints the plan and executes nothing. Production mutations require
`--yes`; `--purge` requires `--yes` in every environment. Each run appends a
line with its exit status to `target/bin-evidence/evidence.log`.

Run `bin/<script>.sh doctor` for the environment self-check.

## Operations lifecycle

```bash
bin/config.sh  <list|show|get|set|diff|validate|edit> --environment <env> [--key K] [--value V] [--reveal]
bin/doctor.sh  --environment <env> [--instance N] [--json] [--export <dir>]
bin/backup.sh  <create|list|verify|restore> --environment <env> [--set <name>] [--component all|config|database|volumes]
bin/docker-deploy.sh logs --environment <env> [--instance N] [--service <s>] [--tail N|all] [--since <d>] [--follow] [--export <dir>]
```

`config.sh` reads the live bundle configuration on the target, redacts secrets,
and backs up before mutating. `doctor.sh` is read-only and exits 70 when a
check fails. `backup.sh` writes checksummed sets to
`/opt/deploy/<module>/backups/` on the target. Runbooks: `docs/runbooks/`.
