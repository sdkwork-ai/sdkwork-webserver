# REQ-2026-0007 Bounded HTTP Protocol Ingress

```yaml
id: REQ-2026-0007
title: Bound HTTP header parsing and HTTP/2 per-connection resources before routing
owner: SDKWork maintainers
status: accepted
source: security
problem: The data plane previously depended on broad and version-dependent Hyper defaults, including an approximately 400 KiB HTTP/1 connection read buffer and approximately 16 MiB HTTP/2 header-list limit, without an authored slow-header deadline or explicit stream/reset/buffer budgets.
goals:
  - Define bounded application configuration for HTTP/1 parser bytes, header count, and header-read deadline.
  - Define bounded HTTP/2 concurrent-stream, header-list, pending-reset, local-error-reset, and per-stream send-buffer controls.
  - Reject configurations whose concurrent HTTP/2 header or send-buffer products exceed the per-connection safety budget.
  - Apply explicit HTTP/2 flow-control windows, frame size, and non-adaptive policy instead of relying on future library defaults.
  - Fail closed on conflicting Content-Length, obsolete line folding, Transfer-Encoding plus Content-Length, and slow or oversized headers.
  - Canonicalize reverse-proxy request framing by removing inbound Content-Length and Transfer-Encoding before Reqwest emits the upstream request.
  - Treat all inbound HTTP/1 Transfer-Encoding as unsupported until a parser boundary can prove original framing without Hyper normalization; REQ-2026-0008 later replaces this temporary behavior.
  - Keep protocol parser and connection budgets restart-only under local Watch reload.
non_goals:
  - Safe Chunked and Trailer behavior, which is owned by the subsequent REQ-2026-0008 requirement.
  - Independent request-line, URI, individual header-name/value, chunk-extension, trailer, pipeline-depth, body-progress, response-write, or keep-alive-idle limits.
  - Complete HTTP/2 HPACK dynamic-table, continuation/frame flood, empty-frame flood, stream-churn, rapid-reset storm, priority, GOAWAY, or graceful-drain conformance evidence.
  - Replacing Hyper with an independently implemented HTTP parser.
  - Claiming commercial runtime-core completion without fuzzing, differential Nginx tests, load/soak, and memory evidence.
users:
  - Platform operators
  - Site reliability engineers
  - Security engineers
acceptance_criteria:
  - Schema, Serde defaults, and semantic validation bound every introduced protocol control.
  - The product of HTTP/2 concurrent streams and per-stream send-buffer bytes is at most 64 MiB per connection.
  - The product of HTTP/2 concurrent streams and header-list bytes is at most 64 MiB per connection.
  - HTTP/1 requests exceeding configured header count or parser bytes are rejected before routing; protocol-level rejection may close the connection before an HTTP response is safe.
  - Incomplete HTTP/1 headers are closed by the configured header deadline and do not consume a connection indefinitely.
  - Conflicting Content-Length and obsolete folded headers are rejected before routing.
  - Transfer-Encoding plus Content-Length is rejected; the original temporary rejection of otherwise valid Chunked input is superseded by REQ-2026-0008.
  - A real HTTP/2 TLS client observes the configured SETTINGS_MAX_CONCURRENT_STREAMS value.
  - An HTTP/2 request exceeding the configured header-list limit is rejected.
  - Rejected protocol inputs do not terminate the listener or prevent subsequent healthy requests.
  - A Watch candidate changing a protocol budget retains the active generation and requires restart.
non_functional_requirements:
  security: Ambiguous inbound framing fails closed; untrusted framing headers never cross the reverse-proxy boundary.
  privacy: Rejected header values are not logged or used as metric labels.
  performance: Parser, stream, header-list, reset, flow-control, and send-buffer values are finite and validated before listener startup.
  reliability: Protocol-limit rejection affects only the offending connection or stream where supported and leaves the listener available.
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

Accepted on 2026-07-16 for the bounded strict-ingress slice only.

- Core contract tests prove all introduced bounds and reject HTTP/2 concurrent header/send products above 64 MiB per connection.
- Raw TCP tests prove HTTP/1 Header count/parser-byte rejection, complete-header timeout closure, conflicting Content-Length rejection, obsolete-fold rejection, and fail-closed framing behavior.
- A real TLS/H2 client observes the configured `SETTINGS_MAX_CONCURRENT_STREAMS` and cannot complete an oversized Header List request.
- Reload integration proves protocol-budget changes retain the active Generation and require restart.
- Healthy requests continue after protocol-limit and malformed-input rejection.
- `pnpm verify`, full-workspace strict Clippy, and formatting checks pass on the acceptance revision.

This acceptance does not include complete HTTP/1 and HTTP/2 adversarial conformance. REQ-2026-0008 subsequently adds bounded original-wire Chunked/Trailer validation without changing the protocol budgets accepted here.

## Change Control

On 2026-07-16, REQ-2026-0008 replaced the temporary no-Transfer-Encoding behavior with a pre-Hyper original-wire Framing Guard. The Header, timeout, HTTP/2, Restart-only, and resource-budget outcomes accepted by this requirement remain unchanged.
