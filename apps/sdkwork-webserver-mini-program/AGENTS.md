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

Read `sdkwork.app.config.json` for mini program application identity, SDK inventory, release metadata, and app-owned capabilities. Read `../../sdkwork.app.config.json` for repository-wide Web Server identity. This is the WeChat mini program surface for Web Server: package taxonomy and root layout follow `../../../sdkwork-specs/MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md`. Concrete runtime values come from this application's `config/mini-program/` profiles and the runtime bundle materialized by `scripts/build-runtime.mjs`; neither app manifest is runtime configuration authority.

## Local Dictionary Structure

- `AGENTS.md`: application agent entrypoint and relative SDKWork spec index.
- `sdkwork.app.config.json`: mini program application identity and capability metadata.
- `specs/`: application composition contract.
- `src/`: thin native mini program root — `app.js`/`app.json`/`app.wxss`, `bootstrap/`, and platform pages that only bind the runtime bundle.
- `packages/`: the `sdkwork-webserver-mp-*` package family (`core`, `commons`, `shell`, `host`, `applications`) that owns all business code.
- `config/mini-program/`: committed runtime profiles, one per `<deployment-profile>.<environment>`.
- `config/host/`: non-secret WeChat platform templates.
- `etc/`: deployable-root source configuration (parent delegation only).
- `scripts/`: deterministic runtime profile selection, route projection, and bundling.
- `tests/`: configuration, route projection, host boundary, and runtime contract tests.
- `package.json`, `project.config.json`, `tsconfig.json`: build, platform, and language manifests.

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
- Mini program: `../../../sdkwork-specs/MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md`, `../../../sdkwork-specs/APP_MINI_PROGRAM_UI_SPEC.md`, and `../../../sdkwork-specs/APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`.
- SDK integration: `../../../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`, `../../../sdkwork-specs/SDK_SPEC.md`, and `../../../sdkwork-specs/APP_PERMISSION_COMPOSITION_SPEC.md`.
- Route or page changes: `../../../sdkwork-specs/MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §5 and §9, plus `PAGINATION_SPEC.md` for list screens.
- Package command changes: `../../../sdkwork-specs/PNPM_SCRIPT_SPEC.md`.
- Packaging workflow changes: `../../../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`.
- Security/auth changes: `../../../sdkwork-specs/IAM_SPEC.md` and `../../../sdkwork-specs/SECURITY_SPEC.md`.

## Code Style Rules

Build scripts and `pnpm clean` must follow `../../../sdkwork-specs/CODE_STYLE_SPEC.md` §7. Consume Web remote capabilities through the generated SDK facades declared by component specs; generated SDK output must not be hand-edited. Business code belongs in `packages/`, not in `src/pages/`: a platform page binds the runtime bundle and `setData`, and owns no state machine of its own.

## Build, Test, and Verification

Choose the narrowest check for the changed surface, then broaden only when the change crosses an application boundary:

```text
pnpm typecheck
pnpm test
pnpm build:mini-program
pnpm check
```

Mutating `bootstrap-*`, `align-*`, `sync-*`, and `--write` commands are not verification defaults. Use them only for an explicitly scoped repair or migration and inspect the resulting diff.

## Agent Execution Rules

This application is a native WeChat mini program (`mp-weixin`). Runtime values are selected from `config/mini-program/` at build time and frozen into `src/runtime/runtime-env.js`. Do not introduce a platform secret, a hardcoded origin, a raw HTTP transport, a hand-maintained `app.json` page list, or a direct `wx.*` call outside `packages/sdkwork-webserver-mp-host`.

## Task-Specific Standards

List/search work loads `../../../sdkwork-specs/PAGINATION_SPEC.md` and runs `check-pagination.mjs`. Source configuration work loads `../../../sdkwork-specs/SOURCE_CONFIG_SPEC.md` and runs `check-source-config-standard.mjs`; `etc/` is this deployable root's source configuration boundary, and `config/` holds the committed runtime profiles. Host platform metadata must stay secret-free.

## HTTP API Response Envelope

All L2+ SDKWork-owned custom HTTP contracts, including `app-api`, `backend-api`, and SDKWork-owned business `open-api`, `MUST` follow `API_SPEC.md` section 4.5, section 14, and section 15:

- **Default classification:** omitted `x-sdkwork-wire-protocol` means SDKWork-owned custom API (`sdkwork-v3`); only operation-level `x-sdkwork-wire-protocol: external` plus `x-sdkwork-external-protocol-id` identifies a third-party compatibility `open-api` operation.
- **Input:** typed request bodies, section 14.1 list/search/command input, `SdkWorkListQuery`, and `q` for free-text search.
- **Success output:** `SdkWorkApiResponse` with `{ "code": 0, "data": <payload>, "traceId": "<server-uuid>" }`.
- **Error output:** HTTP 4xx/5xx `application/problem+json` (`ProblemDetail`) with numeric `code` and `traceId`; SDKWork-owned errors may include `i18nKey` and `locale` presentation metadata.
- Success `code` is numeric `int32`; HTTP 2xx JSON bodies `MUST` use `0` only. REST semantics remain on HTTP status (`201`, `202`, etc.).
- Platform error codes are numeric non-zero values per section 15.3 (`40001`, `40101`, `40401`, …).
- Single resource: `data.item`
- Lists: `data.items` + `data.pageInfo` (`PageInfo.mode` is `offset` or `cursor`)
- Commands: `data.accepted` plus optional `resourceId` / `status`
- Async accept (`202`): `data.operationId`, `data.status`, optional `pollUrl`
- Operation patterns: retrieve/list/search/create/update/delete/command/async/bulk semantics follow `API_SPEC.md` section 15.4; create uses `201`, delete uses `204` with no JSON body, and `PUT`/`PATCH` use SDK action `update`.

Vendor compatibility `open-api` routes that mirror upstream tool or provider wire (for example OpenAI `/v1/*`, Anthropic/Claude `/anthropic/v1/*`, Google/Gemini `/google/v1beta/*`, Claude Code, or Codex) `MAY` opt out only when every exempt operation declares operation-level `x-sdkwork-wire-protocol: external` and `x-sdkwork-external-protocol-id` per `API_SPEC.md` section 4.5.2. SDKWork-owned business `open-api` operations `MUST NOT` opt out. Mixed OpenAPI documents are validated per operation; one external operation never exempts SDKWork-owned operations in the same document.

Errors `MUST` use HTTP 4xx/5xx with `application/problem+json` (`ProblemDetail`) including required numeric `code` and `traceId`. Optional `i18nKey` and `locale` are display metadata only. Business failures `MUST NOT` use HTTP 2xx with non-zero `code`, string wire codes, `success`, or human `message`.

Forbidden legacy envelopes and fields: `PlusApiResult`, `AppbaseApiResult`, `StoreApiResult`, `SdkWorkResponse`, per-domain `*ApiResult`, wire field `requestId`, bare domain DTOs at the HTTP root, and top-level `{ items, pageInfo, traceId }` without `data`.

Handlers `MUST` serialize success and map errors through `sdkwork-web-framework` response mapping. Generated HTTP SDKs (`--standard-profile sdkwork-v3`) unwrap `data` by default and expose typed numeric `ProblemDetail.code` / `traceId` and returned localization metadata on errors; use `.raw` when the full envelope is required.

Before completing API contract, SDK generation, or frontend service work, run:

```bash
node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace <workspace-root>
node <sdkwork-specs>/tools/check-api-response-envelope.mjs --workspace <workspace-root>
```

Authority: `sdkwork-specs/API_SPEC.md` section 4.5 and sections 14–16, `SDK_SPEC.md` section 4.2, `FRONTEND_SPEC.md`, `MIGRATION_SPEC.md` section 4.2.

## Human Review Rules

Human review is required for breaking public API changes, security exceptions, generated SDK ownership changes, destructive operations, permission catalog changes, platform app-id changes, and public-origin changes.
