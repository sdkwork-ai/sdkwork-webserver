# REQ-2026-0054 Bounded Node Sync Manifest Materialization

```yaml
id: REQ-2026-0054
title: Bound legacy v3 node sync materialization before HTTP serialization
owner: sdkwork-webserver
status: accepted
source: control-plane-oom-and-node-integrity
problem: The Node Daemon bounded an HTTP sync response at 16 MiB, but the control plane first used fetch_all to collect every active tenant configuration and certificate, performed per-row domain queries, retained encrypted keys, decrypted the complete result, and only then serialized it. A large or corrupt tenant could therefore exhaust control-plane memory before the receiving node enforced its limit.
goals:
  - Bound rows and bytes at database projection, repository collection, service materialization, and Node Daemon consumption boundaries.
  - Stream database rows without an unbounded fetch_all collection or a per-row domain lookup.
  - Fail closed on oversized or malformed active configuration/certificate records.
  - Bind a sync response to the node identity acknowledged by heartbeat and reject ambiguous activation targets.
non_goals:
  - Claiming the v3 full-manifest protocol is a cursor-paged delta protocol.
  - Adding tombstones, stale-file deletion, snapshot transactions, atomic multi-artifact activation, or cluster convergence.
  - Renaming the public v3 Agent API or editing generated SDK output.
acceptance_criteria:
  - Nginx and certificate queries are deterministically ordered, streamed, and limited to at most 2049 candidate rows while one shared budget admits at most 2048 total items.
  - SQL projects Nginx content only when it is at most 1 MiB and certificate metadata only when it is at most 2 MiB, so an oversized database field is not decoded into application memory.
  - The Repository retains at most 12 MiB of serialized bundle plus encrypted-key bytes and rejects checked-add overflow.
  - Active certificate metadata that is invalid JSON, lacks certPem, or lacks encryptedPrivateKey fails the whole sync rather than silently omitting the certificate.
  - The service rejects certificate/key count mismatch and rejects a final decrypted manifest over 15 MiB before the route creates the SDKWork envelope.
  - The Node Daemon caps the envelope body at 16 MiB and rejects heartbeat/sync node identity mismatch, duplicate config IDs/domains, duplicate certificate IDs/names, negative config versions, and Nginx content fingerprint mismatch.
  - Disposable PostgreSQL repository verification executes the same oversize-record failure cases.
non_functional_requirements:
  memory: Every application-owned collection and response allocation in this path has a finite count or byte ceiling; no retry queue or all-page aggregation is introduced.
  security: Node identity and activation target ambiguity fail closed; certificate private keys are not logged or included in diagnostics.
  performance: The Nginx domain is projected by one correlated query instead of one additional pool query per configuration.
trace:
  specs:
    - RUST_CODE_SPEC.md
    - DATABASE_SPEC.md
    - DATABASE_FRAMEWORK_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-web-agent
verification:
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded -- --ignored --exact
  - pnpm test:postgres:required
  - cargo test -p sdkwork-intelligence-webserver-service
  - cargo test -p sdkwork-web-agent
  - pnpm verify
```

## Boundary

The 12 MiB Repository budget counts serialized bundle bytes and encrypted private-key bytes. The
service performs a second check after decryption because ciphertext and plaintext sizes are not
assumed equal. The 15 MiB domain-response ceiling leaves finite room beneath the Node Daemon's
16 MiB SDKWork envelope ceiling for `code`, `data.item`, `traceId`, property names, and framing.

Database `CASE` projections return `NULL` instead of the oversized text field while returning its
byte length separately. This allows the repository to reject the row without first decoding the
entire value. PostgreSQL uses `OCTET_LENGTH`, so multibyte text does not bypass the byte ceiling.

## Remaining Delta Gate

REQ-2026-0045 remains draft and authoritative for node-scoped immutable snapshots, opaque
tenant/node-bound cursors, bounded delta pages, tombstones, atomic snapshot application, and
high-cardinality cancellation/OOM evidence. This requirement deliberately makes the current v3
full-manifest path finite; it does not preserve that protocol as the final commercial cluster
distribution design.

## Implementation And Verification Evidence

- Repository unit tests pass 5/5, including checked item/serialized-byte budget failure.
- PostgreSQL Repository parity passes and executes both oversized Nginx content and oversized
  certificate metadata failure cases before restoring the fixtures.
- `pnpm test:postgres:required` passes against the digest-pinned disposable PostgreSQL image:
  lifecycle/seed/drift passes 1/1 and full Repository parity passes 1/1 with the same oversize cases.
- Node Daemon tests pass 8/8, including node identity/target/fingerprint validation, envelope decode,
  runtime bounds, alias migration, and durable generation recovery.
- The mandatory node-sync source contracts pass 3/3 and reject regression to `fetch_all` in this
  path; combined Node state/manifest contracts contribute 7/7 passing tests.
- `pnpm verify` passes in 240.9 seconds with the complete Rust workspace, 32 Node contract tests,
  API materialization, SDKWork validators, topology, PostgreSQL database, and cloud gateway validation.
- The disposable PostgreSQL container and network are absent after verification.
