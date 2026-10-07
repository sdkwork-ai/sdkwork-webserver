# Debian / Ubuntu Installer (.deb)

This guide installs the standalone Web Server gateway from a native Debian
package on Ubuntu (verified on Ubuntu 22.04). The installer follows
`RUNTIME_DIRECTORY_SPEC.md` section 4.1 (Linux FHS layout) and
`PACKAGING_SPEC.md` section 5.5 (installer target layout); config discovery
is owned by `ENVIRONMENT_SPEC.md` section 8.

## 1. Packages And Environments

Two installer families are produced per architecture (`dist/installers/`):
`.deb` (Ubuntu/Debian) and `.rpm` (RHEL-family: RHEL/CentOS/Rocky/Alma,
`linux-rhel-*-server-rpm` package ids). Both share the same layout, the typed
TOML runtime configuration, and the environment/database model.

| Package | Environment | Domain | Public Edge | Gateway | Database |
| --- | --- | --- | --- | --- | --- |
| `sdkwork-webserver_<version>_amd64.deb` / `sdkwork-webserver-<version>-1.x86_64.rpm` | production | `server.sdkwork.com` (native edge, :80/:443) | `sdkwork-webserver-edge.service` | `127.0.0.1:8080` | `sdkwork_ai_prod` (auto-initialized) |
| `sdkwork-webserver-test_<version>_amd64.deb` / `sdkwork-webserver-test-<version>-1.x86_64.rpm` | test | `server-test.sdkwork.com` (hosts-bound) | — (plain ingress) | `0.0.0.0:8888` | `sdkwork_ai_test` (auto-initialized) |

The environment differs in the ingress port (test `8888`, production `8080`)
and in how traffic reaches the gateway: the test package binds the host name
to `127.0.0.1` through `/etc/hosts` so `http://server-test.sdkwork.com:8888`
works immediately, while the production package expects
`server.sdkwork.com` DNS to point at the host, keeps the gateway on the
loopback, and fronts it with the native public edge — the gateway's
nginx-compat data plane on `:80`/`:443` (`sdkwork-webserver-edge.service`,
section 6) — configuring ACME certificate issuance (see section 6).

## 2. Prerequisites

- Ubuntu 22.04+ (amd64/arm64) with `systemd` active.
- Root access for `dpkg`/`apt`.
- PostgreSQL is installed automatically when missing (`.deb` declares
  `Depends: postgresql`; `.rpm` declares `Requires: postgresql-server` and
  does not invoke package managers from `%post`).
- **PostgreSQL 16+ is required** (the Web Server database contract;
  `ENVIRONMENT_SPEC` section 7). Ubuntu 22.04 ships 14 by default and RHEL 9
  ships 13 — enable the `postgresql:16` module (RHEL) or the pgdg repository
  (Ubuntu) before installing. Both installers fail closed with a clear
  message when an older server is detected.

## 3. Install

```bash
sudo apt update
sudo apt install ./sdkwork-webserver-test_0.1.0_amd64.deb
# or, for production:
# sudo apt install ./sdkwork-webserver_0.1.0_amd64.deb
```

The `postinst` script performs, in order:

1. Ensures `postgresql` is installed and starts the cluster.
2. Creates the dedicated `sdkwork` system user (the service never runs as
   root).
3. Creates the environment-specific role/database/schema
   (`sdkwork_ai_test` or `sdkwork_ai_prod`) with a fresh random password stored
   at `/etc/sdkwork/webserver/secrets/database.secret` (`0600`); the TOML
   runtime config references it by path, never inlining credentials. On a
   single-application host all configuration, including the workspace
   database secret, lives under `/etc/sdkwork/webserver` (`ENVIRONMENT_SPEC.md`
   section 7.3 single-application host exception); the shared
   `/etc/sdkwork/database/` directory is only used on multi-application hosts.
4. Generates the typed runtime configuration
   `/etc/sdkwork/webserver/config.toml` (`0640`) with the profile,
   ingress (test `0.0.0.0:8888`, production `127.0.0.1:8080` behind the edge),
   runtime roots,
   database settings, and secret file references (`RUNTIME_DIRECTORY_SPEC.md`
   section 4.1 runtime config file). The gateway, `db-migrate`, and the
   certificate worker load this file at startup and materialize it into the
   process environment.
5. Runs the one-time database migration as `sdkwork`.
6. Registers and starts the `sdkwork-webserver` (or `sdkwork-webserver-test`)
   systemd service.
7. Test package: appends `server-test.sdkwork.com → 127.0.0.1` to
   `/etc/hosts`. Production package: renders the native-edge sidecar for
   ACME `http-01` and HTTPS (section 6) and starts
   `sdkwork-webserver-edge`.

## 4. Installed Layout

Per `APPLICATION_DEPLOY_LAYOUT_SPEC.md` and `RUNTIME_DIRECTORY_SPEC.md` section 4.1 (application code `webserver`).
See [`CONFIG_PATHS.md`](CONFIG_PATHS.md) for this application's paths.

| Purpose | Path | Ownership |
| --- | --- | --- |
| Private immutable runtime assets (binaries, database contract, specs, install manifest) | `/usr/lib/sdkwork/webserver` | `root:root` |
| Shared read-only assets (Adaptive Web PC/H5/static + deps) | `/usr/share/sdkwork/webserver` | `root:root` |
| Documentation / examples | `/usr/share/doc/sdkwork/webserver` | `root:root` |
| Runtime config (typed TOML) | `/etc/sdkwork/webserver` | `root:sdkwork` |
| Durable mutable data (ACME, TLS materials) | `/var/lib/sdkwork/webserver` | `sdkwork:sdkwork` |
| IAM / Drive module trees (runtime-mutated registry) | `/var/lib/sdkwork/webserver/iam`, `/var/lib/sdkwork/webserver/drive` | `sdkwork:sdkwork` |
| Logs | `/var/log/sdkwork/webserver` | `sdkwork:sdkwork` |
| Cache | `/var/cache/sdkwork/webserver` | `sdkwork:sdkwork` |
| Runtime state | `/run/sdkwork/webserver` | `sdkwork:sdkwork` |
| Workspace database secret (single-application host; `ENVIRONMENT_SPEC` §7.3 exception) | `/etc/sdkwork/webserver/secrets/database.secret` | `sdkwork:sdkwork` |
| Secret key files (encryption keys, ingress tokens) | `/etc/sdkwork/webserver/secrets/` | `sdkwork:sdkwork` |
| systemd unit | `/usr/lib/systemd/system/sdkwork-webserver[-test].service` | `root:root` |

## 4.1 Runtime Configuration (TOML)

The authoritative runtime configuration is the typed TOML file
`/etc/sdkwork/webserver/config.toml` (loaded by every binary at
startup; no `EnvironmentFile` is used). Sections:

| Section | Purpose |
| --- | --- |
| `[profile]` | deployment profile, environment, profile id, snowflake node id |
| `[ingress]` | gateway bind (test `0.0.0.0:8888`, production loopback `127.0.0.1:8080` behind the edge), expose authorization, URL trio, CORS origins |
| `[app_roots]` | `/usr/lib/sdkwork/webserver`, IAM/Drive roots, Adaptive Web PC/H5/static roots |
| `[deploy]` | Deployments domain profile, Drive facade, internal API URLs + ingress token files |
| `[database]` | workspace PostgreSQL identity (`sdkwork_ai_test`/`sdkwork_ai_prod`), `password_file` reference, auto-migrate |
| `[secrets]` | encryption key file references (production-like environments) |
| `[acme]` / `[tls]` / `[node]` / `[region]` | certificate lifecycle, TLS material roots, node uuid, region/locale |

Secret material is never inlined: the TOML references `0600` files
(`/etc/sdkwork/webserver/secrets/database.secret`,
`/etc/sdkwork/webserver/secrets/*`), which the binaries read and inject
in-process. Every configuration item, including the database connection, is
kept under `/etc/sdkwork/webserver` on this single-application host.

### 4.2 Login Page Bootstrap Token

The PC login page (`/auth/login`, reached from the Console entry) loads the
identity-service metadata endpoints
(`/app/v3/api/system/iam/runtime`, `/app/v3/api/system/iam/verification_policy`)
with a credential-entry bootstrap Access-Token (`x-sdkwork-auth-mode:
credential-entry-bootstrap`). The installer writes this token to
`/etc/sdkwork/webserver/secrets/credential-entry-bootstrap-access-token`
(`0600`, referenced from the TOML `[secrets]` section) and the gateway
injects it into the served `index.html` as an inline script.

- **Test package**: the installer generates a locally provisioned unsigned
  fixture JWT (`@sdkwork/iam-credential-entry` dev-bootstrap shape, claims
  from `sdkwork.app.config.json` backend identity). The test IAM resolver
  accepts it through the development authentication fallback, so the login
  page renders immediately after install.
- **Production package**: the installer intentionally leaves the token unset —
  the iam-credential-entry contract requires production bootstrap tokens to
  be provisioned from a private secret source (a real IAM-issued credential).
  Until a production credential provisioning path exists, the production
  login page cannot bootstrap; the identity endpoints fail closed instead.

## 5. Verify

```bash
systemctl status sdkwork-webserver-test          # test package
curl -fsS http://127.0.0.1:8888/readyz           # test ingress port
curl -fsS http://server-test.sdkwork.com:8888/      # test package (hosts-bound)
sudo -u sdkwork psql -h 127.0.0.1 -U sdkwork_ai_test -d sdkwork_ai_test -c '\dt'
```

Production package (gateway on the loopback, native edge on :80/:443):

```bash
systemctl status sdkwork-webserver sdkwork-webserver-edge sdkwork-webserver-certificate-worker
curl -fsS http://127.0.0.1:8080/readyz
curl -fsSk https://server.sdkwork.com/readyz        # edge :443 -> 127.0.0.1:8080
sudo -u sdkwork psql -h 127.0.0.1 -U sdkwork_ai_prod -d sdkwork_ai_prod -c '\dt'
```

The two packages conflict with each other; installing the other environment
removes the running one. On hosts with a shell `http_proxy`, add
`--noproxy '*'` to curl when probing the host-bound test domain.

Config changes go to `/etc/sdkwork/webserver/config.toml`; then
`sudo systemctl restart sdkwork-webserver-test`. Validate before restarting
with `sudo -u sdkwork /usr/lib/sdkwork/webserver/bin/sdkwork-api-webserver-standalone-gateway validate`.

## 6. Production HTTPS And ACME

The production package runs the **native public edge**: the gateway's
nginx-compat data plane (`sdkwork-webserver-edge.service`,
`sdkwork-api-webserver-standalone-gateway serve-nginx`) binds `:80`/`:443`
directly from a rendered sidecar at
`/etc/sdkwork/webserver/nginx.production.conf`. Stock OpenResty/nginx is never
installed — it MUST NOT serve SDKWork public domains
(`SDKWORK_WEBSERVER_SPEC.md` §0.1); the webserver process is the only edge.

- `:80` serves the ACME `http-01` webroot
  (`/var/lib/sdkwork/webserver/acme-webroot`) and redirects to HTTPS.
- `:443` terminates TLS and proxies to `127.0.0.1:8080`, where the gateway
  (`sdkwork-webserver.service`) listens on the loopback only.

Certificates are issued by the certificate worker
(`sdkwork-webserver-certificate-worker.service`). Issued and renewed material
is exported atomically to the canonical ACME live layout
(`/etc/sdkwork/certs/letsencrypt/<cert-name>/fullchain.pem` + `privkey.pem`),
which is exactly the pair the edge reads. Before first issuance,
`server.sdkwork.com` must resolve to this host and `:80` must be reachable
from the Internet. The postinst seeds a bootstrap self-signed pair at the
same canonical path so `:443` opens on cold start.

Provisioning contract for the single-domain standalone install: **name the
certificate after the domain** (`server.sdkwork.com`) when requesting it in
the console. The worker then exports the issued material in place over the
bootstrap pair, and the edge picks up the refreshed files on its next reload
(nginx-compat configurations materialize once per process, so
`systemctl restart sdkwork-webserver-edge` applies a fresh certificate):

```bash
sudo systemctl restart sdkwork-webserver-edge
```

Certificates named differently are still exported under their own
`/etc/sdkwork/certs/letsencrypt/<cert-name>/` directory; to serve one on the
edge, point the `ssl_certificate`/`ssl_certificate_key` lines of
`/etc/sdkwork/webserver/nginx.production.conf` at that directory and rerun
`validate-nginx` + restart the edge. (If `SDKWORK_CERTS_DIR` overrides the
certificates root on the host, mirror the override in the rendered edge
configuration.)
Renewal is automatic (`SDKWORK_WEBSERVER_CERT_RENEW_SCAN_INTERVAL_SECS`):
the worker exports each renewal over the same canonical path, so a single
edge restart (~60 days, during a maintenance window) is the only manual step.

## 7. Lifecycle

- **Upgrade**: `sudo apt install ./sdkwork-webserver_<new>.deb`; `postinst`
  reuses the existing database secret and migrates the schema.
- **Remove**: `sudo apt remove sdkwork-webserver-test` stops and disables the
  services; `/etc/sdkwork/webserver`,
  `/var/lib/sdkwork/webserver`, and the database are preserved.
- **Purge**: `sudo apt purge sdkwork-webserver-test` additionally removes
  `/etc/sdkwork/webserver`, `/var/lib/sdkwork/webserver`,
  `/var/log/sdkwork/webserver`, and the hosts entry.
- **Rollback**: `apt` keeps the previous `.deb`; downgrade with
  `sudo apt install ./sdkwork-webserver_<previous>.deb`.

## 8. Building The Installers

The installers are built from the standalone release archive (which now
carries every database module: Web, IAM, Drive, Deployments, Web Store):

```bash
node scripts/webserver-deb.mjs package --environment test --architecture x64
node scripts/webserver-deb.mjs package --environment production --architecture x64
node scripts/webserver-rpm.mjs package --environment test --architecture x64
node scripts/webserver-rpm.mjs package --environment production --architecture x64
node scripts/webserver-deb.mjs validate --environment test
node scripts/webserver-rpm.mjs validate --environment test
```

On Windows the scripts drive `dpkg-deb`/`rpmbuild` through WSL; the release
archive itself must be built on Linux (`scripts/webserver-release.mjs package
--deployment-profile standalone`). Outputs land in `dist/installers/` with
SHA-256 sidecars. The `.rpm` installers are verified inside a RockyLinux 9
container (database init, TOML generation, migration, gateway on the
environment ingress port).
