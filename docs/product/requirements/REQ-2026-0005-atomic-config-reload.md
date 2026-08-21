# REQ-2026-0005 Atomic Same-Topology Configuration Reload

```yaml
id: REQ-2026-0005
title: Atomically reload verified Web Server traffic configuration without restarting listeners
owner: SDKWork maintainers
status: accepted
source: reliability
problem: A production Web Server must publish route and upstream changes without restarting healthy listeners, exposing partially compiled state, or replacing the active generation when a candidate is invalid.
goals:
  - Compute a stable SHA-256 revision from the exact bounded configuration bytes that passed validation.
  - Watch an explicitly enabled configuration file at a bounded interval.
  - Build the complete immutable candidate, including upstream clients, before publication.
  - Atomically switch request routing to one complete generation without request-path locks.
  - Retain the last active generation when reading, parsing, schema, semantic, static-root, upstream, or topology validation fails.
  - Require process restart for listener, TLS material, admission, timeout, drain, or watch-policy topology changes.
non_goals:
  - Adding, removing, rebinding, or changing protocols on live listeners.
  - Live certificate/SNI map rotation or executable binary upgrade.
  - Persisted revision catalogs, operator-selected rollback, cluster canary rollout, or node convergence.
  - Claiming the parent commercial reload/rollback gate is complete from this local-file slice alone.
users:
  - Platform operators
  - Site reliability engineers
  - Web application developers
acceptance_criteria:
  - Configuration loading cannot allocate more than the 1 MiB limit when a file changes between metadata inspection and reading.
  - Identical configuration bytes produce the same SHA-256 revision and changed bytes produce a different revision.
  - Watch mode is opt-in and polling is bounded from 100 ms through 60 seconds.
  - A valid same-topology candidate changes real HTTP responses without listener restart.
  - Invalid JSON and restart-only topology changes keep the previous response generation active.
  - Concurrent requests during repeated reloads observe only complete old or new generations.
  - Reload serialization does not add a lock to the request path or hold a request-path lock across asynchronous I/O.
  - Reload worker shutdown completes with normal listener drain and does not deadlock.
  - TLS certificate and private-key reads are bounded to 1 MiB per file before Rustls parsing.
non_functional_requirements:
  security: Candidate content, private keys, tokens, and request data are never logged; only bounded revision identifiers and classified errors are emitted.
  privacy: Configuration diagnostics do not include file content or secret bytes.
  performance: Each request performs one lock-free generation load; candidate compilation and upstream construction remain outside the request path.
  reliability: Publication is all-or-nothing and a failed candidate never replaces the active generation.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - CONFIG_SPEC.md
    - RUST_CODE_SPEC.md
    - SECURITY_SPEC.md
    - DEPLOYMENT_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test data_plane_integration
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm verify
```

Accepted on 2026-07-16 for local-file, same-topology reload only. The parent PRD remains active because persisted rollback, certificate rotation, listener handoff, executable upgrade, signed cluster snapshots, canary rollout, fencing, convergence evidence, and multi-node failure recovery are separate release requirements.
