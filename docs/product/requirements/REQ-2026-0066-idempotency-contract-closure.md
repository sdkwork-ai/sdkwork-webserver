# REQ-2026-0066 Idempotency Contract Closure

```yaml
id: REQ-2026-0066
title: Enforce typed replay-safe idempotency from API authority through application consumers
owner: sdkwork-webserver
status: accepted
source: user
problem: Web Server routes required Idempotency-Key at runtime, while 28 matching OpenAPI operations exposed only x-sdkwork-idempotent metadata. Generated SDKs therefore could not accept the required Header and PC actions failed only after network dispatch with 40001.
goals:
  - Make x-sdkwork-idempotent, the required bounded Header, route metadata, generated SDK inputs, and consumers one verifiable contract.
  - Reject contract drift during authored API checks and materialization before SDK generation or runtime.
  - Reuse one unpredictable key across retries of a logical action and use a new key for a new action.
  - Keep runtime validation fail-closed and preserve replay/conflict safety through shared production stores.
  - Remove the deployment body key as an external input and inject validated framework identity into durable repository deduplication.
non_goals:
  - Making every mutation idempotent without an explicit reviewed marker.
  - Weakening authentication, request fingerprinting, conflict detection, or server-side Header enforcement.
  - Allowing UI packages to assemble Headers or replace generated SDK calls with raw HTTP.
acceptance_criteria:
  - All 28 marked app, backend, and internal operations declare the shared required Idempotency-Key Header with string length 1 through 128.
  - API materialization rejects marker/Header mismatches and does not infer idempotency from operationId names.
  - All generated SDK languages expose required idempotency inputs for marked operations; TypeScript consumers compile only when passing them.
  - Console and Admin actions pass the action-dialog key through SDK params and fail before dispatch if it is absent.
  - Deployment JSON schemas omit idempotencyKey while the validated framework context feeds durable repository deduplication.
  - Runtime accepts a 128-byte key, rejects a 129-byte key before store access, scopes persisted keys, replays matching requests, and conflicts on mismatched fingerprints.
  - Contract tests compare authored OpenAPI, materialized authorities, route manifests, TypeScript SDK output, and consumer boundaries.
affected_surfaces:
  - app-api
  - backend-api
  - internal-api
  - generated-sdks
  - pc-console
  - pc-admin
  - web-framework
  - standards
non_functional_requirements:
  security: Raw keys are bounded, scoped before persistence, absent from consumer Header assembly, and never replace authentication or request fingerprints.
  privacy: Keys must not contain credentials, PII, or business payload data.
  reliability: Retries after ambiguous outcomes reuse one key; production HA uses an atomic shared durable idempotency store.
  availability: Same-key replay avoids duplicate side effects across application instances and process restarts supported by the configured production store.
trace:
  specs:
    - API_SPEC.md
    - SDK_SPEC.md
    - SDK_WORKSPACE_GENERATION_SPEC.md
    - FRONTEND_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - apis/app-api/web/openapi.yaml
    - apis/backend-api/web/openapi.yaml
    - apis/internal-api/web/sdkwork-web-internal-api.openapi.yaml
    - tools/materialize_web_phase1_contracts.mjs
    - sdks/sdkwork-web-app-sdk
    - sdks/sdkwork-web-backend-sdk
    - sdks/sdkwork-web-internal-sdk
    - apps/sdkwork-webserver-pc
    - crates/sdkwork-api-webserver-standalone-gateway
    - crates/sdkwork-webserver-contract
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - ../sdkwork-web-framework/crates/sdkwork-web-core
verification:
  - node ../sdkwork-specs/tools/check-api-operation-patterns.mjs --root .
  - pnpm api:check
  - pnpm sdk:generate:check
  - pnpm test:contracts
  - pnpm --dir apps/sdkwork-webserver-pc typecheck
  - pnpm --dir apps/sdkwork-webserver-pc test
  - cargo test -p sdkwork-api-webserver-standalone-gateway generated_internal_sdk_preserves_runtime_assignment_wire_contract
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded -- --ignored --exact
  - cargo test -p sdkwork-web-core
```

## Result

The external retry identity is Header-owned and generated. Business services receive only the
framework-validated, scoped identity, while UI and SDK consumers never construct raw Headers.
