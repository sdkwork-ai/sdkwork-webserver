# REQ-2026-0049 Mandatory PostgreSQL CI Gate

```yaml
id: REQ-2026-0049
title: Make PostgreSQL lifecycle and Repository parity mandatory in merge and release verification
owner: sdkwork-webserver
status: accepted
source: database-commercial-readiness
problem: The default workspace verify command intentionally ignores tests that require an explicitly disposable PostgreSQL database. PostgreSQL behavior has local evidence, but a change can merge or package while PostgreSQL lifecycle, drift, transaction, tenant-isolation, idempotency, or pagination parity is broken.
goals:
  - Run PostgreSQL lifecycle/seed/drift and full Repository parity against a fresh disposable database in pull requests, main-branch pushes, and release package validation.
  - Keep the application workflow aligned with sdkwork-github-workflow ownership for dependency planning, checkout, toolchains, and package lifecycle execution.
  - Pin the PostgreSQL image by version and manifest digest.
  - Bound readiness, child-process duration, captured diagnostics, container count, database connections, and cleanup behavior.
  - Prove each PostgreSQL suite starts from an empty schema and cannot silently skip.
non_goals:
  - Replacing developer-owned PostgreSQL profiles or changing production database configuration.
  - Adding a schema migration or changing PostgreSQL application semantics.
  - PostgreSQL HA/failover, backup/restore, or multi-region evidence.
users:
  - maintainers reviewing merge readiness
  - release operators packaging the Web Server
acceptance_criteria:
  - The root exposes test:postgres:required and it invokes a deterministic Node runner rather than an optional ignored cargo command.
  - The runner uses postgres 16.9 alpine pinned by sha256 manifest digest, one uniquely named --rm container, loopback-only random host-port publication, and a maximum 60-second readiness wait.
  - The runner passes one non-secret disposable URL through SDKWORK_DATABASE_TEST_POSTGRES_URL and never logs production credentials.
  - PostgreSQL lifecycle/seed/drift runs first, public schema is reset fail-closed, and Repository parity then runs against an empty schema.
  - Both cargo invocations select the exact ignored test and include --ignored --exact so neither suite can silently remain skipped.
  - Child commands have finite timeouts, captured Docker diagnostics are capped at 64 KiB, and finally cleanup force-removes the container.
  - sdkwork.workflow.json installs the locked pnpm workspace, declares every direct sibling build dependency including sdkwork-iam, and runs test:postgres:required during release validation.
  - The single thin package workflow handles pull_request, main push, tags, releases, and manual dispatch while continuing to call only the pinned reusable sdkwork-package workflow.
  - Release validation rejects generated contract drift after workspace and PostgreSQL verification.
non_functional_requirements:
  security: The database binds to 127.0.0.1 on an ephemeral port, uses test-only credentials, has no volume, and uses a digest-pinned image.
  performance: Exactly one PostgreSQL container and one test process run at a time; lifecycle pools remain capped at two connections and Repository parity at four.
  reliability: Each suite proves empty-schema startup, schema reset is fail-closed, and cleanup runs after success or failure.
affected_surfaces:
  - github-ci
  - release-workflow-contract
  - database-lifecycle-tests
  - sqlx-repository-parity-tests
trace:
  specs:
    - GITHUB_WORKFLOW_SPEC.md
    - PNPM_SCRIPT_SPEC.md
    - DATABASE_SPEC.md
    - DATABASE_FRAMEWORK_SPEC.md
    - TEST_SPEC.md
    - SECURITY_SPEC.md
  components:
    - sdkwork.workflow.json
    - .github/workflows/package.yml
    - scripts/postgres-ci-verify.mjs
    - crates/sdkwork-webserver-database-host
    - crates/sdkwork-intelligence-webserver-repository-sqlx
verification:
  - node --test tests/contract/postgres-ci.contract.test.mjs
  - pnpm test:postgres:required
  - node ../sdkwork-github-workflow/scripts/sdkwork-workflow.mjs validate --config sdkwork.workflow.json
  - node ../sdkwork-specs/tools/check-pnpm-script-standard.mjs --root . --product-prefix web
  - node ../sdkwork-specs/tools/check-agent-workflow-standard.mjs --root .
  - pnpm verify
  - git diff --check
```

## Compatibility Boundary

This requirement changes verification and release gating only. It does not alter the public API, database schema, runtime database selection, or production credentials. The existing ignored Rust tests remain ignored by default because they correctly refuse arbitrary databases; the mandatory runner supplies and owns the disposable database explicitly.

## Evidence Boundary

Passing this requirement proves PostgreSQL 16.9 lifecycle and Repository behavior in a single disposable instance. It does not prove PostgreSQL replication, failover, managed-provider differences, major-version compatibility, backup recovery, connection-proxy behavior, or sustained production load.

## Implementation Evidence

- `scripts/postgres-ci-verify.mjs` owns one digest-pinned PostgreSQL 16.9 Alpine container with a unique name, `--rm`, loopback-only random port, no volume, test-only credentials, a 60-second readiness deadline, 15-minute child deadlines, 64-KiB captured-output ceilings, and unconditional bounded cleanup even when container startup fails or times out.
- The runner verifies tracked build-critical Cargo, baseline, lifecycle-test, and Repository-test sources before execution and uses exact structured child arguments without a shell.
- Lifecycle/seed/drift and Repository parity run sequentially through their existing ignored-test safety boundary. A fail-closed `DROP SCHEMA public CASCADE; CREATE SCHEMA public` separates them, while both Rust suites independently reject non-empty schemas.
- `sdkwork.workflow.json` now declares the previously missing direct `sdkwork-iam` sibling dependency, installs with `pnpm install --frozen-lockfile`, runs normal workspace verification, runs mandatory PostgreSQL verification, and rejects generated drift before packaging completes.
- `.github/workflows/package.yml` remains one thin pinned reusable-workflow call and now covers pull requests and main pushes in addition to tags, releases, and manual dispatch. Pull requests use the head SHA as the safe package-run tag, and GitHub Release publishing stays disabled outside release/tag events.
- The root `test:postgres:required` script and `postgres-ci.contract.test.mjs` make the image digest, exact ignored tests, dependency closure, lifecycle steps, thin workflow ownership, and trigger coverage executable.

## Verification Evidence

- `pnpm test:postgres:required` passes against the pinned PostgreSQL image: lifecycle/seed/drift and full PostgreSQL repository parity each execute and pass after a fail-closed schema reset.
- `node --test tests/contract/postgres-ci.contract.test.mjs` passes 2/2.
- `sdkwork-workflow.mjs validate --config sdkwork.workflow.json` passes.
- SDKWork pnpm-script and agent/workflow standard validators pass; the workflow validator confirms exactly one compliant packaging workflow.
- The disposable container is absent after verification, proving normal-path cleanup.
- Final `pnpm verify` passes after the PostgreSQL gate changes, including Rust workspace tests, 21 Node contract tests, API materialization, SDKWork standards checks, topology/database validation, and cloud gateway validation.
