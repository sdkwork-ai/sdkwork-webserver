# REQ-2026-0050 Bounded PostgreSQL Recovery Drill

```yaml
id: REQ-2026-0050
title: Prove bounded real PostgreSQL backup restoration
owner: sdkwork-webserver
status: accepted
source: database-commercial-readiness
problem: Database lifecycle and repository tests do not prove that an independent PostgreSQL database can restore a consistent schema and tenant-scoped business row from a real backup artifact.
goals:
  - Exercise PostgreSQL custom-format pg_dump and pg_restore against a disposable PostgreSQL 16.9 instance.
  - Prove the restored database is isolated from source writes after the backup boundary.
  - Bound operation time, readiness, subprocess output, baseline bytes, artifact bytes, process concurrency, and disposable resources.
  - Keep the drill mandatory in merge and release validation.
non_goals:
  - Supporting PostgreSQL recovery or another non-PostgreSQL authoritative server profile.
  - Implementing a production scheduler, retention service, object-store upload, or KMS policy.
  - Claiming WAL/PITR, managed-provider, multi-region, or product RPO/RTO completion.
acceptance_criteria:
  - The runner uses a digest-pinned PostgreSQL image, one unique disposable container, no persistent volume, no container network, and test-only credentials.
  - It applies the tracked baseline, inserts a tenant canary, creates a no-owner/no-ACL custom dump, records SHA-256, mutates the source, restores an empty database, and observes only pre-boundary state.
  - Total duration, readiness, output, baseline, artifact, CPU, memory, PID, and tmpfs resources are finite.
  - Cleanup runs in a finally path and no recovery container remains after success or failure.
  - Workflow and contract checks pin the real commands, bounds, and mandatory placement.
non_functional_requirements:
  security: Production URLs and credentials are rejected; the disposable port and artifacts are not externally exposed.
  reliability: Restore verification checks checksum, independent database state, schema completeness, tenant scope, subprocess failure, and cleanup.
  performance: Work is sequential and every process, buffer, file, and container resource is bounded.
affected_surfaces:
  - database-recovery-evidence
  - postgresql-release-verification
  - release-workflow-contract
trace:
  specs:
    - DATABASE_SPEC.md
    - DATABASE_FRAMEWORK_SPEC.md
    - TEST_SPEC.md
    - SECURITY_SPEC.md
    - PNPM_SCRIPT_SPEC.md
  components:
    - scripts/database-recovery-verify.mjs
    - sdkwork.workflow.json
verification:
  - node --test tests/contract/database-recovery.contract.test.mjs
  - pnpm test:database:recovery
  - node ../sdkwork-github-workflow/scripts/sdkwork-workflow.mjs validate --config sdkwork.workflow.json
  - git diff --check
```

## Evidence Boundary

The bounded drill proves real `pg_dump`/`pg_restore`, checksum, independent restore, tenant canary,
and cleanup behavior in disposable infrastructure. Production scheduling, encryption, immutable
off-host retention, WAL/PITR continuity, managed-provider recovery, cross-region recovery, and
declared RPO/RTO remain open.

## Change Control

- 2026-07-31: Removed the obsolete PostgreSQL recovery path after the PostgreSQL-only authority
  decision and aligned the requirement to the current runner.
