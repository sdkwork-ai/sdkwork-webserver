# REQ-2026-0009 Bounded Proxy Trailer Fidelity

```yaml
id: REQ-2026-0009
title: Preserve and bound HTTP request and response Trailer frames across reverse-proxy hops
owner: SDKWork maintainers
status: accepted
source: reliability
problem: The reverse proxy converted HTTP bodies into data-only streams. That discarded request and response Trailer frames, prevented complete streaming HTTP semantics, and made HTTP/1 and HTTP/2 behavior diverge from the declared Web Server profile.
goals:
  - Preserve Body Data and Trailer frames without collecting request or response bodies.
  - Forward valid declared HTTP/1 Trailers and HTTP/2 trailing HEADERS through the Reqwest/Hyper proxy path.
  - Apply maxTrailerBytes and maxTrailers to request declarations, response declarations, request Trailer frames, and response Trailer frames.
  - Reject forbidden framing, routing, connection-control, and declaration fields before forwarding them.
  - Regenerate TE: trailers toward upstreams because TE is hop-specific while retaining validated Trailer declarations.
  - Preserve request-body byte accounting and its 413 behavior while adding frame fidelity.
non_goals:
  - Buffering an entire body to discover undeclared Trailer names or synthesize a late HTTP/1 Trailer declaration.
  - Full gRPC proxy acceptance, deadline propagation, status mapping, health checks, or retry behavior.
  - HTTP/1 clients receiving response Trailers without advertising TE: trailers; Hyper follows the HTTP/1 recipient capability signal.
  - Forwarding forbidden or over-budget Trailer fields after response headers have been committed; the affected stream is terminated instead.
  - Completing HTTP/1.0, Expect/Continue, Upgrade, HTTP/2 abuse, or Nginx differential conformance.
users:
  - Web application developers
  - Platform operators
  - Protocol and security engineers
acceptance_criteria:
  - A valid HTTP/1 Chunked request with a Trailer declaration reaches a real upstream with its Data and Trailer frames intact.
  - A real upstream HTTP/1 response Trailer reaches an HTTP/1 client that advertises TE: trailers.
  - A TLS HTTP/2 client can send request trailing HEADERS through an HTTP/1 upstream and receive the upstream response Trailer as HTTP/2 trailing HEADERS.
  - Request Body Data remains incrementally counted and returns 413 when the current generation limit is exceeded.
  - Invalid or duplicate Trailer declarations, forbidden fields, and over-budget request Trailer collections fail closed before they are forwarded.
  - Invalid or over-budget upstream Trailer declarations return 502 before downstream response commitment.
  - Invalid or over-budget upstream actual Trailer frames are not forwarded and terminate the downstream body stream.
  - Unsupported TE tokens are rejected; only TE: trailers is accepted from clients and regenerated toward upstreams.
non_functional_requirements:
  security: Framing and hop-specific headers are not trusted from either peer; declarations and actual Trailer maps use the same forbidden-field and finite-budget policy.
  privacy: Trailer values are neither logged nor retained beyond the active frame.
  performance: Body transfer remains incremental with no body-sized allocation; state is limited to counters, a finite header-name set, and the current bounded Trailer map supplied by Hyper.
  reliability: Trailer errors affect only the request or stream that supplied them; listener health, reload generations, connection admission, and shutdown remain independent.
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
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - pnpm verify
```

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md). Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Acceptance Evidence

Accepted on 2026-07-16 for the declared bounded HTTP/1 and HTTP/2 reverse-proxy Trailer profile.

- `GuardedProxyBody` polls `http_body::Frame` values directly. Request and response Data frames remain streaming; Trailer frames are validated and forwarded without `collect`, `to_bytes`, or data-only conversion.
- Real HTTP/1 tests prove a Chunked request Trailer reaches an Axum upstream and a real upstream response Trailer reaches a client that advertises `TE: trailers`.
- Real TLS/ALPN HTTP/2 tests prove request trailing HEADERS traverse the HTTP/1 upstream hop and return as response trailing HEADERS.
- Unit and integration tests reject forbidden/duplicate declarations, unsupported TE tokens, request Trailer count overflow, upstream declaration overflow, and actual Trailer-frame overflow.
- The existing Body-limit test continues to prove `413` without request-body collection.

This acceptance does not establish full gRPC, undeclared cross-protocol Trailer synthesis, HTTP/1.0/Expect/Upgrade conformance, HTTP/2 abuse completion, or commercial runtime-core acceptance.
