# REQ-2026-0051 Bounded PostgreSQL Streaming Failover

```yaml
id: REQ-2026-0051
title: Prove bounded PostgreSQL physical replication and standby promotion
owner: sdkwork-webserver
status: accepted
source: database-high-availability-readiness
problem: PostgreSQL lifecycle, Repository parity, and backup restoration pass, but there is no executable evidence that an acknowledged control-plane write can reach a physical standby, survive primary shutdown, and remain writable after standby promotion.
goals:
  - Build a physical standby from the application baseline through pg_basebackup and a fixed physical replication slot.
  - Prove the primary observes one streaming replication connection and the standby remains in recovery before failover.
  - Record the primary WAL flush LSN and wait until the standby replay LSN reaches it before declaring the pre-failover write converged.
  - Stop the primary before promotion, promote the standby with pg_ctl, and prove preserved plus new tenant-scoped writes.
  - Bound containers, network, time, readiness, convergence, output, source input, memory, CPU, PIDs, shared memory, and temporary storage.
  - Make the real failover drill mandatory in shared merge and release validation.
non_goals:
  - Automatic failure detection, consensus, leader election, connection endpoint failover, retry orchestration, or topology discovery.
  - Synchronous replication, arbitrary in-flight-write RPO, split-brain fencing, failback, replica rejoin, cascading replicas, or multi-primary operation.
  - Managed PostgreSQL provider certification, Kubernetes operators, multi-zone capacity, cross-region replication, PITR, or product RPO/RTO acceptance.
  - Claiming one primary and one standby satisfies the three-node/multi-zone product availability target.
acceptance_criteria:
  - The drill uses two uniquely named --rm containers from the digest-pinned PostgreSQL 16.9 Alpine image, one unique internal bridge network, no host ports, no volumes, and test-only credentials.
  - Each container is limited to 256 MiB memory, one CPU, 256 PIDs, 32 MiB shared memory, 128 MiB PGDATA tmpfs, 16 MiB temporary tmpfs, and 1 MiB PostgreSQL socket tmpfs.
  - The primary uses wal_level=replica, finite WAL sender/slot counts, explicit SCRAM replication authorization, and one named physical replication slot.
  - The standby uses pg_basebackup with write-recovery-conf, stream WAL mode, the fixed slot, no interactive password prompt, and PGDATA mode 0700.
  - Before failover, pg_stat_replication reports one streaming replica, pg_is_in_recovery is true on standby, and its replay LSN reaches the recorded primary flush LSN.
  - The primary container is stopped before pg_ctl promote runs, promoted pg_is_in_recovery becomes false, and the new primary retains the replicated tenant canary and accepts a second tenant canary.
  - Total duration is at most ten minutes, readiness and convergence polling are each at most sixty attempts, captured output is at most 64 KiB, and the baseline is at most 2 MiB.
  - Failure paths collect only bounded standby diagnostics and unconditionally attempt to remove both containers and the network.
  - Root and shared workflow validation invoke the drill, and contract tests pin its topology, commands, ordering, bounds, and cleanup.
non_functional_requirements:
  security: Test credentials never leave the isolated fixture, the network has no external route, no database port is published, and only bounded PostgreSQL diagnostics are captured.
  reliability: Promotion is fenced by explicit primary shutdown and exact WAL replay evidence; every subprocess and cleanup path is finite and fail-closed.
  performance: Two PostgreSQL processes execute sequential setup and bounded polling under fixed aggregate CPU, memory, PID, shared-memory, and tmpfs budgets.
affected_surfaces:
  - postgresql-high-availability-evidence
  - database-release-verification
  - release-workflow-contract
trace:
  specs:
    - DATABASE_SPEC.md
    - DATABASE_FRAMEWORK_SPEC.md
    - DEPLOYMENT_SPEC.md
    - TEST_SPEC.md
    - SECURITY_SPEC.md
    - GITHUB_WORKFLOW_SPEC.md
    - PNPM_SCRIPT_SPEC.md
  components:
    - scripts/postgres-ha-verify.mjs
    - sdkwork.workflow.json
verification:
  - node --test tests/contract/postgres-ha.contract.test.mjs
  - pnpm test:postgres:ha
  - node ../sdkwork-github-workflow/scripts/sdkwork-workflow.mjs validate --config sdkwork.workflow.json
  - node ../sdkwork-specs/tools/check-pnpm-script-standard.mjs --root . --product-prefix web
  - pnpm verify
  - git diff --check
```

## Evidence Boundary

This requirement proves one explicitly converged asynchronous physical standby can be promoted after the primary has been stopped, preserving the write whose flush LSN was observed and replayed. It does not prove zero-loss failover for arbitrary in-flight writes, automatic leader election, connection rerouting, split-brain prevention under a network partition, failback, managed-provider behavior, multi-zone capacity, or the PRD RPO/RTO and availability targets.

## Implementation Evidence

- `postgres-ha-verify.mjs` creates one internal Docker network and two resource-limited containers from the same digest-pinned PostgreSQL image used by database parity and recovery verification.
- The primary installs explicit SCRAM replication authorization, reloads configuration, creates one physical slot, applies the bounded tracked baseline, and writes a tenant canary.
- The standby is built with real `pg_basebackup`, starts with private `0700` PGDATA, and must appear as `streaming` while it reports recovery mode.
- A pre-failover update is fenced by an exact primary flush LSN and standby replay-LSN comparison. The primary is then stopped before bounded `pg_ctl promote`; the promoted node must preserve the first canary and accept a second.
- Startup diagnostics, polling, process duration, input, output, container resources, and cleanup are all finite. Cleanup is unconditional even when network/container creation or process startup fails.

## Verification Evidence

- `pnpm test:postgres:ha` passes against the digest-pinned PostgreSQL 16.9 Alpine image: one streaming standby is observed, WAL LSN `0/30001F8` is replayed, the primary is stopped, promotion succeeds, and the promoted node preserves and accepts tenant-scoped writes.
- `node --test tests/contract/postgres-ha.contract.test.mjs` passes 2/2 and pins the digest, physical base-backup/slot contract, exact replay evidence, shutdown-before-promotion ordering, internal topology, finite resources, unconditional cleanup, and release-gate placement.
- `pnpm verify` passes with 25 Node contract tests plus the Rust HTTP/HTTPS, Nginx, resource-pressure, ACME, PostgreSQL recovery, database lifecycle, API materialization, SDKWork standards, topology, database-framework, and cloud-gateway verification surfaces.
- Shared workflow, pnpm-script, Agent/workflow, formatting, and diff validators pass. No `sdkwork-web-ha-*` container or network remains after successful or failed runs.
