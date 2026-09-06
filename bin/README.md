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

Declared app types: `pc,h5,server`. Default image tag comes from
`sdkwork.app.config.json` → `release.currentVersion`.

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
`--deps external|embedded` · `--to <version>` · `--purge` · `--yes` ·
`--dry-run` · `-h|--help`

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
