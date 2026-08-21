# REQ-2026-0063 Tenant Super Administrator Control Plane

```yaml
id: REQ-2026-0063
title: Operate the complete tenant Web Server control plane with explicit super-administrator boundaries
owner: sdkwork-webserver
status: in-progress
source: user
problem: The backend-admin shell can list tenant resources, but application lifecycle, domain removal, rollback, audit filtering, accurate Nginx and server registration inputs, and one-time Node credential handling are incomplete. Partial operators and super administrators are not visibly distinguished, and accepted deployment commands can be mistaken for completed traffic publication.
goals:
  - Route Web Server administrators to the isolated /admin surface while preserving the complete /console shell and sign-out command for every authenticated user.
  - Give a tenant Web super administrator complete tenant-scoped application lifecycle control: create, retrieve, update, activate, pause, delete, bind/verify/unbind domains, create artifact-backed deployment commands, and create rollback commands from successful deployments.
  - Keep certificate lifecycle, distribution convergence, Nginx validation/deployment/reload, Web Node inventory, diagnostics, and audit evidence in the same dense operational workspace.
  - Distinguish partial tenant administrators, Web super administrators, and platform super administrators without exposing raw permission arrays or credentials.
  - Require explicit confirmation for application pause/delete, domain unbind, deployment/rollback, certificate renewal, Nginx deployment/reload, and other runtime-affecting operations.
  - Filter audit evidence by target type, action, operator, and date range through the generated Backend SDK.
  - Display a newly registered Web Node credential exactly once and require the operator to transfer it to an approved secret manager.
non_goals:
  - Cross-tenant administration through the tenant-bound Backend API. Cross-tenant operations require a separately authenticated platform-admin control plane.
  - Treating deployment or rollback command acceptance as evidence that traffic changed. A deployment execution worker remains the authority for state progression.
  - Adding raw HTTP, manual authorization headers, local DTO forks, or handwritten generated SDK output.
  - Claiming server disable, deletion, or credential rotation before canonical Backend APIs and reviewed lifecycle semantics exist.
  - Introducing a database migration.
users:
  - tenant Web super administrators
  - tenant application, certificate, Nginx, server, and audit operators
  - platform super administrators entering the tenant-bound Web Server module
acceptance_criteria:
  - Backend OpenAPI, materialized route manifests, generated Backend SDKs, Rust routes, and the service port expose application retrieve/update/delete/activate/pause, domain delete, and deployment rollback operations.
  - Every Backend application operation converts the authenticated Backend context to tenant resource scope; app-user Console operations remain owner scoped.
  - The Admin application workspace calls only generated Backend SDK namespaces and gates actions by selected resource state and IAM write permission.
  - Application update and Nginx update forms prefill only declared request fields from the selected row.
  - An active application can be paused but cannot be activated or deleted from the UI; a non-active application can be activated or deleted.
  - Domain verification is unavailable after verification; domain unbind always requires explicit confirmation.
  - Rollback is available only for a successful deployment and creates a new asynchronous rollback command.
  - Admin deployment creation requires a stable Drive URI, positive size, SHA-256 digest, and explicit environment; the UI labels the result as a command rather than a live publication.
  - The repository rejects deletion of an active application and rollback of any deployment that is not successful; conditional state updates prevent concurrent activation or deployment-state changes from bypassing these rules.
  - Audit list filters map to targetType, action, operatorId, startDate, and endDate on the generated Backend SDK.
  - Nginx and server forms use the canonical generated request fields. Server registration shows the returned agentToken only in a one-time result state.
  - Partial operators see only authorized Admin modules and actions. web.* is classified as a Web super administrator, while * is additionally classified as a platform super administrator.
  - Sensitive state-changing service operations continue to emit tenant audit evidence.
non_functional_requirements:
  security: Authorization is enforced by Backend route permission and tenant-scoped service/repository access. Frontend action visibility is defense-in-depth only. One-time Node credentials are never persisted by the PC client.
  privacy: Tenant administrators can inspect all tenant Web resources but cannot cross the authenticated tenant boundary.
  reliability: Runtime-affecting commands are idempotent where declared, require confirmation, preserve asynchronous status truth, and remain auditable.
  usability: The Admin workspace stays dense, scan-oriented, keyboard accessible, responsive, and explicit about administrator tier and dangerous actions.
affected_surfaces:
  - api
  - sdk
  - backend
  - pc
  - iam
  - security
  - deployment
trace:
  specs:
    - API_SPEC.md
    - SDK_SPEC.md
    - BACKEND_UI_SPEC.md
    - WEB_BACKEND_SPEC.md
    - PAGINATION_SPEC.md
    - IAM_SPEC.md
    - SECURITY_SPEC.md
    - DEPLOYMENT_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-contract
    - crates/sdkwork-routes-webserver-backend-api
    - crates/sdkwork-intelligence-webserver-service
    - sdks/sdkwork-web-backend-sdk
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-core
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-applications
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-commons
verification:
  - cargo test -p sdkwork-intelligence-webserver-service
  - cargo test -p sdkwork-routes-webserver-backend-api
  - pnpm sdk:generate:check
  - pnpm --dir apps/sdkwork-webserver-pc check
  - pnpm --dir apps/sdkwork-webserver-pc exec vitest run tests/architecture-boundary.test.ts
  - node ../sdkwork-specs/tools/check-application-layering.mjs --root .
  - node ../sdkwork-specs/tools/check-permission-composition.mjs --workspace .
  - node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
```

`tests/architecture-boundary.test.ts` is the executable application-owned check that prevents
Backend SDK imports from crossing into Console packages. The workspace standards tools do not
currently provide a `check-backend-sdk-consumer-imports.mjs` command, so this requirement records
only commands that exist and can produce repeatable evidence.

## Decision

The current `/admin` surface is a tenant control plane. `web.*` grants the complete human-operated Web Server tenant module, while `*` identifies a broader platform super administrator but does not alter the tenant bound carried by the Backend request context. A future cross-tenant platform control plane must use a separate route family and authorization context.

Deployment and rollback APIs create auditable asynchronous records. Until a deployment execution worker advances those records and reports runtime evidence, Admin must show pending command state and must not claim that a version is serving traffic.

Server registration and inventory are complete only for the currently canonical Backend contract. Disable, delete, and credential rotation remain explicit release gaps rather than inferred UI actions.
