# ADR-20260728 Embedded Standalone Dependency Assemblies

Status: accepted
Requirement: REQ-2026-0064
Owner: sdkwork-webserver
Date: 2026-07-28
Specs: API_ASSEMBLY_SPEC.md, APPLICATION_GATEWAY_SPEC.md, APP_RUNTIME_TOPOLOGY_SPEC.md, CONFIG_SPEC.md, ENVIRONMENT_SPEC.md, IAM_SPEC.md, TEST_SPEC.md

## Context

The Web Server PC consumed the IAM and Drive App SDKs. Its standalone runtime config pointed those clients to `127.0.0.1:3900`, while the application public ingress was `127.0.0.1:3800`. The standalone gateway bootstrapped IAM persistence but did not mount IAM routes. As a result, successful startup of the Web Server did not make `/app/v3/api/auth/*` available unless an unrelated dependency gateway also happened to be running. This contradicted API assembly composition and made `standalone` depend on an undeclared second API process.

## Decision

- The Web Server standalone gateway is the only browser-visible standalone HTTP ingress. It binds the configured `application.public-ingress`, currently port 3800 in development.
- The gateway calls host-neutral App API contribution exports from `sdkwork-api-iam-assembly` and `sdkwork-api-drive-assembly` in the same Rust process. It also consumes the Web Server owner assembly contribution.
- Owner contributions contain raw Axum routers, route manifests, OpenAPI documents, permission catalogs, domain context injectors, and readiness checks. They do not install their own process-wide Web Framework or infrastructure listener.
- The host gateway validates each owner contribution, rejects normalized method/path collisions, merges all raw routers, and installs one Web Framework, IAM authorization policy, request context resolver, metrics registry, OpenAPI endpoint, readiness composition, and infrastructure router.
- Standalone dependency SDK targets use `application.public-ingress`. Browser-visible URL resolution is governed separately by ADR-20260728 Standalone Browser Same-Origin Delivery: development uses the Vite page origin with a private canonical-path proxy, while production uses the application ingress directly. `platform.api-gateway` URLs remain valid only for cloud profiles where the platform plane is external.
- The component contract records IAM and Drive contribution exports as required ports with `runtimeMode: "same-origin"` and standalone profile coverage.
- Standalone release packages include IAM `database/` and `iam/registry`/module assets under `share/sdkwork/iam`, Drive database assets under `share/sdkwork/drive`, and bind both owner roots explicitly. Installed startup must not use sibling repository paths or compile-time source paths.
- The Web Server lock file pins the newly introduced Drive/AWS dependency graph to the versions already verified by the Drive owner repository and compatible with Rust 1.92.
- IAM reuses the canonical installed `sdkwork_database_sqlx::DatabasePool`. Drive's PostgreSQL App API repositories still expose `sqlx::AnyPool`, so the standalone profile declares one temporary, identity-checked compatibility driver exception owned by `sdkwork-drive maintainers`.
- `SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION=true` and `SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT=1` must be present before the canonical pool is created. The configured `SDKWORK_DATABASE_MAX_CONNECTIONS` value is the combined process budget; the current development budget of 10 is split by the database framework into 5 canonical and 5 compatibility connections. The exception cannot enlarge the process budget and is not single-driver pool compliance.

## Alternatives

- Starting `sdkwork-iam` and `sdkwork-drive` standalone gateways as child processes was rejected because it adds independent lifecycle, ports, readiness, and failure modes to the standalone application unit.
- Keeping a Vite/browser proxy to port 3900 was rejected because it hides rather than removes the second runtime dependency.
- Reimplementing IAM or Drive routes in Web Server was rejected because API authority, persistence, permissions, and generated SDK ownership remain with the dependency repositories.
- Mounting only IAM was rejected because the same PC runtime also consumes the Drive App SDK; pointing Drive at port 3800 without mounting its owner routes would replace connection refused with route 404.

## Consequences

The standalone executable links more owner code and its transitive dependencies, but it has one HTTP lifecycle and one same-origin browser security boundary. IAM and Drive database/config prerequisites and owner runtime assets now fail package validation or Web Server gateway startup rather than failing later as browser connection errors. Until Drive completes its driver migration, the process owns one canonical pool and one governed compatibility pool against the same normalized PostgreSQL identity. Cloud profiles remain externally routed. Internal Web Server data-plane or operations listeners remain separate non-browser responsibilities and are not changed by this decision.

## Database Exception Removal Criteria

1. Drive PostgreSQL App API repositories accept the installed `DatabasePool::Postgres` / `sqlx::PgPool` handle.
2. The embedded Web Server path no longer constructs or resolves `sqlx::AnyPool`.
3. The temporary exception and both `SDKWORK_DATABASE_TEMPORARY_*` profile keys are removed from Web Server contracts and standalone source profiles.
4. Live startup evidence proves one process pool and clean shutdown releases its connections before the next Web Server production release.

## Verification

- Compile the Web Server assembly and standalone gateway with Rust 1.92.
- Validate topology source profiles and component required ports.
- Run PC runtime-config tests and type checking.
- Run API assembly, route collision, permission composition, and standalone topology contract checks.
- Run the process-shared database-pool validator and verify the temporary exception metadata, owner, combined budget, and removal milestone.
- Confirm checked-in standalone browser runtime config contains no port 3900 dependency URL and, under ADR-20260728 Standalone Browser Same-Origin Delivery, no direct backend listener origin.
- Start the real standalone profile and confirm IAM and Drive OpenAPI paths are served by port 3800 without requiring the independent port 3900 process.
- Validate the standalone archive and run extracted-artifact smoke with packaged IAM/Drive roots and no source-workspace fallback.
