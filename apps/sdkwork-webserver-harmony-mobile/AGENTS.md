# Repository Guidelines

## SDKWORK Soul

Read `../../../sdkwork-specs/SOUL.md` before executing application tasks. Start
with the sections that route the current task; related-spec references are not a
startup bundle.

## SDKWORK Standards

The canonical standards index is `../../../sdkwork-specs/README.md`, and
`../../../sdkwork-specs/AGENTS_SPEC.md` governs this entrypoint. Read the relevant
task-matrix row first and do not copy global normative bodies locally.

## Application Identity

Read `sdkwork.app.config.json` only for application identity, SDK/API inventory,
release metadata, packaging, or app-owned capabilities. Runtime values belong to
source configuration under `etc/` and `config/`, not to the application
declaration.

- Application code: `webserver`
- Application key: `sdkwork-webserver-harmony-mobile`
- Bundle name: `com.sdkwork.webserver.mobile`
- Client architecture: `harmony-mobile` (runtime target `harmony-native`)

## Local Dictionary Structure

Use `AGENTS.md` as the application routing entrypoint. Read `.sdkwork/`, `specs/`,
application source, tests, and documentation only when the current task reaches
the contract each location governs.

- `entry/` is the installable HarmonyOS entry/composition module: the entry
  ability, bootstrap, provider assembly, route registry, SDK port construction,
  IAM runtime wiring, and host adapter registration.
- `packages/` owns ArkTS/HAR reusable core, commons, shell, host, and capability
  packages.
- `config/`, `etc/`, `AppScope/`, `oh-package.json5`, `build-profile.json5`, and
  `hvigor/` are deployable-root configuration and build metadata.

## Spec Resolution Order

Use dynamic progressive loading: read this file and `../../AGENTS.md`, then
`../../../sdkwork-specs/HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md`, then applicable
local contracts under `specs/`, then the relevant task route in
`../../../sdkwork-specs/README.md`, and only afterward inspect implementation
files. Language-specific specs are on-demand only.

## Required Specs By Task Type

HarmonyOS client work loads
`../../../sdkwork-specs/APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`,
`../../../sdkwork-specs/HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md`, and
`../../../sdkwork-specs/APP_HARMONY_NATIVE_UI_SPEC.md`. Code changes load
`../../../sdkwork-specs/CODE_STYLE_SPEC.md`,
`../../../sdkwork-specs/NAMING_SPEC.md`, and only the touched frontend authority
such as `../../../sdkwork-specs/FRONTEND_CODE_SPEC.md`. Package-command work loads
`../../../sdkwork-specs/PNPM_SCRIPT_SPEC.md`; packaging workflow work loads
`../../../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`; deployment work loads
`../../../sdkwork-specs/DEPLOYMENT_SPEC.md` and
`../../../sdkwork-specs/CONFIG_SPEC.md`.

## Code Style Rules

Consume remote capabilities through the injected app SDK ports composed in `core`
and constructed in `entry` bootstrap. Do not introduce raw HTTP, manual
authentication headers, generated transport imports, local SDK forks, duplicated
shared utilities, or a second appbase IAM runtime. Feature packages must not
construct SDK clients or read runtime environment values directly.

## Build, Test, and Verification

HarmonyOS builds require DevEco Studio or a compatible HarmonyOS SDK, `hvigor`,
and `ohpm`, plus a documented signing profile; these toolchains are not part of the
repository workspace and are tracked as pending integration.

```powershell
ohpm install
hvigor clean
hvigor assembleHap
```

Static repository verification (runs without the HarmonyOS toolchain):

```bash
node --test tests/harmony-surface-contract.test.mjs
node ../../../sdkwork-specs/tools/check-frontend-composition.mjs --root .
node ../../../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
```

## Agent Execution Rules

Follow specifications before memory and evidence before completion. Keep SDK
construction, authentication, environment selection, and host capabilities in
their owning composition layers. Stop when kernel ownership, API authority, or SDK
family boundaries are ambiguous.

## Task-Specific Standards

SDK consumer work loads `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md` and
runs `check-app-sdk-consumer-imports.mjs`. API work loads
`../../../sdkwork-specs/API_SPEC.md` and its validators. List/search work loads
`../../../sdkwork-specs/PAGINATION_SPEC.md` and `check-pagination.mjs`. Source
configuration work loads `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md` and
`check-source-config-standard.mjs`. Locale resource work loads
`../../../sdkwork-specs/I18N_SPEC.md`.

## Human Review Rules

Human review is required for public API changes, security exceptions, database
migrations, generated SDK ownership changes, destructive operations, and
cross-application standards changes.
