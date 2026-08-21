# REQ-2026-0052 Durable Agent Sync Generation

```yaml
id: REQ-2026-0052
title: Persist bounded desired and observed edge-agent sync generations
owner: sdkwork-webserver
status: accepted
source: cluster-activation-convergence-readiness
problem: The edge agent persisted only lastSyncVersion through an unbounded, non-atomic, best-effort JSON file in a temporary directory. Corruption silently reset state, and a process crash could not distinguish a received manifest from a completely materialized and reloaded generation.
goals:
  - Persist separate desired and observed sync versions with a monotonic local revision.
  - Write desired before any Nginx or certificate materialization and write observed only after every supplied artifact and real Nginx reload succeed.
  - On restart or retry, send only observed to the control plane so an interrupted desired generation receives a complete deterministic replay.
  - Use canonical SDKWork durable data directories rather than repository or temporary paths.
  - Bound, checksum, validate, atomically replace, sync, and permission-protect the local state file.
  - Bound control-plane response bodies, manifest collection counts, request time, sync frequency, URL shape, and token size while reusing one HTTP connection pool.
  - Decode heartbeat and sync through the canonical SDKWork `code/data/item/traceId` resource envelope and reject non-success business codes.
non_goals:
  - Claiming observed proves a served TLS fingerprint, full desired inventory equality, cluster quorum, or control-plane acknowledgement.
  - Deleting stale Nginx/certificate files absent from a newer full manifest; destructive reconciliation requires a reviewed inventory and rollback contract.
  - Replacing the raw AgentToken transport with the generated Rust backend SDK before the AgentToken OpenAPI/security generator contract receives human approval.
  - Signed state, distributed/multi-node leader election, or node readiness publication. Same-state-directory process exclusion is subsequently delivered by REQ-2026-0055.
acceptance_criteria:
  - State schema records schemaVersion, monotonic revision, desiredSyncVersion, observedSyncVersion, and a SHA-256 checksum over the canonical checksum payload.
  - Sync versions use exact sv1 plus 64 lowercase hexadecimal SHA-256 bytes; unknown fields, unsupported schema versions, invalid transitions, checksum mismatch, empty files, oversized files, non-files, and symlinks fail closed.
  - The state file is at most 8 KiB, is staged in the target directory, flushed, fsynced, atomically persisted, and directory-synced; Unix state files use mode 0600.
  - Linux defaults to /var/lib/sdkwork/webserver/edge, Windows to ProgramData/sdkwork/webserver/Data/edge, and macOS to the SDKWork Application Support data directory; every explicit override must be absolute.
  - Exact legacy lastSyncVersion state migrates to equal desired/observed values, while malformed or extended legacy input is rejected.
  - The Agent process loads state once at startup and fails closed on corruption rather than looping with an empty version.
  - A changed manifest is validated and desired is durably saved before the first artifact write; observed is durably saved only after edge.reload succeeds.
  - Heartbeat and ifSyncVersion use observed only. A pending desired generation therefore cannot suppress full replay after restart.
  - Generated backend SDK clients are reused across cycles; timeout is 60 seconds, heartbeat response is at most 64 KiB, sync response at most 16 MiB, and each manifest contains at most 2048 Nginx plus 2048 certificate entries.
  - Heartbeat and sync reject a missing or malformed SDKWork resource envelope, a non-zero business code, and an empty traceId before using the response item.
  - Control-plane URL is an HTTP(S) origin without credentials/path/query/fragment, AgentToken is 16..4096 non-control bytes, and sync interval is 1..3600 seconds.
non_functional_requirements:
  security: State contains no token, private key, PEM, URL, tenant payload, or raw diagnostics. SHA-256 detects corruption but is not represented as authenticity or a signature.
  reliability: Crash before observed persistence leaves desired pending and forces complete replay; corrupt state blocks startup rather than producing false convergence.
  performance: State memory is fixed-small, response and collection allocation are finite, and one HTTP connection pool is reused instead of rebuilt every cycle.
affected_surfaces:
  - edge-agent-runtime
  - local-generation-state
  - nginx-certificate-activation-checkpoint
  - control-plane-ingress-bounds
trace:
  specs:
    - RUNTIME_DIRECTORY_SPEC.md
    - CONFIG_SPEC.md
    - ENVIRONMENT_SPEC.md
    - DEPLOYMENT_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
    - SDK_SPEC.md
    - SDK_WORKSPACE_GENERATION_SPEC.md
    - APP_SDK_INTEGRATION_SPEC.md
  components:
    - crates/sdkwork-web-agent
verification:
  - cargo test -p sdkwork-web-agent
  - node --test tests/contract/agent-sync-state.contract.test.mjs
  - pnpm verify
  - node ../sdkwork-specs/tools/check-pnpm-script-standard.mjs --root . --product-prefix web
  - git diff --check
```

## State Boundary

`observedSyncVersion` means this agent durably completed every artifact supplied in that response and the configured Nginx reload command returned success. It does not prove that the local filesystem contains no stale artifact omitted from the manifest, that a public TLS handshake serves the intended fingerprint, that the control plane received an acknowledgement, or that other nodes converged.

Because stale-artifact removal is destructive, this requirement does not silently delete files. A later reviewed reconciliation requirement must introduce a bounded desired inventory, ownership markers, path confinement, rollback, deletion audit, and served-state probes before full node convergence can be claimed.

## SDK Security Boundary

The application-root Rust backend SDK contains Agent methods, but the current authority materializer emits `security: []` for AgentToken routes and the generator exposes no typed AgentToken credential provider. Calling generic `set_header` would still be manual auth assembly and would violate the backend SDK integration standard. This requirement therefore leaves the existing transport explicitly non-compliant rather than hand-editing generated SDK output or claiming a generic header is a valid SDK integration. Correcting the AgentToken OpenAPI/security/generator contract requires human review.

The generated family metadata also remains inconsistent: `sdks/sdkwork-web-backend-sdk/sdk-manifest.json` declares zero `sdkDependencies`, while the materialized component contract declares one `sdkwork-iam-backend-sdk` dependency. The current validators do not reject this drift. Deciding whether the component dependency is incorrect or the family manifest/generation configuration is incomplete changes generated SDK ownership metadata and therefore remains part of the same human-review gate.

## Implementation Evidence

- `state.rs` replaces best-effort temporary JSON with a strict fixed schema, canonical sync-version validation, SHA-256 corruption detection, bounded reads, atomic same-directory persistence, fsync, Unix mode 0600, symlink rejection, and exact legacy migration.
- `main.rs` loads state before entering the loop, sends observed only, persists desired before any deployment, and advances observed only after the real reload succeeds.
- The Node Daemon reuses generated backend SDK clients whose transport incrementally reads response
  chunks under application-owned byte ceilings before SDKWork envelope decoding. Collection counts,
  URL/token input, and loop frequency are also finite.
- The Agent decodes the real route shape with the shared `SdkWorkApiResponse<SdkWorkResourceData<T>>` contract, validates the success code and trace ID, and only then exposes `data.item` to heartbeat/sync logic. This fixes runtime envelope interoperability without claiming that the unresolved generated AgentToken client contract is compliant.
- Executable shared-route tests prove that `ok_resource` emits the same canonical Agent sync envelope consumed by the Agent. They also caught and now prevent a production panic caused by passing the mixed-case SDKWork trace-header constant to `HeaderName::from_static`; success and Problem Details responses use fallible header-name parsing.
- Unit and repository contract tests lock the crash-replay ordering and reject regression to temporary, unbounded, silent-default, or response.json behavior.

## Verification Evidence

- `cargo test -p sdkwork-web-agent` passes 5/5 for canonical envelope decoding, desired/observed restart recovery, corruption, invalid transitions, exact legacy migration, oversized input, and runtime input bounds.
- `cargo test -p sdkwork-routes-webserver-common` passes 2/2 for canonical Agent resource serialization and non-panicking trace headers on success and Problem Details responses.
- `node --test tests/contract/agent-sync-state.contract.test.mjs` passes 3/3.
- `pnpm verify` passes in 266.5 seconds, including the complete Rust workspace, 28 Node contract tests, API materialization, SDKWork repository/API/document/script/agent-workflow validators, topology, PostgreSQL database validation, and cloud-gateway validation.
- `git diff --check` passes; line-ending notices are informational and no whitespace errors are present.
