# REQ-2026-0021 Proxy Early-Response Request Lifecycle

```yaml
id: REQ-2026-0021
title: Cancel incomplete proxy uploads safely after an upstream early response
owner: SDKWork maintainers
status: accepted
source: reliability
problem: A proxy upstream can return a final response before the client request Body ends. The gateway previously returned that response without an explicit cancellation and connection-ownership policy, leaving request consumption, HTTP/1 reuse, HTTP/2 isolation, timeout interaction, and upstream pool reuse dependent on transport internals.
goals:
  - Detect when a successful upstream response Future completes before the streamed client request Body reaches end-of-stream.
  - Stop polling the client request Body as soon as final upstream response headers arrive, then cancel the upstream producer when the complete response has been handed to Hyper or the downstream cancels it.
  - Close the downstream HTTP/1 connection after forwarding the complete upstream response so unread request framing cannot be reused as another request.
  - Isolate HTTP/2 cancellation to the affected Stream and preserve the connection for other Streams.
  - Prevent reuse of an upstream HTTP/1 connection whose request upload was canceled before end-of-stream.
  - Preserve the upstream final status, headers, Body, and Trailer validation for early 302, 401, 413, 417, other 4xx, and 5xx responses.
  - Release request admission through the existing downstream response Body lifetime, including cancellation and response completion.
non_goals:
  - Buffering complete request Bodies before proxying or enabling an Nginx proxy_request_buffering equivalent.
  - Background draining of slow or infinite client uploads.
  - Retrying a request after an upstream early response.
  - WebSocket, SSE, gRPC, CONNECT, or protocol Upgrade lifecycle changes.
  - Dynamic DNS, active health checks, weighted balancing, circuit breaking, or adaptive memory admission.
acceptance_criteria:
  - One shared atomic lifecycle state distinguishes active, completed, paused, and canceled proxy request Bodies.
  - Receiving any final upstream response pauses an incomplete Body producer and wakes a pending producer poll without reading another client Frame.
  - Complete response handoff or downstream cancellation releases any deferred client Body and wakes the paused upstream producer with a local cancellation.
  - Cancellation is not classified as malformed framing, Body-size overflow, request Body timeout, upstream timeout, or generic 502.
  - Plain HTTP/1 and TLS HTTP/1 real-Socket tests receive the upstream response before upload completion, observe Connection close, and cannot reuse the client connection.
  - A real TLS/H2 test receives the upstream response before END_STREAM, observes Stream reset after the complete response, and successfully reuses the same H2 connection for another request.
  - A real upstream Socket test proves the canceled partial-upload HTTP/1 connection is not reused from the Reqwest pool.
  - Early response wins before requestBodyStartTimeoutMs or requestBodyIdleTimeoutMs and no later timeout corrupts the committed response.
  - A one-request admission limit recovers after the early response Body completes.
  - Unit tests prove a pending producer is woken by cancellation and a completed producer is not canceled.
  - No request-sized allocation, Body collection, detached task, drain queue, per-Frame mutex, or per-Frame copy is introduced.
  - Full repository verification passes.
non_functional_requirements:
  security: Unread HTTP/1 request bytes never become a subsequent request; fixed responses disclose no client Body or upstream detail.
  performance: Cancellation adds one shared atomic state, one AtomicWaker, and one terminal-only deferred-Body ownership slot per active proxy request; Data and Trailer Frames remain zero-copy and backpressured.
  memory: The deferred slot retains only the existing Body object, never request content; HTTP/1 does not drain beyond Hyper's already-buffered cheap-drain path, while HTTP/2 stops releasing request Stream flow-control capacity and bounds unread data by finite protocol windows.
  reliability: Upstream response delivery, request cancellation, downstream connection ownership, response Body admission, and timeout classification have one explicit lifecycle.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-api-webserver-standalone-gateway
    - tests/nginx/proxy-early-response
verification:
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm verify
```

## Protocol Ownership

The proxy request Body remains a backpressured Frame stream while the upstream request is active. A shared atomic lifecycle is marked complete only when the producer reaches end-of-stream. If Reqwest returns final upstream response headers first, the handler atomically changes the lifecycle to paused and wakes the Body poll owned by Hyper's upstream connection driver. A paused poll returns `Pending` without reading another client Frame.

Hyper's upstream HTTP/1 driver can drop its request Body before returning an early response Future. The request wrapper therefore transfers the existing inner Body into one terminal-only deferred ownership slot when an incomplete upstream-side Drop occurs. The slot uses a single non-nested `std::sync::Mutex` only for ownership transfer and release, never during Frame polling and never across `.await`. Destructors run outside the lock. The response Body wrapper owns the same control until Hyper has processed the complete response or the downstream cancels it; wrapper Drop changes the lifecycle to canceled, releases the deferred Body, and wakes any paused producer. This ordering prevents a request-side H2 reset from truncating the response while introducing no Body buffer or task.

For downstream HTTP/1, canceling the Body receiver invokes Hyper's bounded cheap-drain behavior: it consumes only request bytes already available to the parser and otherwise closes the read side. The forwarded final response carries `Connection: close`; the gateway never tries to preserve reuse by waiting for a slow or infinite upload.

For downstream HTTP/2, pausing and later dropping the request receiver stops releasing Stream flow-control capacity. H2 retains connection ownership, completes the response, and sends `RST_STREAM(NO_ERROR)` when the receive side is still streaming. Other Streams and the connection remain usable. The fixed 65,535-byte initial Stream and connection windows keep unread input finite. Hyper treats the post-response `NO_ERROR` reset as a clean request-upload stop; a production-stack Reqwest/H2 test proves the complete upstream response Body remains readable.

An upstream HTTP/1 request Body error makes that connection ineligible for pool reuse. The downstream response already received from the upstream remains authoritative and continues through the existing bounded response Body and Trailer guards.

## Nginx Comparison Scope

The comparison fixture uses `proxy_request_buffering off` because SDKWork streams requests rather than pre-buffering them. Pinned Windows Nginx 1.26.2 forwarded the complete `401` response Body, closed downstream TCP without emitting an explicit `Connection: close`, closed the upstream connection, and had forwarded the four already-available `seed` bytes before the response. SDKWork reaches the same non-reuse result and additionally emits `Connection: close` on HTTP/1 for explicit framing ownership. This is differential evidence for this lifecycle only, not a claim of full Nginx proxy-module compatibility.

## Current Evidence

- Unit tests prove cancellation wake-up, pause behavior, completed-Body immunity, and deferred ownership release without Body collection.
- Plain HTTP/1 real-Socket tests cover `302`, `401`, `413`, `417`, and `500`, explicit close, five distinct upstream connections, timeout separation, and one-permit recovery.
- TLS HTTP/1 proves the same close and recovery behavior after decryption.
- Raw TLS/H2 proves `RST_STREAM(NO_ERROR)`, timeout separation, and same-connection Stream recovery; Hyper/Reqwest TLS/H2 separately proves the complete `early-401` response Body remains readable.
- The Nginx fixture passes `nginx -t` and the repeatable probe records the pinned comparison above.
- Gateway tests pass 35 unit tests, 43 data-plane integration tests, and 4 raw HTTP/1 connection tests.
- Strict full-workspace Clippy, formatting, and `pnpm verify` pass, including workspace tests, contract tests, materialization, repository/docs/scripts/agent standards, topology, database validation, and cloud gateway validation.

## Acceptance

Accepted on 2026-07-16 for bounded reverse-proxy request ownership when an upstream returns before client upload completion. PostgreSQL lifecycle execution remains ignored without `SDKWORK_DATABASE_TEST_POSTGRES_URL`. The backend OpenAPI authority currently contains apparent text-encoding corruption and an unreviewed public operation rename from `agent.sync` to `agent.retrieve`; the operation-pattern checker passes against that current file, but API/SDK commercial readiness remains blocked on required human review. Dynamic DNS and rebinding defense, upstream TLS policy, retries/health/balancing/circuit breaking, full gRPC/WebSocket/SSE, adaptive memory pressure, load/soak evidence, and commercial release readiness remain separate requirements.
