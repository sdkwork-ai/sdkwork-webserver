# REQ-2026-0010 HTTP/1 Connection Semantics

```yaml
id: REQ-2026-0010
title: Provide bounded HTTP/1.0 and HTTP/1.1 connection semantics with explicit Nginx differences
owner: SDKWork maintainers
status: accepted
source: reliability
problem: The data plane had bounded HTTP parsing and framing, but its Expect/Continue, HTTP/1.0, TCP half-close, Keep-Alive, and Pipeline behavior was not explicitly enforced or compared with a real Nginx reference. Hyper half-close was disabled, unsupported expectations could reach routes, and proxy requests could forward a client Expect header into a second Continue negotiation.
goals:
  - Support HTTP/1.0 default-host requests, persistent connections, and ordered pipelined responses without Chunked output.
  - Accept exactly one HTTP/1.1 Expect: 100-continue value and reject unsupported expectations with 417.
  - Return 413 for a known oversized Content-Length without first sending 100 Continue.
  - Terminate Expect at the SDKWork listener and remove it from the upstream proxy request.
  - Permit a complete request followed by TCP write-side half-close while preventing a truncated body from executing a successful route.
  - Reject Transfer-Encoding on HTTP/1.0 and HTTP/2 while retaining the HTTP/1.1 Framing Guard.
  - Record measured Nginx 1.26.2 matches, intentional security differences, and implementation constraints without claiming complete conformance.
non_goals:
  - A configurable maximum HTTP/1 Pipeline request count; current task admission remains serialized and buffered bytes remain bounded by the parser buffer.
  - A custom HTTP/1 encoder solely to reproduce Nginx's HTTP/1.1 response status line for an HTTP/1.0 peer.
  - Complete request-line, URI, individual header-name/value, body-progress, response-write, or Keep-Alive timeout controls.
  - HTTP/2 informational-response support, WebSocket Upgrade, SSE, CONNECT, or full-duplex tunnel behavior.
  - A complete Nginx differential corpus, fuzz corpus, slow-client suite, load test, or commercial runtime-core acceptance.
users:
  - Web application developers
  - Platform operators
  - Protocol and security engineers
acceptance_criteria:
  - A raw HTTP/1.1 client receives 100 Continue only after a valid Expect request and can then complete the request successfully.
  - A known oversized Content-Length returns 413 without a preceding 100 response, while unsupported, duplicate, malformed, HTTP/1.0, and HTTP/2 expectations fail closed.
  - A reverse-proxy route sends the final request Body upstream without forwarding the client Expect header.
  - An HTTP/1.0 request without Host selects the configured listener default virtual host, emits a finite non-Chunked response, and supports ordered Keep-Alive Pipeline responses.
  - HTTP/1.0 Transfer-Encoding is rejected before a successful route can execute.
  - A complete request survives TCP write-side half-close, while a declared but truncated Body never receives a 2xx response.
  - A raw TLS client negotiated through ALPN receives the same valid HTTP/1.1 100 Continue sequence.
  - A pinned local Nginx 1.26.2 probe records comparison results and every observed difference is classified rather than hidden.
non_functional_requirements:
  security: Unsupported expectations, non-HTTP/1.1 Transfer-Encoding, and truncated Bodies fail closed. The gateway does not create a client-to-gateway-to-upstream Continue wait chain.
  privacy: Request Body and Expect values are neither retained nor added to logs.
  performance: No Body collection, response queue, or Body-sized allocation is introduced. Hyper service readiness serializes Pipeline dispatch and the configured parser buffer bounds unread Pipeline bytes.
  reliability: TCP read EOF does not suppress a valid response after a complete request; malformed or truncated input affects only its connection.
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
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test http1_connection_semantics -- --nocapture
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - pnpm verify
```

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md). Compatibility authority: [PRD-nginx-compatibility.md](../prd/PRD-nginx-compatibility.md). Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Reference Comparison

The reference probe used the installed `nginx/1.26.2` Windows build with OpenSSL 3.0.14, TLS SNI, HTTP/2, and debug support. It is engineering evidence for this slice, not the versioned cross-platform corpus required for final Nginx behavioral acceptance.

| Case | Nginx 1.26.2 | SDKWork profile | Classification |
| --- | --- | --- | --- |
| HTTP/1.0 default server without Host | Serves the default server | Serves the configured listener default virtual host | Semantic match |
| HTTP/1.0 Keep-Alive Pipeline | Two ordered responses | Two ordered responses | Semantic match |
| HTTP/1.0 Transfer-Encoding | `400` | `400` | Behavioral match |
| HTTP/1.1 `Expect: 100-continue` | `100`, then final `200` | `100`, then final response | Behavioral match |
| Unknown Expectation | Ignores it and serves the route | Returns `417` | Intentional RFC/security hardening difference |
| Complete Body followed by write half-close | Serves the route | Serves the route | Semantic match |
| Truncated declared Body on an immediate `return` route | Can serve the route without consuming the Body | Drains/counts before non-proxy execution and never returns 2xx | Intentional security and Body-budget difference |
| HTTP/1.0 response status-line version | Emits HTTP/1.1 | Hyper 1.10.1 enforces HTTP/1.0 for an HTTP/1.0 peer | Known behavioral difference; custom encoder deferred |

The compatibility grade for this slice is therefore not complete Behavioral compatibility. Security differences are retained under [PRD-nginx-compatibility.md](../prd/PRD-nginx-compatibility.md) section 8.1, and the HTTP/1.0 status-line difference remains visible until a reviewed transport decision changes it.

## Acceptance Evidence

Accepted on 2026-07-16 for the declared bounded HTTP/1 connection-semantics slice.

- Three dedicated raw-Socket tests prove valid Continue sequencing, early `413` without `100`, strict `417`, HTTP/1.0 default-host and ordered Keep-Alive Pipeline behavior, Transfer-Encoding rejection, and complete/truncated write-half-close behavior.
- Fifteen data-plane integration tests include a raw Rustls/ALPN HTTP/1.1 Continue exchange and a real Axum upstream that proves the gateway removes `Expect` after completing the client negotiation.
- Ten Gateway unit tests retain the original-wire Chunked guard, bounded Trailer/Body behavior, and TLS certificate validation coverage.
- A real `nginx/1.26.2` probe with the installed debug/TLS/HTTP2 build produced the comparison matrix above. It identified semantic matches, two intentional fail-closed differences, and one Hyper response-version constraint rather than producing a false complete-compatibility claim.
- `pnpm verify`, strict Gateway and full-Workspace Clippy, `cargo fmt -- --check`, repository documentation validation, example configuration compilation, pagination, API response-envelope, app SDK consumer-import checks, and `git diff --check` passed.

The unrelated API operation-pattern validator still requires human review for the existing public `GET /backend/v3/api/agent/sync` operation id. PostgreSQL lifecycle execution also remains ignored without an explicitly configured disposable database. Neither is evidence for, or changed by, this HTTP/1 runtime requirement.

This acceptance did not establish configurable Pipeline depth; that focused boundary was subsequently delivered by accepted [REQ-2026-0019](REQ-2026-0019-bounded-http1-pipeline-depth.md). Full HTTP/1 parser/fuzz/differential conformance, HTTP/2 informational responses, WebSocket/SSE, load/soak evidence, complete Nginx Behavioral compatibility, and commercial runtime-core acceptance remain separate gates.
