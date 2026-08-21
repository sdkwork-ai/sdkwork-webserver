# REQ-2026-0036 Bounded Target Physical Connections

```yaml
id: REQ-2026-0036
title: Add bounded per-target authority physical connection limits
owner: sdkwork-webserver
status: accepted
source: nginx-upstream-server-max-conns-commercial-readiness
problem: The runtime enforces one aggregate physical connection ceiling per upstream, but one selected target can consume that entire budget. Nginx operators expect a `server max_conns`-style peer boundary, while an inaccurate request counter or pool-blind limit would miscount HTTP/2 and idle sockets.
goals:
  - Add an optional `targets[].maxConnections` limit bounded by the enclosing upstream maximum.
  - Acquire target and upstream physical capacity without queueing before DNS, TCP, or TLS work.
  - Hold both permits through connecting, TLS, active HTTP/1, multiplexed HTTP/2, and idle-pool socket ownership.
  - Keep target-authority capacity immutable and generation-owned so Watch replacement cannot reuse retired limits or security contexts.
  - Expose aggregate explicitly configured target capacity through fixed-cardinality node-local metrics.
  - Reject ambiguous duplicate target authorities whenever any per-target limit is configured.
non_goals:
  - Nginx shared-memory `zone`, cross-worker or cross-node connection accounting, connection queueing, or cluster-global pools.
  - Treating request/stream concurrency as physical socket concurrency.
  - Exact Nginx idle-keepalive exclusion; this safety profile counts idle sockets and is deliberately stricter.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - Omitted target limits preserve the existing aggregate-upstream connection behavior.
  - Schema and semantic validation enforce 1..100,000, target limit not above upstream limit, and unique normalized scheme/authority when any target limit is present.
  - Target saturation returns the existing bounded local 503 with retry guidance, does not poll the client Body, does not retry, and does not mutate active/passive health.
  - A target cannot open more physical sockets than configured even while aggregate upstream capacity remains.
  - One target's saturation does not consume another distinct target's target-specific permits.
  - HTTP/2 multiplexed Streams reuse one target permit instead of consuming one permit per Stream.
  - Idle expiry, transport failure, Watch retirement, shutdown, and cancellation release target and aggregate ownership.
  - Capacity state uses only bounded generation-time vectors, Semaphores, and fixed aggregate metrics; no request-derived Map, waiter queue, or periodic full connection inventory is introduced.
non_functional_requirements:
  security: Target limits cannot bypass DNS/SSRF policy, TLS identity, total upstream limits, or immutable generation isolation.
  privacy: Capacity telemetry contains no target URL, authority, address, route, tenant, user, request, or trace labels.
  performance: Connector lookup is one bounded linear scan over at most 1,000 targets only when opening a new socket; pooled request dispatch performs no target-capacity scan.
  reliability: Permit ownership is RAII and survives H1/H2 pooling semantics without detached cleanup tasks.
affected_surfaces:
  - sdkwork-webserver-app-config
  - request-data-plane
  - host-operations-plane
trace:
  specs:
    - CONFIG_SPEC.md
    - NGINX_SPEC.md
    - PERFORMANCE_SPEC.md
    - OBSERVABILITY_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_physical_connections
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test data_plane_metrics
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

`targets[].maxConnections` maps to the common safety intent of Nginx upstream `server ... max_conns=N`, but its accounting is intentionally stricter: the immutable Rust client counts every physical socket from connector admission through idle-pool drop. It does not claim Nginx `zone` sharing or exact worker-local idle-connection behavior.

## Acceptance Evidence

- The authored Rust model, root JSON Schema, compiler validation, checked-in example, and configuration documentation define optional `targets[].maxConnections` with range 1..100,000. Semantic validation rejects target ceilings above the enclosing upstream maximum and rejects duplicate normalized scheme/host/effective-port authorities whenever any target ceiling is present. Omitted target ceilings preserve existing origin-pool behavior. `cargo test -p sdkwork-webserver-core --test webserver_config` passes 52 tests.
- Each immutable upstream generation builds at most one fixed capacity entry per configured target. A new connector call performs one bounded authority scan, then obtains the aggregate upstream and applicable target Semaphore permits with non-queuing `try_acquire_owned` before DNS/TCP/TLS. Both permits live in the same `PermitStream` as the real socket, so connection failure, TLS failure, cancellation, H1/H2 close, idle expiry, Watch retirement, and shutdown release ownership through RAII.
- Target saturation maps to the existing local `503 Service Unavailable` plus `Retry-After: 1`, is classified as local connection capacity, does not poll the client Body, does not retry, and does not update target health. Distinct target authorities own independent Semaphores while the enclosing upstream cap remains authoritative.
- Real socket tests prove target saturation while aggregate capacity remains, recovery and pool reuse, independent dual-target capacity, one target's saturation without another target's socket growth, HTTPS/H2 concurrent Stream multiplexing through one target permit, idle ownership, active-health saturation isolation, Watch replacement of idle pools, retained old-generation streaming ownership, and shutdown release. `cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_physical_connections` passes 7 tests; the complete standalone gateway suite passes 163 tests.
- The operations listener exposes `sdkwork_web_data_plane_upstream_target_connection_capacity{state="configured|in_use|available"}` as one fixed aggregate family without authority, target, URL, address, upstream, route, tenant, user, request, or trace labels. The real streaming metrics test proves configured/in-use/available values while a target socket is active.
- `cargo clippy -p sdkwork-api-webserver-standalone-gateway --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, `cargo fmt --all -- --check`, and `git diff --check` pass.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, and database-framework validators passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers one process and one immutable runtime generation. Idle pooled sockets deliberately retain target capacity, making this profile stricter than Nginx worker-local idle behavior. Nginx shared-memory `zone`, cross-process or cross-node connection accounting, cluster-global pools, and exact directive import/render compatibility remain unimplemented.
