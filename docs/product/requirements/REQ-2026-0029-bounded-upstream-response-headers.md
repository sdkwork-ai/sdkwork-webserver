# REQ-2026-0029 Bounded Upstream Response Headers

```yaml
id: REQ-2026-0029
title: Bound upstream HTTP/1 response parsing and HTTP/2 Header Lists before proxy forwarding
owner: sdkwork-webserver
status: accepted
source: security-reliability
problem: Client request headers are bounded before allocation, but upstream response headers currently rely on Hyper defaults. A malicious or faulty origin can therefore consume substantially more per-connection memory than the Web Server configuration declares, and HTTP/1 and HTTP/2 do not share one explicit application contract.
goals:
  - Add finite per-upstream response Header bytes and field-count controls with safe defaults.
  - Apply parser-level limits before unbounded HTTP/1 growth and during HTTP/2 Header List decoding.
  - Apply one protocol-independent exact post-parse field budget before any upstream response is forwarded.
  - Preserve streaming Body and Trailer behavior without collecting response content.
  - Classify an oversized upstream response as a target failure and return a bounded local 502 response.
non_goals:
  - Claiming directive-level compatibility with Nginx proxy_buffer_size, proxy_buffers, or proxy_busy_buffers_size.
  - Adding response Body buffering, caching, compression, retry, or response-header rewriting.
  - Configuring HPACK dynamic-table size, which is not exposed by the selected Hyper client builder.
users:
  - platform operators
  - site reliability engineers
  - security engineers
acceptance_criteria:
  - upstreams[].maxResponseHeaderBytes defaults to 65536, accepts 8192..1048576, and rejects aliases or unknown fields.
  - upstreams[].maxResponseHeaders defaults to 100 and accepts 1..1024.
  - Hyper HTTP/1 max buffer size is set from maxResponseHeaderBytes and max header count is set without forcing heap allocation when the configured value is the Hyper default.
  - Hyper HTTP/2 max Header List size is set from maxResponseHeaderBytes.
  - Parsed Header fields are counted as name bytes plus value bytes plus wire separators with checked arithmetic before proxy response construction.
  - A count or byte overflow drops the upstream response without polling its Body and returns local 502 Bad Gateway.
  - Oversized response headers count as target failure for passive health and as failed active-health observations, while local connection-capacity saturation remains unchanged.
  - Real HTTP/1 tests cover exact-budget success, count rejection, byte rejection, no Body forwarding, connection recovery, and bounded memory behavior.
  - Real HTTPS/H2 tests cover Header List rejection and preservation of a healthy connection/Stream boundary where the protocol permits it.
  - Watch builds candidate clients with the new immutable budgets before publication; invalid candidates retain the active generation.
non_functional_requirements:
  security: No upstream header name or value is included in client errors or logs.
  privacy: The limiter stores only finite numeric budgets and does not retain response values beyond Hyper's response object lifetime.
  performance: The post-parse check is linear in the already bounded Header field count and allocates no collection or copied Header value.
  reliability: Header rejection never polls or buffers the response Body and releases all request/connection ownership through existing drop paths.
affected_surfaces:
  - config
  - runtime
  - proxy
  - security
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

The Hyper legacy client exposes `http1_max_buf_size`, `http1_max_headers`, and `http2_max_header_list_size`. These parser controls prevent the selected transport from using its broader defaults, but they are not alone a stable SDKWork contract because protocol accounting differs. The client therefore performs a second allocation-free check on the parsed `HeaderMap` before returning the response to the proxy adapter.

The byte budget counts every materialized field occurrence as header-name bytes plus header-value bytes plus four bytes for `": "` and CRLF, plus the terminating CRLF. This is deterministic across HTTP/1 and HTTP/2 after decoding. It intentionally does not model compressed HPACK bytes or the HTTP/1 status line. Parser-level controls remain the earlier defense against encoded input growth.

An oversized Header Block is an upstream observation, unlike local request or connection admission saturation. It therefore participates in the existing target failure policy and is exposed to clients only as the generic bounded `502 upstream failed` response.

## Architecture Review

The change remains inside the existing configuration compiler and standalone upstream adapter. It adds no API, SDK, database, process, or cross-repository protocol. No ADR or human review is required unless later work attempts Nginx directive-level buffer compatibility or changes public management APIs.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- Core configuration: 8 unit tests and 49 integration/configuration tests passed. Defaults, exact lower/upper bounds, out-of-range values, and forbidden aliases are covered.
- Standalone gateway: 54 library tests, 55 data-plane integration tests, 4 raw HTTP/1 connection tests, 1 resource-pressure test, 4 active-health tests, 5 physical-connection tests, and 4 response-Header tests passed, for 127 tests total.
- Response-Header unit evidence covers exact byte accounting, repeated field occurrences, count rejection, byte rejection, and checked-arithmetic overflow.
- Real HTTP/1 evidence covers accepted bounded responses, count and just-over-byte rejection, generic `502`, no rejected Header/Body disclosure, passive ejection, and recovery.
- Real HTTPS/H2 evidence covers post-decode field-count rejection, Header List rejection, absence of rejected Header disclosure, and a subsequent healthy request after each failure.
- Active-health evidence proves an oversized observation removes a target and a later bounded observation restores it. Watch evidence proves valid budget changes construct a new client and an invalid candidate retains the active generation.
- Full-workspace `cargo clippy --workspace --all-targets -- -D warnings` passed with the isolated target directory.
- `pnpm.cmd verify` passed the environment-independent workspace, contract, API materialization, repository, topology, database-framework, and cloud-gateway checks. PostgreSQL lifecycle was not executed in this requirement's acceptance run; REQ-2026-0004 and REQ-2026-0049 own that evidence.
- Pagination, API operation patterns, API response envelope, app SDK consumer imports, application layering, Rust backend composition, `cargo fmt --all -- --check`, and `git diff --check` passed.
