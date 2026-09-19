# REQ-2026-0004 PostgreSQL Authoritative Database Lifecycle

```yaml
id: REQ-2026-0004
title: Prove PostgreSQL lifecycle and repository behavior for every Web Server profile
owner: SDKWork maintainers
status: in-progress
source: reliability
problem: The authoritative Web control plane requires executable evidence that a fresh PostgreSQL database initializes, remains drift-clean, enforces tenant-safe relationships, and supports bounded repository behavior.
goals:
  - Keep PostgreSQL as the only authoritative server database for standalone and cloud profiles.
  - Execute baseline, lifecycle, seed history, drift, repository, recovery, and failover verification against disposable PostgreSQL.
  - Require explicit snowflake BIGINT ids and numeric tenant/organization/user scope.
  - Prove transaction rollback, uniqueness, idempotency, tenant isolation, and store pagination.
non_goals:
  - Adding a database dependency to the HTTP/HTTPS request path.
  - Reintroducing an alternative authoritative server database or rollback target.
  - Claiming managed-provider, PITR, multi-region, or production RPO/RTO completion.
users:
  - platform operators
  - site reliability engineers
  - backend maintainers
acceptance_criteria:
  - PostgreSQL fresh initialization, repeated lifecycle execution, and drift analysis pass against a disposable database.
  - The baseline uses explicitly supplied BIGINT business and subject ids and produces no error-level drift.
  - Foreign keys, unique constraints, partial indexes, and check expressions match the database contract.
  - Repository integration tests cover rollback, unique/idempotency conflict, tenant filtering, domain filtering, and store-level pagination.
  - Root-domain, hostname, certificate identifier/version, listener binding, and deployment relationships pass real PostgreSQL tests.
  - CI and release validation fail when PostgreSQL lifecycle or repository parity does not execute.
non_functional_requirements:
  security: Disposable test tooling rejects non-empty or non-test targets and never accepts production credentials.
  privacy: Verification uses synthetic tenant data only.
  performance: P0/P1 queries use bounded SQL pagination and reviewed PostgreSQL indexes/query plans.
  reliability: Initialization and drift checks are deterministic and backup/restore and failover are separate mandatory gates.
affected_surfaces:
  - database
  - backend
  - composition
trace:
  specs:
    - SUBJECT_ID_SPEC.md
    - DATABASE_SPEC.md
    - DATABASE_FRAMEWORK_SPEC.md
    - PAGINATION_SPEC.md
    - TEST_SPEC.md
  components:
    - database/database.manifest.json
    - database/contract/schema.yaml
    - database/ddl/baseline/postgres/0001_webserver_baseline.sql
    - crates/sdkwork-webserver-database-host
    - crates/sdkwork-intelligence-webserver-repository-sqlx
verification:
  - pnpm db:validate
  - pnpm test:postgres:required
  - pnpm test:database:recovery
  - pnpm test:postgres:ha
```

## Evidence Boundary

The disposable PostgreSQL lifecycle, repository parity, backup/restore, and bounded physical
promotion drills are engineering evidence. Managed failover, client endpoint switching,
split-brain fencing, PITR, production scheduling, encrypted retention, and measured RPO/RTO remain
commercial release gates.

## Change Control

- 2026-07-31: Replaced the prelaunch dual-engine requirement with PostgreSQL-only authoritative
  lifecycle and repository evidence. Removed alternative-engine lifecycle, release, recovery, and
  rollback claims.
