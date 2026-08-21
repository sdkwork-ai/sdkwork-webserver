# REQ-2026-0044 Bounded PROXY Protocol v1/v2

```yaml
id: REQ-2026-0044
title: Add bounded trusted-source HAProxy PROXY Protocol v1/v2
owner: sdkwork-webserver
status: accepted
source: nginx-proxy-protocol-commercial-readiness
problem: A Web Server behind a TCP load balancer must recover the original client address before TLS or HTTP parsing, but accepting an untrusted or auto-detected PROXY header would let a direct client forge transport identity and influence forwarding, affinity, and retries.
goals:
  - Add one listener-local mandatory PROXY v1/v2 transport policy with no trust by default.
  - Authenticate the identity assertion by immediate TCP peer CIDR before consuming header bytes.
  - Parse before TLS ClientHello and HTTP while bounding time, total bytes, allocation, tasks, and telemetry cardinality.
  - Support common TCP4/TCP6, v1 UNKNOWN, and v2 LOCAL semantics without consuming TLS/HTTP payload bytes.
  - Use the resolved source as the same effective client identity used by canonical upstream XFF, IP-hash, and safe retries.
non_goals:
  - Optional or automatic PROXY-header detection.
  - PROXY v2 TLV semantic consumption beyond bounded CRC32C integrity validation, authority/SSL TLV trust, or TLV forwarding.
  - AF_UNIX, UDP/datagram, outbound send-proxy, hostname trust sources, or dynamic cloud-provider CIDR discovery.
  - Nginx directive import/render or byte-for-byte implementation equivalence.
users:
  - operators placing the Rust Web Server behind trusted L4 load balancers or ingress proxies
  - applications requiring original client IP for bounded affinity and canonical forwarding
acceptance_criteria:
  - Omitted proxyProtocol consumes no transport preface and preserves the accepted TCP peer.
  - Configured proxyProtocol requires every connection to start with an enabled valid v1/v2 Header; missing or invalid input closes without an HTTP response.
  - Only an immediate peer in 1..64 unique trustedSourceCidrs may provide a Header.
  - Versions are a non-empty unique subset of v1/v2; timeoutMs is 100..10000 and maxHeaderBytes is 107..4096.
  - v1 has the protocol maximum of 107 bytes including CRLF, requires strict CRLF, and accepts TCP4, TCP6, and UNKNOWN.
  - v2 validates signature/version/command/family/protocol/address length and accepts LOCAL or PROXY over TCP4/TCP6.
  - v2 remaining bytes, including TLVs, are discarded within the declared and configured total bound through fixed scratch memory.
  - v2 CRC32C policy is explicit (`ignore`, `validate-if-present`, or `required`); malformed, duplicate, missing-required, or mismatched CRC TLVs fail closed.
  - Parsing occurs after non-queuing connection admission but before TLS/HTTP and does not block the listener accept loop.
  - proxyProtocol and HTTP trustedProxy are mutually exclusive, and any proxyProtocol policy change is Restart-only.
  - HTTP/1, HTTPS/H2, fragmented v1, v1/v2 IPv4/IPv6, UNKNOWN, LOCAL, TLV, timeout, malformed, oversized, untrusted, disabled-version, and Watch behavior have executable tests.
non_functional_requirements:
  security: Immediate-peer trust is checked before bytes are consumed; auto-detection is forbidden; rejection exposes no attacker-controlled body or identity in logs or labels.
  privacy: Transport and resolved client addresses are not added to metric labels or new access-log fields.
  performance: Each accepted connection uses fixed parser buffers, exact bounded reads, no header-sized heap allocation, no per-client map, no queue, and no lock across I/O.
  reliability: Connection permits bound slow prefaces; the per-connection timeout releases permits; immutable listener policy cannot change under an accepted connection.
affected_surfaces:
  - sdkwork-webserver-app-config
  - request-data-plane
trace:
  specs:
    - CONFIG_SPEC.md
    - NGINX_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test proxy_protocol
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

The wire parser follows HAProxy PROXY protocol v1/v2 framing and the common Nginx `listen ... proxy_protocol` deployment intent. Configuration presence is mandatory rather than optional because the protocol authority explicitly warns receivers not to guess whether a connection carries a PROXY Header. SDKWork adds immediate-source CIDR trust, finite time/byte ceilings, strict canonical v1 ports, and fail-closed version selection. These are deliberate security constraints, not a claim of complete Nginx directive compatibility.

## Implementation Evidence

- The root JSON Schema and public Rust model expose optional listener `proxyProtocol` with 1..64 trusted non-overbroad CIDRs, a unique v1/v2 set, a finite timeout, a finite total Header ceiling, and an explicit CRC32C policy. Defaults are both versions, 3 seconds, 536 bytes, and CRC ignore. Semantic validation rejects malformed, duplicate, overbroad, or IPv4-universal mapped IPv6 networks and simultaneous `trustedProxy`.
- The listener obtains process/listener connection capacity before spawning the connection task. That task validates the immediate peer, resolves the bounded preface, and only then constructs the ConnectInfo service and enters the TLS/HTTP acceptor chain. Slow or partial input therefore remains inside the existing connection ceiling and never stalls the accept loop.
- v1 uses one 107-byte stack line, strict CRLF, exact consumption, canonical decimal ports, and typed IPv4/IPv6 parsing. v2 uses fixed signature/fixed-address buffers, validates declared total length before reading it, streams bounded TLVs through fixed scratch memory, and applies the configured duplicate/missing/mismatch CRC32C policy without promoting TLVs to request identity.
- `DownstreamConnectionInfo` retains both transport and resolved client peers. The handler uses only the resolved client peer for the existing effective-IP path, so canonical upstream XFF, initial IP-hash, and safe retry selection remain consistent.
- Rejections increment one fixed `proxy_protocol` reason and close before HTTP. No address, Header bytes, TLV data, listener id, or client-controlled value becomes a metric label.

## Verification Evidence

- `cargo test -p sdkwork-webserver-core --test webserver_config` passes 60 configuration tests, including strict PROXY defaults, bounds, unknown-field rejection, CIDR/version validation, and `trustedProxy` mutual exclusion.
- `cargo test -p sdkwork-api-webserver-standalone-gateway --test proxy_protocol` passes all current real-socket integrations. The matrix covers fragmented v1, strict CRLF, canonical ports, v1/v2 IPv4 and IPv6, v1 `UNKNOWN`, v2 `LOCAL`, bounded TLV processing, all CRC32C modes, HTTP/1, TLS ALPN H2, missing/partial timeout, malformed/oversized/unsupported input, untrusted peers, disabled versions, and retained active policy after a Restart-only Watch candidate.
- The complete current standalone gateway library and integration suites pass across HTTP/HTTPS/H2, WebSocket, DNS/TLS, health, capacity, retry, forwarding identity, PROXY protocol, reload, and shutdown behavior. The separately isolated HTTP/1 semantics suite also passes.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and isolated-target `pnpm.cmd verify` pass. The latter covers all workspace Rust tests, contract tests, API materialization, repository checks, topology, database lifecycle, and cloud gateway validation without additional generated diff.
- Source-config, agent/workflow, repository docs, apps index, pagination, API operation/envelope, application layering, Rust backend composition, strict component-port, route-collision, SDK consumer import, identity naming, database framework, and `verify-repo` validators pass. Production gateway CORS now fails closed with exact `https://server.sdkwork.com`, derived from the canonical cloud public-host topology.
- The standalone CLI validates `etc/examples/sdkwork.webserver.config.json` as revision `3c599aba9a77de1120a92146293181c1a4d07a2c214b9063ad72b9ba29f18486`, with one listener, one virtual host, three routes, three resources, and one upstream.
- This transport requirement does not claim database lifecycle evidence; PostgreSQL verification and release gating are owned by REQ-2026-0004 and REQ-2026-0049.

## Remaining Boundary

This requirement does not implement or claim PROXY v2 authority/SSL TLV trust, TLV forwarding, AF_UNIX/UDP families, outbound `send-proxy`, optional auto-detection, hostname/dynamic trust discovery, Nginx config import/render, cluster-global identity policy, hard allocator/OOM immunity, 100,000 concurrent connections, or 24-hour soak evidence. CRC32C is bounded corruption detection and does not authenticate the sender; immediate-peer CIDR policy remains the trust authority. Those remaining capabilities are separate commercial release gates.
