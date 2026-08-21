# REQ-2026-0046 Bounded PROXY v2 TLV and CRC32C

```yaml
id: REQ-2026-0046
title: Add bounded HAProxy PROXY v2 TLV framing and CRC32C policy
owner: sdkwork-webserver
status: accepted
source: nginx-proxy-protocol-commercial-readiness
problem: A trusted L4 peer can currently send structurally truncated PROXY v2 TLVs or an invalid CRC32C TLV and still reach TLS/HTTP because the bounded parser discards all bytes after the address block without validating their framing or integrity.
goals:
  - Validate every PROXY v2 TLV boundary without retaining the complete Header or allocating in proportion to its declared length.
  - Add listener-local ignore, validate-if-present, and required CRC32C policies with a compatibility-preserving ignore default.
  - Validate CRC32C over the complete v2 Header using the HAProxy wire rule and reject ambiguous duplicate or malformed CRC TLVs.
  - Preserve mandatory immediate-peer trust, finite connection admission, finite parsing timeout, and Restart-only listener policy.
  - Cover plain HTTP, HTTPS with ALPN H2, fragmentation, PROXY, LOCAL, and rejection/recovery behavior with real sockets.
non_goals:
  - Trusting or forwarding SSL, ALPN, authority, unique-id, network-namespace, or vendor-specific TLV values.
  - Cryptographic peer authentication; CRC32C detects corruption and accidental mismatch but is not a MAC or signature.
  - Optional PROXY auto-detection, outbound send-proxy, AF_UNIX, UDP/datagram, or dynamic trust-source discovery.
  - Nginx directive import/render or byte-for-byte implementation equivalence.
users:
  - operators placing the Rust Web Server behind HAProxy, Nginx stream, cloud L4 ingress, or another trusted PROXY v2 producer
  - security and platform teams requiring deterministic corrupt-Header rejection before TLS or HTTP parsing
acceptance_criteria:
  - proxyProtocol.crc32cPolicy accepts only ignore, validate-if-present, or required and defaults to ignore.
  - validate-if-present and required are rejected unless v2 is enabled; the policy does not change v1 parsing.
  - Every v2 extension is parsed as type u8, length u16, and exactly length value bytes; truncated headers and values fail closed.
  - A CRC32C TLV uses type 0x03 and length 4; malformed length and duplicate CRC TLVs fail closed under every policy.
  - validate-if-present accepts a missing CRC but rejects a mismatched CRC; required rejects a missing or mismatched CRC.
  - ignore accepts a correctly framed CRC value without comparing it and preserves the pre-existing default configuration behavior.
  - CRC calculation covers the signature, fixed header, address block, and all TLVs with the four CRC value bytes treated as zero and compares network-byte-order values.
  - PROXY and LOCAL commands apply the same TLV framing and CRC policy; TLV values never become trusted identity, request Headers, logs, or metric labels.
  - Header bytes remain bounded by maxHeaderBytes, value streaming uses fixed scratch memory, and no lock, queue, per-client map, or Header-sized allocation is introduced.
  - crc32cPolicy changes are rejected by Watch reload as Restart-only with the active listener generation retained.
non_functional_requirements:
  security: Immediate-peer CIDR trust remains the authority boundary; invalid framing, duplicate CRC, malformed CRC, and policy failure close before TLS/HTTP without reflecting attacker-controlled bytes.
  privacy: TLV types and values are not exposed in request metadata, logs, traces, or metric labels.
  performance: Parsing is O(declared Header bytes) with fixed stack buffers and one streaming CRC digest; memory is O(1) within the existing connection ceiling.
  reliability: Slow or fragmented Headers remain inside the existing timeout and connection permit; a rejected Header cannot partially enter TLS/HTTP parsing.
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
  - cargo clippy -p sdkwork-webserver-core --all-targets -- -D warnings
  - cargo clippy -p sdkwork-api-webserver-standalone-gateway --all-targets -- -D warnings
  - cargo fmt --all -- --check
  - git diff --check
  - pnpm.cmd verify
```

## Compatibility Boundary

The default `ignore` policy preserves configuration compatibility and does not compare CRC values. All policies now require well-formed TLV boundaries and an unambiguous type `0x03` CRC shape. This is a deliberate fail-closed protocol correction: malformed TLVs are not valid opaque payloads under the HAProxy PROXY v2 contract. `validate-if-present` supports mixed load-balancer fleets during rollout; `required` is the strict target once every trusted producer emits CRC32C.

CRC32C is integrity checking, not authentication. The immediate TCP peer CIDR remains the only sender authorization in this requirement. Operators requiring cryptographic sender identity must use a protected network path or mutually authenticated transport outside this Header.

## Parser And Memory Design

- The parser initializes one Castagnoli CRC digest with the already-consumed 12-byte signature and the four-byte v2 fixed Header.
- Address bytes and unknown TLV values are fed to the digest as they are read. Unknown values are consumed through one fixed scratch buffer and are never retained.
- CRC TLV metadata is fed normally, its four received value bytes are retained in one fixed array, and four zero bytes are fed into the digest as required by the protocol.
- Remaining-byte accounting rejects incomplete TLV metadata or values before any read can cross into TLS/HTTP payload bytes.
- At most one expected CRC value and one boolean presence flag are retained per accepted connection. No dynamic labels, caches, maps, queues, or attacker-sized allocations are added.

## Implementation Evidence

- The public Rust config model and root JSON Schema expose `crc32cPolicy` with strict `ignore`, `validate-if-present`, and `required` tokens. Omission defaults to `ignore`; semantic compilation rejects either checking mode when v2 is not enabled. The field is part of listener topology equality, so Watch candidates cannot change it under active sockets.
- The gateway uses the directly declared `crc` 3.x implementation with the published CRC-32/ISCSI Castagnoli algorithm. It streams the already-consumed signature, fixed Header, address bytes, TLV metadata, and unknown values through one digest; CRC value bytes are replaced with four zero bytes in the digest exactly as required by the HAProxy wire contract.
- TLV parsing maintains exact remaining-byte accounting, one three-byte metadata array, one four-byte CRC value array, one optional expected checksum, and one 256-byte value scratch buffer. It rejects incomplete metadata, over-declared values, duplicate CRC TLVs, and CRC values whose length is not four without reading into TLS/HTTP payload bytes.
- PROXY TCP4/TCP6 and LOCAL share the structural and CRC path. Unknown well-formed TLVs are accepted but discarded, and no TLV type/value becomes request identity, a forwarded Header, an access-log field, or a metric label.

## Verification Evidence

- `cargo test -p sdkwork-webserver-core --test webserver_config` passes 60/60 configuration tests, including the compatibility default, strict token rejection, and the v2 semantic dependency.
- `cargo test -p sdkwork-api-webserver-standalone-gateway --test proxy_protocol` passes 3/3 real-socket integrations. The matrix covers default-ignore mismatch compatibility, valid/missing/wrong/duplicate/malformed CRC, truncated metadata/value, unknown TLVs, fragmented HTTP, TLS ALPN H2, PROXY/LOCAL, required policy, and retained active policy after a Watch candidate.
- The complete standalone gateway suite passes 204 tests: 99 library tests and 105 integration tests across the broader HTTP/HTTPS/H2, WebSocket, DNS/TLS, health, capacity, retry, reload, and shutdown surfaces.
- Strict Clippy passes independently for `sdkwork-webserver-core` and `sdkwork-api-webserver-standalone-gateway` with all targets and `-D warnings`. `cargo fmt --all -- --check` and `git diff --check` pass.
- Isolated-target, environment-independent `pnpm.cmd verify` checks passed workspace Rust tests, contract tests, API materialization consistency, repository standards, topology, database-framework, and cloud-gateway validation. PostgreSQL lifecycle was not executed or claimed by this transport requirement; REQ-2026-0004/0049 own that evidence.

## Remaining Boundary

This requirement does not claim cryptographic Header authentication, semantic trust of any non-CRC TLV, outbound PROXY support, complete HAProxy/Nginx stream compatibility, hard allocator/OOM immunity, 100,000 concurrent connection proof, or long-duration soak evidence. Those remain separate commercial release gates.
