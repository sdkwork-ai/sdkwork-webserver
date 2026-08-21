# REQ-2026-0008 Safe HTTP/1 Chunked Framing

```yaml
id: REQ-2026-0008
title: Validate original HTTP/1 framing before Hyper normalization and safely serve Chunked bodies
owner: SDKWork maintainers
status: accepted
source: security
problem: Hyper intentionally normalizes a request whose Transfer-Encoding precedes Content-Length by dropping the latter, so an application-layer guard cannot prove whether the original wire input was ambiguous. The temporary safe profile rejected every Transfer-Encoding and therefore could not provide complete Web Server request-body behavior.
goals:
  - Inspect decrypted HTTP/1 wire bytes before Hyper exposes or normalizes them.
  - Reject Transfer-Encoding plus Content-Length in either order and reject every duplicate Content-Length, including identical values.
  - Support exactly one chunked Transfer-Encoding with bounded Chunk Size lines, extensions, body total, Trailer count, and Trailer bytes.
  - Reject malformed CRLF, Chunk Size, Chunk Data terminators, obsolete folding, duplicate Transfer-Encoding, unsupported transfer codings, and forbidden Trailer fields.
  - Apply the guard to every request on plain and TLS Keep-Alive/Pipeline connections while bypassing HTTP/2 selected by ALPN.
  - Enforce maxRequestBodyBytes for fixed and Chunked HTTP/1 bodies and for HTTP/2 bodies without Content-Length across every route action.
  - Preserve streaming proxy behavior and canonicalize upstream framing through Reqwest.
  - Bound the configured global HTTP/1 connection/header window product to 1 GiB.
non_goals:
  - Transfer codings other than exactly chunked.
  - Forwarding request Trailers to upstreams or supporting response Trailers; the current proxy data stream forwards Body Data frames only.
  - Replacing Hyper's complete request-line, URI, method, header-value, Expect, upgrade, response, or connection semantic parser.
  - Returning a synthetic HTTP status after every wire-level framing error; ambiguous connections may be closed before Hyper can safely construct a response.
  - Full HTTP/1 differential conformance, fuzzing, malformed corpus coverage, HTTP/2 abuse completion, or commercial runtime-core acceptance.
users:
  - Web application developers
  - Platform operators
  - Security engineers
acceptance_criteria:
  - A bounded incremental state machine accepts fragmented Header, fixed Body, Chunk Size, Chunk Data, Trailer, and next-request bytes without collecting a request body.
  - Valid Chunked bodies with extensions and bounded Trailers work over plain HTTP and TLS HTTP/1.
  - A valid Chunked body reaches a real streaming reverse proxy with exact Body Data bytes.
  - Transfer-Encoding plus Content-Length is rejected in both header orders before routing.
  - Identical and conflicting duplicate Content-Length values are rejected before routing.
  - Chunk Body, Chunk Size line, Trailer count, Trailer bytes, forbidden Trailer field, CRLF, and unsupported transfer-coding violations close or reject the offending connection.
  - Fixed Content-Length over maxRequestBodyBytes returns 413 for every route action.
  - Chunked Body over maxRequestBodyBytes is terminated before the route can report success.
  - HTTP/2 Body without Content-Length over maxRequestBodyBytes returns 413 for a non-proxy action.
  - Pipelined requests reset framing state and each request is independently validated.
  - TLS ALPN h2 remains functional and is not inspected as HTTP/1 wire data.
  - Reverse proxy requests remove inbound Content-Length and Transfer-Encoding before Reqwest emits new upstream framing.
non_functional_requirements:
  security: Original framing ambiguity fails closed before Hyper normalization; error text never contains request header values or body bytes.
  privacy: Wire data is neither logged nor retained after incremental validation.
  performance: Parsing is linear in bytes, line memory is configuration-bounded, Body memory is streaming, and no request-sized collection is introduced.
  reliability: A malformed connection does not terminate the listener and subsequent healthy requests continue to succeed.
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
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - pnpm verify
```

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md). Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Acceptance Evidence

Accepted on 2026-07-16 for original-wire HTTP/1 message-boundary validation and bounded Chunked input only.

- State-machine unit tests feed Header, Chunk Size, Chunk Data, Trailer, and a pipelined next request in three-byte fragments without collecting Body data.
- Unit and real-socket tests reject TE/CL in both orders, identical/conflicting duplicate Content-Length, malformed folding, oversized Chunk lines, excessive/forbidden Trailers, and fixed/Chunked bodies beyond the applicable ceiling.
- Real plain HTTP proves valid Chunk Extensions and Trailers, Pipelined state reset, exact streaming Proxy Body bytes, and listener health after malformed connections.
- Real TLS HTTP/1 proves the Guard executes after decryption; real TLS HTTP/2 proves ALPN `h2` bypass remains functional.
- Fixed Content-Length over the active application limit returns `413`; Chunked Proxy/non-Proxy streams and HTTP/2 bodies without Content-Length are counted without Body-sized collection.
- Watch integration proves `maxRequestBodyBytes` remains an atomically reloadable Generation policy while parser topology limits remain Restart-only.
- The example config, `pnpm verify`, full-workspace strict Clippy, formatting, repository documentation, topology, and database framework checks pass.

This acceptance does not include request-Trailer forwarding, response Trailers, transfer codings other than Chunked, or complete HTTP parser/Nginx conformance.
