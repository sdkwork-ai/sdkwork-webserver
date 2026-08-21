# REQ-2026-0047 Fail-Closed Bounded Nginx Edge Activation

```yaml
id: REQ-2026-0047
title: Replace fake Nginx validation and reload success with bounded fail-closed edge activation
# note: this requirement governs the optional legacy external-Nginx edge adapter
# (sdkwork-webserver-edge-runtime). The certificate lifecycle main path is the
# self-hosted data plane TLS runtime (versioned material root + tls-runtime
# snapshot) and does not involve Nginx.
owner: sdkwork-webserver
status: accepted
source: nginx-edge-commercial-readiness
problem: The edge adapter runs nginx -t against the default main config instead of the candidate site content, accepts any non-empty candidate after validation failure, and converts reload failure into success. The control plane can therefore report valid, deployed, or reloaded while Nginx rejected the configuration or never reloaded.
goals:
  - Test the exact candidate site content inside a generated minimal Nginx main configuration before activation.
  - Fail closed when Nginx is disabled, unavailable, times out, rejects the candidate, or rejects reload.
  - Bound candidate bytes, command duration, diagnostic bytes, temporary files, and process count.
  - Validate domain-derived file names and prevent path traversal outside the configured sites root.
  - Use unique same-directory staging files and atomic persistence so concurrent deploy attempts do not share a fixed .tmp path.
  - Isolate blocking filesystem/process operations from async backend request executor threads.
  - Ensure deploy and reload API responses cannot claim success after an edge operation failed.
non_goals:
  - Cross-database-and-Nginx distributed transactions or schema migration for activation generations.
  - Nginx configuration rendering/import, directive translation, or complete semantic compatibility.
  - Starting or supervising the Nginx master process.
  - Replacing the Rust standalone data plane with Nginx.
users:
  - operators managing optional external Nginx edge nodes through backend-api
  - edge agents applying reviewed Nginx site configurations
acceptance_criteria:
  - Validation writes the exact candidate to an isolated temporary file and invokes nginx -t with a generated main config that includes that file inside http context.
  - Empty or greater-than-1-MiB candidate content fails before filesystem or process execution.
  - nginx disabled, spawn failure, non-zero exit, signal termination, and timeout all return errors; no degraded non-empty fallback exists.
  - Command execution retains at most 8 KiB of diagnostic output and has a configurable 100..60000 ms timeout with a 10000 ms default.
  - Domain file names are 1..253 safe ASCII DNS-style characters with no separator, dot-segment, control, drive, or alternate-path syntax.
  - Deployment stages to a unique file in the target directory, validates that exact file, syncs it, and atomically persists only after validation succeeds.
  - Backend validation/deploy/reload runs blocking edge work through spawn_blocking and propagates task or edge errors.
  - The deploy operation preflights content and domain before changing database active state and does not swallow content, domain, deployment, or reload failures.
  - Real Nginx 1.26.2 evidence proves one valid candidate passes and one invalid directive fails without replacing the prior target.
non_functional_requirements:
  security: Candidate text and Nginx diagnostics are not reflected unboundedly, path-derived values cannot escape configured roots, and failure never becomes success.
  performance: One administrative operation owns at most one child process, one bounded diagnostic file, one candidate file, one wrapper file, and one finite timeout; no child stdout/stderr pipe can deadlock or grow process memory.
  reliability: Validation failure leaves the active file unchanged, unique staging avoids shared-temp races, and reload errors remain visible to callers.
affected_surfaces:
  - webserver-edge-runtime
  - webserver-business-service
  - backend-api-nginx-operations
trace:
  specs:
    - NGINX_SPEC.md
    - CONFIG_SPEC.md
    - DEPLOYMENT_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-edge-runtime
    - crates/sdkwork-intelligence-webserver-service
verification:
  - cargo test -p sdkwork-webserver-edge-runtime
  - cargo test -p sdkwork-intelligence-webserver-service
  - cargo clippy -p sdkwork-webserver-edge-runtime --all-targets -- -D warnings
  - cargo clippy -p sdkwork-intelligence-webserver-service --all-targets -- -D warnings
  - node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root . --strict
  - cargo fmt --all -- --check
  - git diff --check
  - pnpm.cmd verify
```

## Compatibility Boundary

Returning success after failed syntax validation or reload is not a compatibility behavior; it is false state. This requirement deliberately changes those cases to errors without changing the public OpenAPI shape. `NginxValidateResponse.valid` remains the validation result contract, while deploy and reload operations now fail through the existing standard problem envelope when the host capability cannot complete.

External Nginx remains optional. Operators that do not run it must keep this edge capability disabled and must not call its deploy/reload operations. Disabled capability is represented as unavailable, not as a successful no-op.

## Implementation Evidence

- `EdgeRuntimeConfig` now owns strict `nginx_enabled`, the main config path, and a 100..60000 ms command timeout with a 10000 ms default. Unknown boolean tokens and invalid timeout values fail bootstrap instead of silently enabling Nginx.
- Validation writes the exact candidate and a generated minimal `events`/`http` wrapper into isolated temporary directories. The wrapper includes the absolute candidate path, creates bounded validation runtime directories, and invokes the configured Nginx binary with `-t -q -p <isolated-prefix> -c <wrapper>`.
- Child stdin/stdout are null, stderr is redirected to an OS temporary file, the process is polled with a finite deadline, timeout kills and reaps the child, and internal diagnostics retain at most 8192 bytes. Exit/signal/spawn/timeout failures all return an error; the previous non-empty degraded fallback and reload-success conversion are removed.
- Candidate content is non-empty and at most 1 MiB. Deployment validates a strict DNS-style domain, stages through a unique same-directory `NamedTempFile`, flushes and syncs it, validates that exact staged file, atomically persists it, and syncs the parent directory where supported. Failed validation leaves the previous target byte-for-byte unchanged and leaks no wrapper/runtime files into the sites directory.
- `WebService` runs validation, deployment, and reload through `spawn_blocking`. Backend activation resolves the candidate site/domain and validates content before database active-state mutation, then propagates deployment and reload failures instead of logging and returning success.
- The legacy Repository validation port retains its Rust signature but can no longer claim syntax validity without a host provider. It returns `valid: false` with an explicit edge-runtime requirement; the real backend route continues through service plus edge runtime.
- Every authored Rust workspace crate now owns `specs/component.spec.json` and `specs/README.md`. The newly declared route, contract, service, provider, worker, agent, and host ports describe only existing exports and generated route authorities.

## Verification Evidence

- `cargo test -p sdkwork-webserver-edge-runtime` passes 8/8 unit/integration tests. The installed `nginx/1.26.2` accepts a valid exact candidate, rejects an unknown directive, preserves the previous active file after rejection, replaces it with a second valid candidate, and leaves one target file only.
- Edge tests also prove strict enable tokens, path traversal/DNS rejection, empty and over-1-MiB rejection, 8-KiB diagnostic truncation, disabled/unavailable failure, reload spawn failure, and a 100-ms child timeout that kills a five-second process.
- `cargo test -p sdkwork-intelligence-webserver-service` passes 2/2 tests and compiles the async Nginx orchestration. PostgreSQL Repository parity passes with the conservative legacy validation result while retaining transaction, tenant, pagination, Nginx activation, certificate, agent, and audit coverage.
- Strict all-target Clippy passes for edge runtime, business service, and SQLx Repository with `-D warnings`.
- Strict component-port binding, application-layering, and route-collision validators pass after all nine missing Rust component contracts were added; no Cargo member remains without `specs/component.spec.json` or `specs/README.md`.
- Isolated-target, environment-independent `pnpm.cmd verify` checks passed workspace Rust tests, contract tests, API materialization consistency, repository standards, topology, database-framework, and cloud-gateway validation. PostgreSQL lifecycle was not executed or claimed by this Nginx activation requirement; REQ-2026-0004/0049 own that evidence.
- `cargo fmt --all -- --check` and `git diff --check` pass.

## Remaining Boundary

This requirement cannot make database active-state mutation and a separate Nginx master reload one atomic transaction. A durable activation generation, desired/observed state, rollback target, node acknowledgement, and reconciliation loop require a reviewed database/API/Agent contract and remain a commercial HA gate. Complete Nginx directive compatibility, fuzz/differential testing, signed configuration provenance, multi-node rollout, and chaos evidence also remain separate gates.
