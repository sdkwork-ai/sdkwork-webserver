# Bootstrap Administrator Password Reset

Operator procedure for regaining access to the Adaptive Web console when the
platform administrator password is unknown, expired, or locked out.

Password recovery cannot travel through an authenticated surface: the operator
has lost exactly the credential that surface requires. The `reset-admin`
operation therefore writes the IAM password credential of the platform
administrator directly, with the same Argon2id parameters IAM itself uses, and
verifies the stored hash before reporting success.

## Commands

| Target | Command |
| --- | --- |
| Workspace development database (`.env.postgres`) | `pnpm admin:reset:dev -- --password "<new-password>"` |
| Installed deployment, test lifecycle | `pnpm admin:reset:test -- --password "<new-password>"` |
| Installed deployment, staging lifecycle | `pnpm admin:reset:staging -- --password "<new-password>"` |
| Installed deployment, production lifecycle | `pnpm admin:reset:release -- --password "<new-password>"` |

The new password must be at least 8 characters. It is passed to the gateway
through `SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD`, never as a process argument,
so it stays out of `ps`, `/proc/<pid>/cmdline`, and Task Manager. Exporting the
variable directly is equivalent:

```bash
SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD='<new-password>' pnpm admin:reset:dev
```

Inspect the resolved command and database target without touching the database:

```bash
pnpm admin:reset:dev --dry-run -- --password '<new-password>'
```

`--dry-run` prints the workspace database keys with every credential redacted.

## Options

| Option | Meaning |
| --- | --- |
| `--mode <dev\|release>` | `dev` reads a dotenv profile; `release` reads the runtime TOML configuration |
| `--environment <lifecycle>` | `development`, `test`, `staging`, `demo`, or `production` |
| `--username <username>` | Administrator username; default `admin` |
| `--tenant-id <tenant-id>` | Tenant id; default the platform tenant `100001` |
| `--dev-env-file <path>` | Dotenv profile for `--mode dev`; defaults to `.env.postgres`, then `.env.postgres.example` |
| `--config-file <path>` | Runtime TOML path; equivalent to `SDKWORK_WEBSERVER_CONFIG_FILE` |
| `--database-url <url>` | PostgreSQL URL override |
| `--gateway-binary <path>` | Run an installed gateway binary instead of building from source |
| `--release` | Build the gateway with `cargo --release` |

## Success output

```json
{"status":"reset","user_id":"1","tenant_id":"100001","username":"admin","credential_existed":true,"verified":true}
```

`verified` is `true` only after the stored hash was read back and accepted the
new password. `credential_existed` reports whether the administrator already had
a password credential: `false` means the environment had provisioned the
administrator subject without one, and the reset created it.

## Failure modes

| Symptom | Cause | Remedy |
| --- | --- | --- |
| `SDKWORK_WEBSERVER_ADMIN_RESET_PASSWORD is required` | No password was supplied | Pass `--password`, or export the variable |
| `admin reset password must be at least 8 characters` | Password below the shared installer floor | Choose a longer secret |
| `no administrator found in tenant <id>` | IAM never provisioned the administrator | Run `pnpm dev` or the installed bootstrap once, then retry |
| `admin reset requires PostgreSQL` | A client-local SQLite profile was resolved | Point `SDKWORK_DATABASE_*` (or `--database-url`) at the server database |
| `database connection failed` | Wrong host, port, database, or credentials | Check the dotenv profile or `--database-url`; `--dry-run` shows the resolved target |

## Bounds

- The reset writes one credential row of the platform administrator. It never
  creates tenants, users, memberships, or roles, and it never reads a private
  key.
- The operation is idempotent: repeating it replaces the password hash,
  clears `failed_attempts`, and clears `locked_until`.
- Authorization is filesystem and database access. Anyone who can read the
  dotenv profile or the runtime TOML and reach the database can reset the
  administrator, which is the same trust boundary that already owns the IAM
  schema.
