# Native Installer Packaging Guide (`bin/apps-pkg-installer`)

> 原生安装包打包操作指南 — sdkwork-webserver
> Authority: `sdkwork-specs/MODULE_BIN_SPEC.md` §4.9 · `PACKAGING_SPEC.md` §5
> Windows 不支持 `.sh`：Windows 操作者使用 `bin/apps-pkg-installer.ps1`（与本文件 sh 命令一一对应）。

## 1. What this entrypoint packages

The webserver is a **Linux server product**. Its host-native installer
channel is Linux-only; every other OS delivery is the container image.

| App type | Platform | Format | Command | Coverage |
| --- | --- | --- | --- | --- |
| `server` | `linux` | `.deb` (default) | `bin/apps-pkg-installer.sh server linux <env>` | `test` / `production` only |
| `server` | `linux` | `.rpm` | `... server linux <env> --format rpm` | `test` / `production` only |
| `server` | `windows` / `macos` | — | fails fast | container channel (`bin/docker-image.sh save`) |
| `pc` / `h5` | any | — | fails fast | static web bundles (`bin/apps-package.sh pc|h5`) |

Artifacts land in `--out` (default `target/bin-installers/`) with a sidecar
`.sha256`. 产物自动带 SHA256 校验文件。

## 2. Command execution per OS platform

### 2.1 Linux / macOS / WSL Ubuntu (POSIX sh)

```sh
bin/apps-pkg-installer.sh server linux test                       # .deb
bin/apps-pkg-installer.sh server linux production --format rpm    # .rpm
bin/apps-pkg-installer.sh server linux production --arch arm64    # arm64
bin/apps-pkg-installer.sh server linux test --dry-run             # plan only
```

### 2.2 Windows — Git Bash (bridged)

From Git Bash the sh entrypoint works unchanged: every repository command is
bridged into WSL Ubuntu automatically (path translation included). WSL Ubuntu
must be installed.

```bash
cd /e/sdkwork-space/sdkwork-webserver
bin/apps-pkg-installer.sh server linux test
```

### 2.3 Windows — PowerShell (native, no WSL prerequisite in the wrapper)

`bin/apps-pkg-installer.ps1` mirrors the sh contract 1:1. Requires
PowerShell 5.1+ and `node` on PATH. (The underlying `webserver-deb.mjs` /
`webserver-rpm.mjs` bridge `dpkg-deb`/`rpmbuild` into WSL themselves — WSL is
needed for the actual Linux packaging step, not for the wrapper.)

```powershell
pwsh bin/apps-pkg-installer.ps1 server linux test
pwsh bin/apps-pkg-installer.ps1 server linux production -Format rpm -Arch arm64
pwsh bin/apps-pkg-installer.ps1 server linux test -DryRun
```

Flag mapping: `-Arch` ↔ `--arch`, `-Format` ↔ `--format`, `-Out` ↔ `--out`,
`-DryRun` ↔ `--dry-run`.

## 3. Delegation targets (single source of truth)

| Format | Repository builder | Output dir |
| --- | --- | --- |
| `.deb` | `scripts/webserver-deb.mjs package --environment <env> --architecture <arch>` | `dist/installers/` |
| `.rpm` | `scripts/webserver-rpm.mjs package --environment <env> --architecture <arch>` | `dist/installers/` |

The entrypoints never re-implement packaging; they validate, delegate,
collect the newest artifact into `--out`, and write the checksum.

## 4. Coverage rules (fail-fast, never substitute)

- `development` / `staging` / `demo` have **no** host-native installer; the
  container path covers them:
  `bin/docker-image.sh save` → `bin/docker-deploy.sh install --environment <env>`.
- Unknown platform/format/architecture values fail before any side effect
  (`ERROR(66)/(64)`); a run that produces no artifact fails with
  `ERROR(67)` instead of "succeeding".

## 5. Install the produced .deb (Ubuntu target)

```sh
bin/apps-deploy.sh server install test --host ssh://root@<host>
bin/apps-deploy.sh server status  production --host ssh://root@<host>
```

See `deb-install.md` and `docker-install.md` for the full operator flows.

## 6. Evidence

Every run appends command, flags, and exit status to
`target/bin-evidence/evidence.log`.
