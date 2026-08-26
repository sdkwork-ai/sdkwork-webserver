# Repository Guidelines

<!-- SDKWORK-AGENTS-GENERATED: v2 -->

## SDKWORK Soul

Read `../sdkwork-specs/SOUL.md` before executing tasks in this root. Follow specs before memory, dictionary before context, stop on ambiguity, and evidence before completion.

## SDKWORK Standards

<!-- SDKWORK-PROGRESSIVE-LOADING: v1 -->
Resolve this standards root once and use it as the global authority for the current task:

- `../sdkwork-specs/README.md`
- `../sdkwork-specs/SOUL.md`
- `../sdkwork-specs/AGENTS_SPEC.md`

Read only the relevant README task-matrix row or navigation heading, then load the selected authority sections.
<!-- /SDKWORK-PROGRESSIVE-LOADING: v1 -->

Canonical SDKWORK specs path from this root:

- `../sdkwork-specs/README.md`
- `../sdkwork-specs/SOUL.md`
- `../sdkwork-specs/AGENTS_SPEC.md`
- `../sdkwork-specs/PNPM_SCRIPT_SPEC.md`
- `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`
- `../sdkwork-specs/CODE_STYLE_SPEC.md`
- `../sdkwork-specs/NAMING_SPEC.md`

Do not copy root standard text into this repository. If these relative paths do not resolve, stop and report the broken workspace layout.

## Application Identity

Read `sdkwork.app.config.json` for Web Server identity, registration, SDK/API inventory, release metadata, packaging capability, or app-owned capabilities. Read `etc/` for concrete environment, bind, upstream, runtime, and deployment values. The app manifest is not runtime configuration authority.

## Deployment Profile (Standalone-Only)

> Manual note — keep this section when AGENTS.md is regenerated.

`sdkwork-webserver` is **standalone-only** (`SDKWORK_WEBSERVER_SPEC.md` §17.4) and is the **only** public reverse-proxy edge (`SDKWORK_WEBSERVER_SPEC.md` §0.1, `NGINX_SPEC.md` §0):

- Stock OpenResty/nginx and `/etc/nginx` `MUST NOT` serve SDKWork public domains. Uninstall host nginx (`deployments/docker/scripts/uninstall-wsl-nginx.sh`); `install-wsl-nginx.sh` is retired and only invokes uninstall. Docker development publishes host `:80`/`:443`.
- `sdkwork.app.config.json` declares `runtime.supportedDeploymentProfiles = ["standalone"]`; there is no cloud build, cloud package, or cloud runtime-env surface in this repository.
- Its browser applications (`apps/sdkwork-webserver-pc`, `apps/sdkwork-webserver-h5`) build with the canonical runner at `build:pc|h5:<env>` only (no `:cloud` variants). Every SDK API base URL is the same-origin root `/` (`browserOriginMode = same-origin`); the gateway serves the SPAs and the API on one origin.
- Release packaging (`scripts/webserver-release.mjs`, deb/rpm) produces standalone server packages only; `--deployment-profile cloud` is rejected.
- The nginx-compatible **module import plane** (`imports.d`, `SDKWORK_WEBSERVER_SPEC.md` §17.3) is a separate concept from the webserver's own build mode: it still materializes both `standalone` and `cloud` import sets for the imported sibling modules (default active set `cloud`) so the edge startup mode can switch freely. Sidecars are inputs to the Rust data plane, not a stock nginx process.
- Import uses **high-cohesion, low-coupling configuration**: the aggregator `imports.d/import.conf` includes each sibling module's own checkout sidecar directly (`/opt/deploy/sdkwork-space/<module>/deployments/webserver/nginx.<profile>.<environment>.conf`); the module's `deployments/webserver/` tree (sidecar + snippets) is the single source of truth and the webserver never copies or rewrites module configs under `/etc`. Switch the active set with `pnpm import:switch:cloud|standalone` (`SDKWORK_WEBSERVER_IMPORT_PROFILE` at container start, default `cloud`), then restart `serve-imports`.
- All other modules under `sdkwork-space` support both `standalone` (same-origin `/`) and `cloud` (unified `api-dev.<domain>` … `api.<domain>` edge) build modes per `ENVIRONMENT_SPEC.md` §5.1.0.1 and `PNPM_SCRIPT_SPEC.md` §4.2.

## Local Dictionary Structure

- `AGENTS.md`: repository agent entrypoint and relative SDKWork spec index.
- `CLAUDE.md`, `GEMINI.md`, `CODEX.md`: compatibility shims that point to `AGENTS.md`.
- `sdkwork.app.config.json`: Web Server application identity, runtime, release, and capability metadata.
- `etc/`: Web Server deployment/runtime profiles, application ingress configuration, upstream targets, and safe local examples.
- `sdkwork.workflow.json`: GitHub packaging/release workflow manifest.
- `.github/workflows/package.yml`: thin reusable workflow call only.
- `.sdkwork/`: repository/application AI workspace metadata.
- `specs/`: local application/component contracts.
- `apis/`: Web Server-owned API contract sources.
- `apps/`: browser application roots (`sdkwork-webserver-pc`, `sdkwork-webserver-h5`) served by the webserver process Adaptive Web console; the same webserver process owns public reverse proxy (no stock nginx).
- `crates/`: Rust service, repository, route, and API server crates.
- `sdks/`: SDK families and generated SDK artifacts.
- `database/`: database contract, baseline DDL, migrations, seeds, drift policy.
- `etc/`, `deployments/`, `scripts/`, `tools/`, `docs/`, `tests/`: source configuration, infrastructure descriptors, command entrypoints, validators, documentation, and verification assets.
- `package.json`, `Cargo.toml`: language/build manifests.

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Spec Resolution Order

<!-- SDKWORK-PROGRESSIVE-LOADING: v1 -->
Use dynamic progressive loading for the current task: resolve the selected root and task category before reading broad source context.

1. Read this `AGENTS.md` routing material and classify the owned surface.
2. Read `sdkwork.app.config.json`, module `specs/`, repository/application `specs/`, and `.sdkwork/` only when the task reaches the contract each item governs.
3. Locate only the relevant task-matrix row or navigation heading in `../sdkwork-specs/README.md`; do not load the full catalog.
4. Read only the task-specific global spec sections selected by that route, then inspect implementation files.
<!-- /SDKWORK-PROGRESSIVE-LOADING: v1 -->

Use dynamic progressive loading:

1. Read this `AGENTS.md` and any nearer component-level `AGENTS.md`.
2. Read `sdkwork.app.config.json` only when app behavior, runtime config, SDK wiring, release, packaging, or app-owned capabilities are touched.
3. Read local `specs/README.md` and `specs/component.spec.json` only when the task touches that local contract.
4. Read `../sdkwork-specs/README.md`, then only the task-specific root specs.
5. Inspect implementation files after the dictionary and relevant specs are clear.

## Required Specs By Task Type

- Agent/workflow changes: `../sdkwork-specs/SOUL.md`, `../sdkwork-specs/AGENTS_SPEC.md`, `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`, `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`, `../sdkwork-specs/TEST_SPEC.md`.
- Any code change: `../sdkwork-specs/CODE_STYLE_SPEC.md`, `../sdkwork-specs/NAMING_SPEC.md`, plus only the touched language/framework spec.
- Rust code: `../sdkwork-specs/RUST_CODE_SPEC.md`.
- API/SDK changes: `../sdkwork-specs/API_SPEC.md`, `../sdkwork-specs/WEB_FRAMEWORK_SPEC.md`, `../sdkwork-specs/WEB_BACKEND_SPEC.md`, `../sdkwork-specs/SDK_SPEC.md`, `../sdkwork-specs/TEST_SPEC.md`.
- Database changes: `../sdkwork-specs/DATABASE_SPEC.md`, `../sdkwork-specs/DATABASE_FRAMEWORK_SPEC.md`, `../sdkwork-specs/TEST_SPEC.md`.
- Runtime/deployment/release changes: `../sdkwork-specs/CONFIG_SPEC.md`, `../sdkwork-specs/ENVIRONMENT_SPEC.md`, `../sdkwork-specs/APPLICATION_DEPLOY_LAYOUT_SPEC.md`, `../sdkwork-specs/DEPLOYMENT_SPEC.md`, `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`.
- Security/auth changes: `../sdkwork-specs/IAM_SPEC.md`, `../sdkwork-specs/SECURITY_SPEC.md`.

## Int64 Wire Contract (API_SPEC §13.6)

- OpenAPI `int64` fields and parameters `MUST` be `type: string`, `format: int64`,
  a decimal `pattern` such as `^-?[0-9]+$`, and `x-sdkwork-int64-string: true`.
  `type: integer, format: int64` is a contract violation: generated TypeScript
  SDKs then emit `number`, and browsers silently round ids past
  `Number.MAX_SAFE_INTEGER` (2^53), replaying wrong ids into lookups.
- Rust response DTOs `MUST` serialize `i64` wire fields with
  `#[serde(with = "sdkwork_utils_rust::serde_int64")]` (or `::option`); request
  boundaries parse inbound strings with the same helper.
- Generated TypeScript SDKs keep `int64` as `string`; frontend code `MUST NOT`
  convert ids/snowflake ids/sequence ids to `number` for storage, comparison,
  or submission.
- Verification: `node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace .`

## Code Style Rules

Read `../sdkwork-specs/CODE_STYLE_SPEC.md` and `../sdkwork-specs/NAMING_SPEC.md` before code changes. Use `sdkwork-utils-rust` and `sdkwork-id-core` for shared helpers instead of duplicating utility logic locally. Generated SDK output must not be hand-edited.

Build scripts, dev runners, and `pnpm clean` must follow `CODE_STYLE_SPEC.md` §7 (Build Source Integrity And Self-Healing). Git-tracked build-critical source files must be verified before builds and self-healed from git when missing; `clean` must not delete them.

## Build, Test, and Verification

<!-- SDKWORK-VERIFICATION-ROUTING: v1 -->
Choose only the narrowest verification selected by the changed surface. This is not a default full-suite command list.
Run workspace-wide checks only when the change crosses that boundary.
`bootstrap-*`, `align-*`, `sync-*`, `--write`, and other mutating repair commands are not verification defaults; use them only for an explicitly scoped repair, migration, bootstrap, or alignment task and inspect the resulting diff.
<!-- /SDKWORK-VERIFICATION-ROUTING: v1 -->

```powershell
pnpm dev
pnpm check
pnpm verify
pnpm db:validate
pnpm topology:validate
```

## Agent Execution Rules

<!-- SDKWORK-PROGRESSIVE-LOADING: v1 -->
Use dynamic progressive loading for the current task; treat indexes and cross-references as discovery, not as a startup bundle.
Keep `../sdkwork-specs/SOUL.md` and the task-selected standards authoritative; expand context only when evidence exposes a new contract boundary.
Language-specific specs are on-demand: only the touched language loads `../sdkwork-specs/RUST_CODE_SPEC.md`, `../sdkwork-specs/JAVA_CODE_SPEC.md`, `../sdkwork-specs/TYPESCRIPT_CODE_SPEC.md`, or `../sdkwork-specs/FRONTEND_CODE_SPEC.md`.
Package command standardization loads `../sdkwork-specs/PNPM_SCRIPT_SPEC.md` only when the current task changes package commands or scripts; GitHub packaging work loads `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md` only when it reaches that workflow boundary.
Do not infer a recursive workspace scan or a broad validation suite from the presence of a path alone.
<!-- /SDKWORK-PROGRESSIVE-LOADING: v1 -->

Do not rely on memory when a relevant SDKWork spec exists. Do not replace generated SDK calls with raw HTTP. Stop when the relative specs path, app identity, component spec, API authority, SDK family, or table prefix is ambiguous. `sdkwork-discovery` is not required until RPC services are introduced.

## Task-Specific Standards

API work loads `../sdkwork-specs/API_SPEC.md` and its validators. List/search work loads `../sdkwork-specs/PAGINATION_SPEC.md` and `check-pagination.mjs`. Source configuration work loads `../sdkwork-specs/SOURCE_CONFIG_SPEC.md` and `check-source-config-standard.mjs`. Link these authorities instead of copying their normative bodies into `AGENTS.md`.

## Human Review Rules

Human review is required for breaking public API changes, schema migrations, privacy/security exceptions, generated SDK ownership changes, and destructive filesystem or data operations.
