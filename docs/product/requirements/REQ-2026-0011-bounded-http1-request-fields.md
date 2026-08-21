# REQ-2026-0011 Bounded HTTP/1 Request Fields

```yaml
id: REQ-2026-0011
title: Bound HTTP/1 request-line and individual field allocations before Hyper parsing
owner: SDKWork maintainers
status: accepted
source: security
problem: HTTP/1 total header bytes and field count were bounded, but the original-wire guard could still grow one request line or field line to the total header ceiling before rejecting it. Method, request-target, header-name, and header-value budgets were implicit Hyper behavior rather than SDKWork configuration.
goals:
  - Add finite maxRequestLineBytes, maxRequestMethodBytes, maxRequestTargetBytes, maxHeaderNameBytes, and maxHeaderValueBytes configuration.
  - Validate request-line grammar, HTTP/1.0 or HTTP/1.1 version, method token, visible-ASCII request target, header token, and field-value control bytes before Hyper normalization.
  - Apply individual name/value budgets to request Headers and Chunked Trailer fields.
  - Reject an oversized line before the incremental line buffer grows beyond its configured effective ceiling.
  - Treat every new parser budget as restart-only listener topology.
  - Preserve streaming Bodies, Keep-Alive/Pipeline state reset, TLS decryption ordering, and HTTP/2 ALPN bypass.
non_goals:
  - Replacing Hyper's complete HTTP semantic parser or emitting an HTTP status after every wire-level rejection.
  - Percent-decoding, URI normalization, IDNA, route matching, or application-specific query limits.
  - HTTP/2 HPACK field-size controls beyond the existing total Header List budget.
  - Pipeline depth, body-progress, response-write, Keep-Alive idle, fuzz, full Nginx differential, load/soak, or commercial acceptance.
users:
  - Platform operators
  - Site reliability engineers
  - Security engineers
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and runtime topology expose all five finite budgets.
  - Request lines exceeding maxRequestLineBytes are rejected before routing.
  - Methods exceeding maxRequestMethodBytes or containing a non-token byte are rejected before routing.
  - Request targets exceeding maxRequestTargetBytes, containing a control/non-ASCII byte, or using malformed request-line spacing are rejected before routing.
  - Header and Trailer names/values exceeding individual budgets or containing invalid control bytes are rejected before routing or forwarding.
  - Fragmented valid requests and a following pipelined request retain correct parser state.
  - Plain and TLS raw-Socket tests prove oversized inputs do not execute a successful route and healthy requests still succeed.
  - A Watch candidate changing any new budget retains the active generation and reports restart required.
non_functional_requirements:
  security: Invalid raw request metadata fails closed before Hyper normalization and is never reflected into logs or diagnostics.
  privacy: Request lines and Header/Trailer values are not logged or retained after incremental validation.
  performance: Per-connection line memory is bounded by the minimum applicable total and individual budgets; Body data remains uncollected.
  reliability: Rejection affects only the offending connection and leaves the listener and active reload generation available.
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

Accepted on 2026-07-16 for the declared HTTP/1 original-wire request-field budget slice.

- The root JSON Schema, Rust defaults, semantic validation, example configuration, Core README, and runtime Restart-only topology expose all five finite budgets.
- Eighteen Core configuration tests include cross-field rejection when target/value ceilings exceed their enclosing line/Header ceilings.
- Eleven Gateway unit tests include fragmented valid Pipeline parsing and rejection for oversized/malformed request lines, methods, targets, Header names/values, control bytes, and Trailer values.
- Fifteen data-plane integration tests include real plain and Rustls HTTP/1 connections that reject individual field overflow, keep the listener healthy, and retain the active generation when a Watch candidate changes `maxRequestTargetBytes`.
- The line buffer starts at no more than the configured request-line ceiling and cannot grow past the smaller applicable total and individual field-line ceiling. Request Body bytes remain streaming and are never copied into this parser.
- `pnpm verify`, full-Workspace strict Clippy, `cargo fmt -- --check`, example configuration compilation, repository documentation validation, topology validation, database framework validation, and `git diff --check` passed.

PostgreSQL lifecycle execution remains explicitly ignored without a disposable database URL. This acceptance does not establish query-component/normalized-URI budgets, Pipeline depth, complete timeout coverage, HTTP/2 HPACK abuse defense, parser fuzz/differential conformance, load/soak memory evidence, or commercial runtime-core acceptance.
