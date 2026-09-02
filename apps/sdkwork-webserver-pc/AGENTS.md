# Repository Guidelines

<!-- SDKWORK-AGENTS-GENERATED: v2 -->

## SDKWORK Soul

Read `../../../sdkwork-specs/SOUL.md` before executing tasks in this application root. Follow specs before memory, dictionary before context, stop on ambiguity, and evidence before completion.

## SDKWORK Standards

Resolve this standards root once for the current task:

- `../../../sdkwork-specs/README.md`
- `../../../sdkwork-specs/SOUL.md`
- `../../../sdkwork-specs/AGENTS_SPEC.md`

Read only the relevant README task-matrix row or navigation heading, then load the selected authority sections. Do not copy global standard bodies into this application. If these relative paths do not resolve, stop and report the broken workspace layout.

## Application Identity

Read `sdkwork.app.config.json` for PC application identity, SDK inventory, release metadata, packaging, and app-owned capabilities. Read `../../sdkwork.app.config.json` for repository-wide Web Server identity. Concrete browser runtime values come from this application's `etc/` profiles and materialized runtime-env output; neither app manifest is runtime configuration authority.

## Local Dictionary Structure

- `AGENTS.md`: application agent entrypoint and relative SDKWork spec index.
- `sdkwork.app.config.json`: PC application identity and capability metadata.
- `specs/`: application composition contract.
- `packages/`: PC core, commons, console, admin, shell, and capability packages; each authored package owns `specs/component.spec.json`.
- `src/`: thin browser bootstrap and app/console/admin surface composition.
- `etc/`: deployable-root source configuration for browser profiles.
- `public/`: materialized public runtime configuration and static browser assets.
- `scripts/`: deterministic application build and configuration tools.
- `tests/`: application architecture and interaction contract tests.
- `package.json`, `vite.config.ts`, `tsconfig.json`: build and language manifests.

## Spec Resolution Order

Use dynamic progressive loading:

1. Read this file and `../../AGENTS.md`.
2. Read application identity and the nearest `specs/component.spec.json` only when the task touches those contracts.
3. Locate the relevant row in `../../../sdkwork-specs/README.md`.
4. Read only the task-selected global specs.
5. Inspect implementation files after the dictionary and relevant specs are clear.

Language-specific standards are loaded on demand only; do not load unrelated language, runtime, UI, deployment, or SDK specs as a startup bundle.

## Required Specs By Task Type

- Any code change: `../../../sdkwork-specs/CODE_STYLE_SPEC.md`, `../../../sdkwork-specs/NAMING_SPEC.md`, and only the touched language/framework spec.
- TypeScript/Node: `../../../sdkwork-specs/TYPESCRIPT_CODE_SPEC.md`.
- React/frontend: `../../../sdkwork-specs/FRONTEND_CODE_SPEC.md`, `../../../sdkwork-specs/FRONTEND_SPEC.md`, `../../../sdkwork-specs/APP_PC_ARCHITECTURE_SPEC.md`, and `../../../sdkwork-specs/APP_PC_REACT_UI_SPEC.md`.
- SDK integration: `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`, `../../../sdkwork-specs/SDK_SPEC.md`, and `../../../sdkwork-specs/APP_PERMISSION_COMPOSITION_SPEC.md`.
- Package command changes: `../../../sdkwork-specs/PNPM_SCRIPT_SPEC.md`.
- Packaging workflow changes: `../../../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`.
- Security/auth changes: `../../../sdkwork-specs/IAM_SPEC.md` and `../../../sdkwork-specs/SECURITY_SPEC.md`.

## Code Style Rules

Consume Web remote capabilities through the composed `@sdkwork/webserver-app-sdk` and backend-admin-only `@sdkwork/webserver-backend-sdk` facades declared by component specs. Bootstrap owns SDK client and TokenManager construction. Feature UI receives injected data sources or services and must not create raw HTTP transports, manual auth headers, local SDK forks, or cross-surface business imports. Generated SDK output must not be hand-edited.

## Build, Test, and Verification

Choose the narrowest check for the changed surface, then broaden only when the change crosses an application boundary:

```text
pnpm typecheck
pnpm test
pnpm build
pnpm check
```

Mutating `bootstrap-*`, `align-*`, `sync-*`, and `--write` commands are not verification defaults. Use them only for an explicitly scoped repair or migration and inspect the resulting diff.

## Agent Execution Rules

This application is a Vite browser renderer with separate app-console and backend-admin surfaces. Console packages may consume only app-api SDK composition through `pc-console-core`; admin packages may consume backend-api SDK composition only through `pc-admin-core`. Runtime values are sourced from `etc/` and materialized public configuration. Do not introduce a browser-side provider secret, Node proxy, fake-success fallback, backend SDK access in the app/console surface, or dependency SDK transport deep import.

## Task-Specific Standards

API response envelope and operation-pattern work loads `../../../sdkwork-specs/API_SPEC.md` sections 4.5 and 14-16 and runs `check-api-response-envelope.mjs` plus `check-api-operation-patterns.mjs`. SDK consumer import work loads `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md` section 9 and runs `check-app-sdk-consumer-imports.mjs`. List/search work loads `../../../sdkwork-specs/PAGINATION_SPEC.md` and runs `check-pagination.mjs`. Source configuration work loads `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md` and runs `check-source-config-standard.mjs`; `etc/` is this deployable root's source configuration boundary.

## Human Review Rules

Human review is required for breaking public API changes, security exceptions, database migrations, generated SDK ownership changes, destructive operations, permission catalog changes, and app/console/admin SDK boundary changes.
