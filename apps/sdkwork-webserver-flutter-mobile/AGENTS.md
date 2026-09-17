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
source configuration under `env/` and `config/`, not to the application
declaration.

- Application code: `webserver`
- Application key: `sdkwork-webserver-flutter-mobile`
- Package/bundle id: `com.sdkwork.webserver.mobile`
- Client architecture: `flutter-mobile` (runtime target `flutter-android`)

## Local Dictionary Structure

Use `AGENTS.md` as the application routing entrypoint. Read `.sdkwork/`, `specs/`,
application source, tests, and documentation only when the current task reaches
the contract each location governs.

- `lib/` is the installable Flutter entry/composition module: `main.dart`, the
  app shell, the route-guard widget, and `lib/bootstrap/` (environment
  resolution, SDK client construction, IAM runtime wiring, host adapter
  registration, route assembly).
- `packages/` owns Dart packages: `..._core`, `..._commons`, `..._shell`, and the
  `..._applications` capability.
- `env/`, `config/`, `etc/`, `pubspec.yaml`, and `.gitignore` are deployable-root
  configuration and build metadata.

## Spec Resolution Order

Use dynamic progressive loading: read this file and `../../AGENTS.md`, then
`../../../sdkwork-specs/FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md`, then applicable
local contracts under `specs/`, then the relevant task route in
`../../../sdkwork-specs/README.md`, and only afterward inspect implementation
files. Language-specific specs are on-demand only.

## Required Specs By Task Type

Flutter client work loads
`../../../sdkwork-specs/APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`,
`../../../sdkwork-specs/FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md`, and
`../../../sdkwork-specs/APP_FLUTTER_UI_SPEC.md`. Code changes load
`../../../sdkwork-specs/CODE_STYLE_SPEC.md`,
`../../../sdkwork-specs/NAMING_SPEC.md`, and only the touched frontend authority
such as `../../../sdkwork-specs/FRONTEND_CODE_SPEC.md`. Package-command work loads
`../../../sdkwork-specs/PNPM_SCRIPT_SPEC.md`; packaging workflow work loads
`../../../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`; deployment work loads
`../../../sdkwork-specs/DEPLOYMENT_SPEC.md`,
`../../../sdkwork-specs/ENVIRONMENT_SPEC.md`, and
`../../../sdkwork-specs/CONFIG_SPEC.md`.

## Code Style Rules

Consume remote capabilities through the ports composed in `core` and bound in
`lib/bootstrap/`. Do not introduce raw HTTP, manual authentication headers,
generated transport imports, local SDK forks, duplicated shared utilities, or a
second appbase IAM runtime. Capability packages must not construct SDK clients or
read runtime environment values directly.

Dart is **nominal**, not structural: a generated client can be handed straight
through in TypeScript because the slice is satisfied structurally, but Dart
requires a nominal `implements` that generated code will never declare. When a
Dart SDK for a dependency domain is generated, the adapter that bridges it
belongs in `core` — never in a capability package, and never as a
`dependency_overrides` fork.

## Build, Test, and Verification

Flutter builds require the Flutter SDK and Dart toolchain; neither is installed in
this workspace, so `pub get`, `analyze`, and `test` cannot run here and are
tracked as pending integration.

```powershell
flutter pub get
flutter analyze
flutter test
```

Static repository verification (runs without the Flutter toolchain):

```bash
node --test tests/flutter-surface-contract.test.mjs
node ../../../sdkwork-specs/tools/check-frontend-composition.mjs --root .
node ../../../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
```

## Agent Execution Rules

Follow specifications before memory and evidence before completion. Keep SDK
construction, authentication, environment selection, and host capabilities in
their owning composition layers. Stop when kernel ownership, API authority, or SDK
family boundaries are ambiguous.

A generated artifact that does not exist must never be simulated. Declare the
port, report the port unavailable, and let the screen render a distinct state —
do not resolve an empty list, which would read as "this tenant has no
applications".

## Task-Specific Standards

SDK consumer work loads `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`.
API work loads `../../../sdkwork-specs/API_SPEC.md` and its validators. List/search
work loads `../../../sdkwork-specs/PAGINATION_SPEC.md` and `check-pagination.mjs`.
Source configuration work loads `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md` and
`check-source-config-standard.mjs`. Locale resource work loads
`../../../sdkwork-specs/I18N_SPEC.md`.

Note that `check-app-sdk-consumer-imports.mjs` could not be pointed at this root
until a Flutter consumer-import rule exists; `tests/flutter-surface-contract.test.mjs`
§8 asserts the equivalent boundary over `.dart` sources in the meantime.

## Human Review Rules

Human review is required for public API changes, security exceptions, database
migrations, generated SDK ownership changes, destructive operations,
cross-application standards changes, and any change that adds or removes a
runtime profile.
