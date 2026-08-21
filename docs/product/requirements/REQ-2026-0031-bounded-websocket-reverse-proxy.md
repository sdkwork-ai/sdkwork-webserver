# REQ-2026-0031 Bounded WebSocket Reverse Proxy

```yaml
id: REQ-2026-0031
title: Proxy classic WebSocket upgrades with bounded lifetime and supervised shutdown
owner: sdkwork-webserver
status: accepted
source: runtime-correctness-compatibility
problem: The data plane rejects every protocol upgrade. Forwarding a 101 response alone would also be incomplete because Hyper transfers each upgraded socket out of the HTTP connection future, so the existing connection drain and maximum-age supervision would no longer own the long-lived traffic.
goals:
  - Proxy standards-conforming HTTP/1.1 WebSocket upgrade handshakes and transparent bidirectional bytes.
  - Preserve Hyper read-ahead bytes and TCP half-close behavior without WebSocket frame buffering.
  - Keep tunnel memory, lifetime, admission, reload ownership, and shutdown behavior explicitly bounded.
  - Preserve normal bounded proxy behavior for upstream handshake rejections such as 401 and 403.
  - Align the common Nginx WebSocket reverse-proxy operator behavior without claiming unsupported directives.
non_goals:
  - RFC 8441 HTTP/2 extended CONNECT.
  - WebSocket frame parsing, message limits, compression negotiation, heartbeat generation, or application authentication.
  - CONNECT or arbitrary protocol tunnels.
  - Exact Nginx proxy_read_timeout, proxy_send_timeout, proxy_buffer_size, map, sticky-session, or shared-zone semantics.
users:
  - platform operators
  - application developers
  - site reliability engineers
acceptance_criteria:
  - HTTP/1-only and mixed listeners can complete a real HTTP/1.1 WebSocket handshake; HTTP/2-only traffic cannot enter the classic upgrade path.
  - A request is eligible only when it is HTTP/1.1 GET, Connection contains the upgrade token, Upgrade is exactly websocket case-insensitively, and no HTTP request Body framing is present.
  - Malformed WebSocket upgrades return bounded 400 responses and other syntactically valid upgrade protocols return bounded 501 responses.
  - The upstream request removes hop-by-hop fields and restores canonical Connection: upgrade and Upgrade: websocket while preserving Sec-WebSocket-* fields.
  - An upstream 101 is accepted only with a valid WebSocket Connection/Upgrade response; invalid 101 responses become generic local 502 responses without Header or Body disclosure.
  - Non-101 upstream responses continue through the bounded streaming response path.
  - Each successful tunnel holds its upstream in-flight permit, upstream physical connection, and immutable runtime generation until tunnel completion.
  - Each tunnel uses fixed directional copy buffers, has a hard maximum lifetime derived from limits.maxConnectionAgeMs, and observes runtime shutdown.
  - Runtime shutdown stops new tunnel admission, closes active tunnels, and gives tunnel drain only the listener drain budget that remains from the configured drain timeout.
  - Reload publishes a new generation without terminating a tunnel owned by the previous generation.
non_functional_requirements:
  security: Upgrade negotiation fails closed; rejected upstream 101 metadata is not exposed; existing DNS/SSRF, TLS identity, Header limits, route selection, and request admission remain in force.
  privacy: The tunnel supervisor stores no payload, Header value, peer identity, or unbounded per-message metadata.
  performance: Each active tunnel owns two fixed 16 KiB copy buffers and no application-level queue, frame collection, or retained JoinHandle inventory.
  reliability: Every tunnel task selects over both upgrade futures, runtime shutdown, hard lifetime, and bidirectional copy completion; an active-count guard releases drain state on every exit path.
affected_surfaces:
  - runtime
  - proxy
  - http1
  - https
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - PERFORMANCE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test websocket_proxy
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

The runtime owns one `TunnelSupervisor`. A successful handshake transfers both Hyper `OnUpgrade` futures, the per-upstream in-flight permit, and an `Arc` to the immutable runtime generation into a finite detached task. The upgraded streams are adapted directly to Tokio I/O and bridged with fixed 16 KiB directional buffers. No WebSocket frame or message is parsed or retained.

`limits.maxConnectionAgeMs` is the hard lifetime for both ordinary HTTP connections and upgraded tunnels. `deployment.drainTimeoutMs`, falling back to `limits.drainTimeoutMs`, is one shared listener-and-tunnel drain budget; the tunnel supervisor receives only the time left after listener tasks drain. The supervisor retains only an atomic active count, a shutdown Watch channel, and a drain notification; it does not accumulate completed task handles.

## Compatibility Boundary

This requirement aligns the standard Nginx deployment pattern that explicitly forwards `Upgrade` and `Connection` for classic HTTP/1.1 WebSocket proxying. SDKWork sanitizes and regenerates these fields automatically for an eligible proxy route. It does not claim Nginx directive-level compatibility, RFC 8441, protocol-aware WebSocket policy, or cluster-wide tunnel accounting.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- Standalone gateway tests passed with 58 library tests, 55 primary data-plane integration tests, 4 raw HTTP/1 tests, 1 resource-pressure test, 4 active-health tests, 5 physical-connection tests, 4 response-Header tests, 2 weighted-selection tests, and 9 focused WebSocket tests: 142 tests total.
- Classifier tests prove HTTP/1.1 GET, exact Connection token, unique case-insensitive WebSocket Upgrade, absent Body framing, HTTP/2 rejection, and explicit unsupported-protocol classification.
- Real raw-socket tests prove HTTP/1-only upgrade, upstream and client bytes sent in the same packet as their handshake, 70,000-byte bidirectional transfer across multiple fixed buffers, and propagated TCP half-close.
- A real mixed HTTP/1+HTTP/2 HTTPS listener proves certificate-verified TLS, HTTP/1.1 ALPN, `101`, and encrypted tunnel bytes. Existing upstream TLS suites continue to prove private-CA, hostname, mTLS, version, and Watch pool isolation used by HTTPS WebSocket targets.
- Rejection tests prove malformed/no-Connection/non-GET/Body-framed attempts fail before upstream connect, other protocols return `501`, upstream `403` remains a streamed response, and an invalid upstream `101` returns generic `502` without its secret Header or Body.
- Capacity tests separately prove that one tunnel holds `maxInFlightRequests` and the physical `maxConnections` socket permit through its lifetime, rejects excess work immediately, and recovers after close.
- Watch evidence proves a replacement generation serves new tunnels while a retired-generation tunnel remains bidirectionally usable; the old upstream physical connection releases only after that tunnel closes.
- Lifetime and shutdown evidence prove `maxConnectionAgeMs` closes a tunnel and process shutdown closes active tunnels and returns within the shared drain budget. The HTTP/1 wire guard regression suite proves normal framing/Pipeline behavior remains intact.
- Full-workspace `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and the environment-independent `pnpm.cmd verify` checks passed with the isolated target directory. PostgreSQL lifecycle was not executed in this requirement's acceptance run; REQ-2026-0004 and REQ-2026-0049 own that evidence.
- Pagination, API operation patterns, API response envelope, app SDK consumer imports, application layering, Rust backend composition, repository documentation, topology, cloud-gateway configuration, and `git diff --check` passed.
