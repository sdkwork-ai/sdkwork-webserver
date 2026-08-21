# Bare-Metal And VM Installation

This guide installs the standalone Web Server binary package on Linux (and the equivalent system
scope on macOS and Windows) without Kubernetes. It covers the canonical config directory
initialization, resource deployment, a systemd unit, verification, and lifecycle operations.

On Ubuntu/Debian prefer the native installer in
[`deb-install.md`](deb-install.md) (`sdkwork-webserver[_test]_.deb`), which automates the
layout below, the PostgreSQL initialization, migration, and service registration. This guide
documents the manual archive flow for other Linux distributions and custom layouts.

Normative host layout, ownership, and permissions are owned by
`RUNTIME_DIRECTORY_SPEC.md` sections 3-4 and 11; config discovery order is owned by
`ENVIRONMENT_SPEC.md` section 8. This guide links those authorities instead of restating them.

## 1. Prerequisites

- The release archive for the target architecture (`linux-x64-standalone-server-tar-gz` or
  `linux-arm64-standalone-server-tar-gz`), built and attested per `sdkwork.workflow.json`.
- A dedicated non-root service identity. The SDKWork convention is a `sdkwork` system user;
  the service must never run as `root` (`PRD-production-operations.md`).
- PostgreSQL reachable from the host (the standalone production profile requires a real
  database; see `../database/README.md` and `ENVIRONMENT_SPEC.md` section 7).
- SHA-256 checksums and signatures from the release (checksums are mandatory per
  `sdkwork.app.config.json` security metadata).

## 2. Extract The Package

Self-contained archive installs follow the archive install root of
`RUNTIME_DIRECTORY_SPEC.md` section 4.1: `/opt/sdkwork/<application-code>`. Install the
archive under `/opt/sdkwork/webserver` (product identity), keeping the archive layout:

```bash
sudo install -d -o root -g root -m 0755 /opt/sdkwork/webserver
sudo tar -xzf sdkwork-webserver-linux-x64-standalone-server-*.tar.gz -C /opt/sdkwork/webserver
sudo chown -R root:root /opt/sdkwork/webserver
```

The package contains `bin/` (five service binaries), `etc/examples/` (safe example config and
`public/` resource), `specs/` (schema and IAM module manifest), `database/` (contract, DDL,
seeds), and `share/sdkwork/` (PC shell and dependency runtime assets).

## 3. Initialize The Canonical Config Directory

Linux service config lives at `/etc/sdkwork/webserver/` (application code `webserver`):

```bash
sudo install -d -o root -g sdkwork -m 0750 /etc/sdkwork/webserver
```

Permissions follow `RUNTIME_DIRECTORY_SPEC.md` section 11: config directory `0750`
(`root:sdkwork`), config files `0640`, secret files `0600` or `0640`, never world-readable.

## 4. Deploy The Config And Resources

1. Copy the safe example as a starting point and edit it:

   ```bash
   sudo install -o root -g sdkwork -m 0640 \
     /opt/sdkwork/webserver/etc/examples/sdkwork.webserver.config.json \
     /etc/sdkwork/webserver/sdkwork.webserver.config.json
   ```

2. Deploy every relative resource the config references **relative to the config
   directory** (the compiler resolves relative paths against the config file's parent
   directory). The example expects `public/` next to the config:

   ```bash
   sudo install -d -o sdkwork -g sdkwork -m 0755 /etc/sdkwork/webserver/public
   sudo install -o sdkwork -g sdkwork -m 0644 \
     /opt/sdkwork/webserver/etc/examples/public/index.html \
     /etc/sdkwork/webserver/public/index.html
   ```

3. Certificates and private keys are file references (`tlsPolicies[].certificateFile` etc.);
   place them under the config directory (or an approved protected path) with `0600`/`0640`
   and reference them by path. Never embed PEM in the JSON.

4. Configure the environment the way the profiles expect: set
   `SDKWORK_WEBSERVER_ENVIRONMENT`, `SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE`, and the database
   environment per `etc/topology/standalone.production.env` in the service unit (section 6).

Config resolution order (no argument or env needed when the canonical path is used):
explicit config argument → `SDKWORK_WEBSERVER_SERVER_CONFIG_FILE` → canonical OS directory
(Linux `/etc/sdkwork/webserver`, macOS `/Library/Application Support/sdkwork/webserver`,
Windows `%ProgramData%\sdkwork\webserver`) joined with `sdkwork.webserver.config.json`.
A missing canonical default fails closed with the expected path.

## 5. Validate Before Starting

```bash
sudo -u sdkwork /opt/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway validate
```

Expected output: `validated appKey=... revision=... bytes=... listeners=... virtualHosts=...`.
Run `validate` after every config change before reloading the service.

## 6. systemd Unit (Linux)

```ini
[Unit]
Description=SDKWork Web Server (standalone gateway)
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=sdkwork
Group=sdkwork
ExecStart=/opt/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway data-plane
Environment=SDKWORK_WEBSERVER_ENVIRONMENT=production
Environment=SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE=standalone
Environment=SDKWORK_WEBSERVER_RUNTIME_TARGET=server
Environment=SDKWORK_WEBSERVER_DATA_PLANE_OPERATIONS_BIND=127.0.0.1:3901
# Database settings follow ENVIRONMENT_SPEC.md section 7.1; use SDKWORK_DATABASE_* or
# password_file references, never inline credentials.
Restart=on-failure
RestartSec=5s
LimitNOFILE=65536
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadOnlyPaths=/etc/sdkwork/webserver /opt/sdkwork/webserver
PrivateTmp=true
# Add ReadWritePaths=/var/lib/sdkwork/webserver only when the service uses an
# A/B recovery directory (website runtime-set or native TLS recovery).
[Install]
WantedBy=multi-user.target
```

The `data-plane` operation requires no config argument because the canonical directory is
discovered. Start and verify:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now sdkwork-webserver
curl -fsS http://127.0.0.1:3901/readyz
journalctl -u sdkwork-webserver -f
```

## 7. Database Migration

Run migrations once before first start:

```bash
sudo -u sdkwork /opt/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway db-migrate
```

## 8. Lifecycle

- **Config change**: edit the file, run `validate`, then `sudo systemctl restart
  sdkwork-webserver` (or enable `"reload": {"mode": "watch"}` in the config for hot reload of
  same-topology generations; listener/TLS-policy changes still require restart).
- **Upgrade**: extract the new archive to a fresh directory, `validate` with the new binary,
  restart the service, then remove the old tree after a soak window.
- **Rollback**: keep the previous archive; point the unit back at the previous `ExecStart` and
  restart.
- **Uninstall**: `sudo systemctl disable --now sdkwork-webserver`, remove `/opt/sdkwork/webserver`,
  and archive or remove `/etc/sdkwork/webserver` after confirming no other SDKWork services
  share the host.

## 9. Other Platforms

| OS | Config | Adaptive Web share roots |
| --- | --- | --- |
| macOS service | `/Library/Application Support/sdkwork/webserver/` | `.../web/{pc,h5,static}/` |
| Windows service | `%ProgramData%\sdkwork\webserver\` | `%ProgramFiles%\sdkwork\webserver\web\{pc,h5,static}\` |
| Container | `deployments/docker/`, `deployments/kubernetes/` | `/app/share/sdkwork/webserver/web/{pc,h5,static}` |

Authority: `RUNTIME_DIRECTORY_SPEC.md` §4.1.1. Runtime TOML `[app_roots]` must
point at these paths (or env overrides). See `etc/README.md` for mounts.

## 10. Verification Checklist

- `validate` passes with the deployed config and resource directory.
- `/readyz` reports ready on the loopback operations listener.
- The service process runs as `sdkwork` (never root) with `NoNewPrivileges`.
- `/etc/sdkwork/webserver` is `0750 root:sdkwork`; config files `0640`; secret files `0600`/`0640`.
- No credential value appears in the unit, the config, or the environment; only file references.
