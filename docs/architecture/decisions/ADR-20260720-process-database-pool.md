# ADR-20260720 Process Database Pool Ownership

Status: accepted
Owner: sdkwork-webserver
Date: 2026-07-20
Updated: 2026-07-31
Requirement: REQ-2026-0004
Specs: DATABASE_SPEC.md, DATABASE_FRAMEWORK_SPEC.md, RUST_CODE_SPEC.md

## Context

Lifecycle, readiness, repositories, and shutdown must share one bounded authoritative database
pool. Secondary pools and driver-erased bridges make connection budgets and transaction behavior
unobservable.

## Decision

Every Web Server process enables the `sdkwork-database` process-shared pool guard before database
bootstrap. `sdkwork-webserver-database-host` creates and owns one PostgreSQL `DatabasePool` and
performs lifecycle initialization against it. `PostgresWebRepository` consumes the exact installed
`sqlx::PgPool` through `WebRepositoryPort`.

No route, service, worker, or repository constructs a secondary low-level pool. Alternative
engines and `sqlx::AnyPool` are not Web authoritative-server profiles.

## Consequences

- Lifecycle, readiness, repository work, and graceful shutdown share one connection budget.
- PostgreSQL types, constraints, isolation, and query plans are tested directly.
- Runtime engine branching and dual-engine expression helpers are removed from the Web product.

## Verification

```powershell
node ../sdkwork-specs/tools/check-process-shared-database-pool.mjs --root .
pnpm db:validate
pnpm test:postgres:required
```

## Supersedes / Superseded By

This record replaces its prelaunch dual-driver text while retaining the stable ADR identifier.
