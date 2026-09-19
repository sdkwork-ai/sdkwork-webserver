# SDKWork Web Server
repository-kind: application

Standards-aligned HTTP backend for site, domain, deployment, certificate, Nginx, and Web Node management. Authors the **backend-api** and **internal-api** OpenAPI authorities (the legacy webserver `app-api` surface was retired; tenant-facing application workflows consume the Deployments `app-api` assembled into this gateway) and hosts the standalone composed gateway with the Rust request data plane.

## Framework Integration

| Framework | Status |
| --- | --- |
| `sdkwork-web-framework` | Integrated on backend-api and internal-api routers |
| `sdkwork-database` | Integrated through `database/` assets and `sdkwork-webserver-database-host` |
| `sdkwork-utils-rust` | API envelope, crypto, env parsing, slugify, serde helpers |
| `sdkwork-id-core` (via `sdkwork-database-id`) | Snowflake PKs and UUID resource identifiers |
| `sdkwork-discovery` | No current RPC transport; contract gate requires framework + discovery when RPC is introduced |
| `sdkwork-drive` | No current upload capability; contract gate rejects app-owned upload/provider lifecycle |

## Root Layout

| Directory | Purpose |
| --- | --- |
| `apis/` | Authoritative OpenAPI contracts |
| `crates/` | Rust service, repository, route, and gateway crates |
| `database/` | Database contract, baseline DDL, migrations, seeds |
| `specs/` | Component and topology contracts |
| `etc/` | Deployment index, topology profiles, runtime examples, and safe local inputs |
| `deployments/` | Docker and Kubernetes descriptors |
| `docs/` | PRD, architecture, ADRs, standards alignment |
| `tests/` | Cross-package contract tests |

## Development

```powershell
pnpm dev
pnpm check
pnpm verify
```

`pnpm dev` is server-only and selects `standalone.development`. `pnpm dev:cloud` starts only the
local Web Node Daemon against explicit remote development surfaces. Release packages remain
disabled in `sdkwork.app.config.json` until publication and production evidence are approved.

## Documentation

- Standards entry: `../sdkwork-specs/README.md`
- Alignment status: [docs/standards-alignment.md](docs/standards-alignment.md)
- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Application Roots

- [apps directory index](apps/README.md)
