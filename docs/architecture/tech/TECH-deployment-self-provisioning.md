# Deployment Identity Self-Provisioning At Startup

Status: implemented (`crates/sdkwork-api-webserver-standalone-gateway/src/deployment_bootstrap.rs`).

## 1. Problem

A packaged standalone Web Server could not log a browser in on a fresh installation. The
credential-entry issuance path asked IAM for a signed Access-Token for
`SDKWORK_DEPLOY_APPLICATION_ID`, but nothing had provisioned the IAM installation that anchor
names. IAM answered:

```text
deploy application sdkwork-web is not bound to an enabled installation of tenant 100001;
provision the installation with deployApplicationId
```

The two halves of the sequence lived on different startup paths:

- IAM's manifest auto-provision (`ensure_tenant_application_from_app_root_if_configured`) ran
  while the management router was built, and anchored the installation on the manifest's
  runtime `backend.appId` (`sdkwork-webserver`).
- `issue-credential-entry-bootstrap-token` ran before the router, minted a token from
  `SDKWORK_DEPLOY_APPLICATION_ID` (a deploy application id, or a legacy slug such as
  `sdkwork-web`), and never built the router.

The two anchors could therefore never meet. A token minted for one anchor could also be
invalidated by the other path re-anchoring the installation on the next boot.

## 2. Authorities

| Concern | Authority |
| --- | --- |
| Deploy application identity, creation, publishing | `sdkwork-deployments` (`deploy_app`), `APPLICATION_FACTORY_DEPLOYMENT_SPEC.md` §3/§6 |
| One-to-one installation binding, enable/disable, release scope | `sdkwork-iam` (`iam_application_installation`), `APPLICATION_IDENTITY_SPEC.md` §2/§3, `IAM_APPLICATION_BOOTSTRAP_SPEC.md` §5.1 |
| Token claims, reuse, regeneration, packaging | `APP_ACCESS_TOKEN_SPEC.md` §3, `IAM_APPLICATION_BOOTSTRAP_SPEC.md` §5.4, `IAM_CREDENTIAL_ENTRY_SPEC.md` §4 |
| Browser runtime configuration | `BROWSER_RUNTIME_ENV_SPEC.md`, `FRONTEND_SPEC.md`, this repository's `etc/client-env.materialization.json` |
| Startup/verification placement | `DEPLOYMENT_SPEC.md`, `APPLICATION_DEPLOY_LAYOUT_SPEC.md`, `SDKWORK_WEBSERVER_SPEC.md` §17 |

The Web Server owns no application identity. It **reconciles** the deploy application and the
installation; it never invents an anchor and never creates a second identity.

## 3. Startup Sequence

`bootstrap_deployment` runs:

1. **Resolve configuration.** Application root from `SDKWORK_APP_ROOT` (the packaged root
   materializer sets it), manifest `agent.app.config.json`, lifecycle environment, tenant
   (`backend.tenantId`, default `100001`), and deployment slug (`SDKWORK_WEBSERVER_DEPLOY_APPLICATION_SLUG`
   or `app.key`).
2. **Converge the IAM subject.** `upsert_postgres_default_subject` guarantees the bootstrap
   tenant and administrator exist, so a later login resolves a real subject.
3. **Serialize concurrent starts.** A PostgreSQL advisory lock keyed by tenant + slug bounds
   duplicate provisioning work across replicas. The lock is an optimization; the unique
   indexes below are the guarantee.
4. **Fast path.** If `SDKWORK_DEPLOY_APPLICATION_ID` is a decimal deploy application anchor
   whose installation is already `enabled`, skip Deploy entirely and go to step 8.
5. **Ensure the deploy application.** List by slug, and create it through the Deploy
   service with the deterministic `Idempotency-Key`
   `sdkwork-webserver-bootstrap:<tenant>:<slug>` when absent. A concurrent creator that wins
   `uk_deploy_app_tenant_slug` is adopted by re-reading the slug. `apps.identityProvision` is
   replayed idempotently, but login does not depend on that cross-module step.
6. **Reconcile the installation.** Build the canonical IAM provision command from the
   manifest (`manifest_to_ensure_command`), then supply the *runtime-resolved* anchor
   (`deploy_application_id`) and the installation's own `iam:self` scope. An installation that
   was previously anchored on the manifest runtime id or the legacy slug is **relinked** to the
   deploy anchor (forward-only; no row is deleted or duplicated). Provisioning then selects the
   row by anchor — the same row the Deploy plane created — updates permissions and
   `application_type`, and enables it. A second row for one anchor cannot exist:
   `uk_iam_application_installation_deploy` rejects it.
7. **Verify the gate.** `validate_enabled_tenant_runtime_app` proves the installation is bound
   and enabled before any credential is signed. A non-decimal configured value
   (`sdkwork-web`) is treated as a **slug**, never as an anchor; the resolved numeric anchor
   replaces it in `SDKWORK_DEPLOY_APPLICATION_ID` for the rest of the process.
8. **Reuse or issue the token.** See §4.
9. **Materialize static identity.** See §5.
10. **Release the lock and report** a structured outcome (tenant, slug, anchor, installation,
    token path, `token_reused`, `installation_reconciled`, updated static roots).

### 3.1 Why IAM's manifest auto-provision is skipped on every lifecycle path

IAM's database lifecycle and its request-context resolver both call
`ensure_tenant_application_from_app_root_if_configured`, which anchors the installation on
the manifest runtime id, not the deploy application. Running it after step 6 would re-anchor
the enabled row every boot and detach the credential-entry token; running it before step 6
would create a competing row for the same `app_id`/`instance_key`.

Every IAM lifecycle entrypoint this gateway invokes (`db-migrate`, the management-plane
bootstrap, and `build_router`'s resolver construction) therefore removes `SDKWORK_APP_ROOT`
for that single call and restores it immediately afterwards. `SDKWORK_IAM_APP_ROOT` is
untouched, so IAM catalog materialization still resolves the packaged IAM assets.
`deployment_bootstrap` is the single installation-provisioning path.

## 4. Token Reuse And Regeneration

Reuse is the default (`IAM_APPLICATION_BOOTSTRAP_SPEC.md` §5.4.2). Startup inspects, in order:
`SDKWORK_ACCESS_TOKEN` when explicitly configured, then the secret file. A candidate is reused
only when all hold:

- non-JWT opaque values are reused unchanged;
- `exp` is absent, `0`, or later than now + 60 s;
- `tenant_id` (when present) equals the target tenant;
- `app_id` (when present) equals the resolved deploy application anchor;
- `environment` (when present) normalizes to the target lifecycle;
- an unsigned `signature` fixture is accepted in `development` only.

A reusable file token is left **byte-identical**. A reusable environment token is mirrored to
the file the browser-injection path reads, without re-signing.

Regeneration happens only when no reusable candidate exists, the claims disagree, the token is
expired, or the operator explicitly passes `--regenerate-access-token` / sets
`SDKWORK_WEBSERVER_REGENERATE_ACCESS_TOKEN=1`. The token file is written through a
same-directory temporary file and rename, mode `0600`; concurrent writers converge on one
complete file.

Automatic `serve-management` startup persists the token only where the browser-injection policy
allows it (`development`, or `test` with the documented opt-in). Staging, demo, and production
still reconcile the deploy application and installation, but leave the secret to the private
source the installer/operator owns; they never recreate the file the entrypoint purges. Explicit
operator commands (`bootstrap-deployment`, `issue-credential-entry-bootstrap-token`) persist the
token because running them is the operator's own decision.

## 5. Static Artifacts

The installer materializes `runtime-env.json` from the container environment. Startup then adds
the **non-secret** deployment identity (`tenantId`, `deployApplicationId`, `applicationCode`) to
each configured PC/H5 static root, writing atomically only when a value changes.

The Access-Token is **never** written into a built bundle, `runtime-env.json`, or HTML. It
reaches the browser only through the existing request-time injection policy
(`IAM_CREDENTIAL_ENTRY_SPEC.md` §4/§5): development, or `test` behind the documented isolated-runner
opt-in; staging, demo, and production fail closed.

## 6. Failure And Concurrency Semantics

| Failure | Outcome |
| --- | --- |
| Deploy module cannot compose, explicit anchor configured | Warn, continue with that anchor; login still lands |
| Deploy module cannot compose, no anchor | Fail with the Deploy error |
| Installation provisioning conflict | Read the bound row by anchor and update it; never insert a second row |
| Concurrent first boot | Deterministic idempotency key + unique slug; loser adopts the winner's row |
| Concurrent token writers | Atomic rename; last complete file wins |
| Advisory lock unavailable | Continue; idempotent writes and unique indexes still hold |
| `serve-management` default | `best-effort` (warn and serve); `strict` fails boot; `off` skips |

`serve-management` runs the sequence after the serve-time database lifecycle converges and
before the listener binds. The packaged entrypoint runs the explicit
`issue-credential-entry-bootstrap-token` command, which performs the same sequence and fails the
container start when it cannot land the identity.

## 7. Commands

```bash
# Full idempotent provisioning; prints a JSON outcome.
sdkwork-api-webserver-standalone-gateway bootstrap-deployment

# Explicit rotation of the credential-entry bootstrap token.
sdkwork-api-webserver-standalone-gateway bootstrap-deployment --regenerate-access-token

# Historical installer entrypoint; same sequence, first boot migrates the schema.
sdkwork-api-webserver-standalone-gateway issue-credential-entry-bootstrap-token \
  [--regenerate-access-token] [path]
```

Relevant environment:

| Key | Meaning |
| --- | --- |
| `SDKWORK_DEPLOY_APPLICATION_ID` | Deployment anchor (decimal). A legacy slug is resolved through the deploy application lookup |
| `SDKWORK_WEBSERVER_DEPLOY_APPLICATION_SLUG` | Explicit slug when the manifest key is not the deployment slug |
| `SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID` | Tenant override (default `100001`) |
| `SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_TOKEN_FILE` | Token secret path (default `/etc/sdkwork/webserver/secrets/credential-entry-bootstrap-access-token`) |
| `SDKWORK_WEBSERVER_REGENERATE_ACCESS_TOKEN` | `1`/`true` forces rotation |
| `SDKWORK_WEBSERVER_DEPLOYMENT_BOOTSTRAP` | `strict`, `best-effort` (default), or `off` for `serve-management` |

## 8. Verification

- Unit tests cover token reuse decisions (tenant/anchor/environment/expiry/fixture/opaque),
  legacy-slug resolution, environment aliases, idempotency-key stability, atomic token writes,
  and non-destructive `runtime-env.json` materialization.
- `validate_enabled_tenant_runtime_app` is the runtime gate before signing, so a failed
  installation reconciliation cannot yield a login token.
- The installer smoke lane boots the packaged gateway with the production database lifecycle and
  asserts the token command succeeds; a first boot that cannot land the identity fails the
  container/systemd start instead of serving a login page that always rejects.
