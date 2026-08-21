# REQ-2026-0003 Rust Web Server Data-Plane Foundation

```yaml
id: REQ-2026-0003
title: Run verified application Web Server configuration on the Rust data plane
owner: SDKWork maintainers
status: accepted
source: platform
problem: The current application manages sites, domains, certificates, Nginx configuration, and agents, but it does not yet execute an application-owned Web Server configuration as an independent HTTP/HTTPS data plane.
goals:
  - Define a strict machine-readable sdkwork.webserver.app configuration contract.
  - Load and semantically validate configuration without requiring a database.
  - Bind real HTTP and HTTPS listeners from verified configuration.
  - Select virtual hosts and exact/prefix routes deterministically.
  - Serve static resources and bounded fixed/redirect responses.
  - Stream reverse-proxy requests and responses through verified HTTP/HTTPS upstreams.
  - Expose validate and data-plane startup operations without breaking existing management APIs.
  - Enforce finite defaults for request bodies, timeouts, connections, route counts, and configuration size.
non_goals:
  - Full arbitrary Nginx configuration compatibility in this first implementation slice.
  - HTTP/3, generic TCP/UDP proxying, forward proxy CONNECT, FastCGI, WebDAV, WASM, or WAF modules.
  - ACME automation, distributed cache, globally distributed rate limiting, or cloud rollout completion in this requirement.
  - Claiming commercial release readiness before the parent PRD release gates pass.
users:
  - Web application developers
  - Platform operators
  - Site reliability engineers
acceptance_criteria:
  - A checked-in example passes the authoritative JSON Schema and Rust semantic validation.
  - Unknown fields, duplicate ids, unresolved references, conflicting listeners, unsafe static roots, invalid domains, missing TLS files, unsupported route modes, and unbounded limits fail validation.
  - The data-plane startup path does not initialize PostgreSQL.
  - A configured HTTP listener serves exact-host and default-host fixed, redirect, static, and proxy routes.
  - A configured HTTPS listener serves TLS 1.2/1.3 with the selected certificate and HTTP/2 ALPN support.
  - Proxy request and response bodies are streamed; configured request-size and request/deadline bounds are enforced.
  - Shutdown stops accepting new traffic and drains active server tasks to a configured deadline.
  - Unit and integration tests exercise config failures, host/path precedence, static traversal rejection, proxy streaming, TLS handshake, and shutdown.
non_functional_requirements:
  security: Fail closed on invalid protocol/configuration, private-key exposure, unsafe paths, unverified upstream TLS, host confusion, and SSRF-sensitive dynamic destinations.
  privacy: Request bodies, authorization data, cookies, private paths, and key material are not logged by the data-plane foundation.
  performance: Request/proxy memory is bounded by configured buffers and body windows; no full-body collection is allowed on the proxy path.
  reliability: The data plane starts from local verified configuration and remains independent of management database availability.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo test --workspace
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - node ../sdkwork-specs/tools/check-application-layering.mjs --root .
  - node ../sdkwork-specs/tools/check-rust-backend-composition.mjs --root .
  - pnpm verify
```

Product authority: [PRD.md](../prd/PRD.md). Architecture decision: [ADR-20260715-rust-webserver-data-plane.md](../../architecture/decisions/ADR-20260715-rust-webserver-data-plane.md).

## Acceptance Evidence

Accepted on 2026-07-15 as the bounded Phase 1 foundation slice, not as commercial-release acceptance of the parent PRD.

- The checked-in configuration example is validated by Draft 2020-12 JSON Schema, strict Serde decoding, semantic validation, and immutable compilation tests.
- Real-socket integration tests prove fixed, redirect, static, default-host, HTTP proxy, HTTPS, HTTP/2 ALPN, request-body rejection, traversal/authority-confusion rejection, connection overload rejection, and finite shutdown drain behavior.
- Proxy request and response bodies remain streams; the implementation does not collect an unbounded body before forwarding.
- `cargo test --workspace`, full-workspace strict Clippy, and `pnpm verify` pass on the acceptance revision.

The parent PRD remains active. Dynamic configuration generations, multi-certificate SNI rotation, WebSocket Upgrade, Nginx import/conformance, dynamic DNS, advanced HTTP abuse controls, cache/rate limiting, cluster rollout, observability, and production load/soak/chaos evidence were outside this accepted foundation slice. Classic bounded WebSocket/WSS was subsequently delivered by REQ-2026-0031; the other listed capabilities remain governed by their later requirements or release gates.
