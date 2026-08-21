# REQ-2026-0012 Bounded HTTP/2 Abuse And Drain

```yaml
id: REQ-2026-0012
title: Bound HTTP/2 frame churn and header blocks while proving graceful GOAWAY drain
owner: SDKWork maintainers
status: accepted
source: security
problem: HTTP/2 had finite concurrent-stream, decompressed Header List, reset-state, flow-control, and send-buffer settings, but no SDKWork limit for per-connection frame/new-stream/RST_STREAM churn or encoded CONTINUATION blocks. Graceful shutdown called Hyper correctly but lacked real GOAWAY and in-flight Stream evidence.
goals:
  - Inspect decrypted HTTP/2 frame metadata after ALPN and before Hyper without collecting frame payloads.
  - Limit total frames, newly opened client streams, and RST_STREAM frames per fixed per-connection time window.
  - Limit encoded Header Block bytes and CONTINUATION frame count before H2 accumulates or decodes an unbounded block.
  - Expose and validate SETTINGS_MAX_FRAME_SIZE and SETTINGS_MAX_HEADER_LIST_SIZE through the supported Hyper server builder surface.
  - Record the H2 dependency's finite HPACK dynamic-table default without advertising a configuration control that the runtime cannot wire.
  - Preserve existing H2 pending-accept reset and local-error reset thresholds that issue ENHANCE_YOUR_CALM GOAWAY.
  - Prove graceful shutdown sends GOAWAY, stops accepting new Streams, allows an in-flight Stream to finish within drainTimeoutMs, and terminates at the finite deadline.
  - Keep every HTTP/2 parser/abuse control restart-only under Watch reload.
non_goals:
  - Full HPACK Huffman CPU accounting, every malformed frame combination, priority-tree behavior, server push, or extended CONNECT.
  - Per-IP/distributed attack aggregation, global request rate limiting, WAF policy, or DDoS edge mitigation.
  - Cleartext h2c; the current foundation profile requires TLS and ALPN for HTTP/2.
  - Replacing H2/Hyper framing, HPACK, flow-control, error-code, or GOAWAY implementations.
  - Configuring SETTINGS_HEADER_TABLE_SIZE; Hyper 1.10.1 and Hyper-util 0.1.20 do not expose this H2 server builder control.
  - Complete fuzz, RFC conformance, multi-core load, 100k connection, 24-hour soak, or commercial acceptance.
users:
  - Platform operators
  - Site reliability engineers
  - Security engineers
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and restart-only topology expose every new finite HTTP/2 control.
  - A fragmented valid client preface and frame sequence passes the incremental guard without payload collection.
  - Too many frames, new Streams, or RST_STREAM frames in one configured window close only the offending connection.
  - Oversized encoded Header Blocks, excessive CONTINUATION frames, cross-stream CONTINUATION, and non-CONTINUATION interleaving fail closed.
  - The server SETTINGS frame advertises configured concurrent-Stream, maximum-Frame, and decoded Header List sizes.
  - Existing maxPendingAcceptResetStreams and maxLocalErrorResetStreams remain finite and wired to H2 GOAWAY behavior.
  - After an abusive connection closes, a new healthy HTTP/2 connection continues to receive a successful response.
  - During graceful shutdown an in-flight Stream can complete, a new Stream is refused after GOAWAY, and the server exits within drainTimeoutMs.
  - A Watch candidate changing any new HTTP/2 control retains the active generation and reports restart required.
non_functional_requirements:
  security: Frame metadata and reset churn fail closed per connection; header bytes and request values are not logged or reflected.
  privacy: The guard retains only counters, frame headers, stream ids, and encoded byte totals; it never retains Header values or Body payloads.
  performance: Parsing is incremental and constant-memory per connection; rate counters use one fixed window and no per-Stream collection.
  reliability: Abuse isolation and graceful GOAWAY preserve listener health and bounded shutdown behavior for unaffected connections.
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

## Configuration Contract

| JSON field | Default | Accepted range | Runtime effect |
| --- | ---: | ---: | --- |
| `http2MaxFrameBytes` | 16,384 | 16,384..16,777,215 | Wire Guard input ceiling and advertised `SETTINGS_MAX_FRAME_SIZE`. |
| `http2AbuseWindowMs` | 1,000 | 100..60,000 | Fixed per-connection counter window; it does not create a timer task or per-Stream record. |
| `http2MaxFramesPerWindow` | 10,000 | 100..1,000,000 | Maximum inbound Frames in one window, including control Frames. |
| `http2MaxNewStreamsPerWindow` | 1,000 | 1..100,000 | Maximum newly observed odd client Stream ids in one window; must not exceed the Frame budget. |
| `http2MaxResetFramesPerWindow` | 100 | 1..100,000 | Maximum valid-shaped inbound `RST_STREAM` Frames in one window; must not exceed the Frame budget. |
| `http2MaxContinuationFrames` | 16 | 0..1,024 | Maximum `CONTINUATION` Frames following one `HEADERS`; zero forbids fragmented Header Blocks. |
| `http2MaxEncodedHeaderBlockBytes` | 65,536 | 1,024..1,048,576 | Maximum encoded bytes across one `HEADERS`/`CONTINUATION` sequence before HPACK decoding. |

Existing `http2MaxConcurrentStreams`, `http2MaxPendingAcceptResetStreams`, `http2MaxLocalErrorResetStreams`, `http2MaxSendBufferBytes`, and `http2MaxHeaderListBytes` remain finite and are wired to Hyper/H2. Semantic validation limits each per-connection product of concurrent Streams and encoded Header Block, decoded Header List, or send-buffer bytes to 64 MiB. Every field in this section is Restart-only because an accepted connection owns immutable parser and H2 builder state.

H2 0.4.15 retains its finite 4,096-byte HPACK dynamic-table default. This is a dependency-pinned fact, not an SDKWork configuration guarantee: no `http2HeaderTableBytes` field exists until the selected server runtime exposes and tests a real builder path.

## Runtime Contract

The decrypted Wire Guard is composed after TLS/ALPN and before Hyper. It retains at most client-Preface progress, one 9-byte Frame Header, the current payload countdown, three fixed-window counters, the highest client Stream id, and one Header Block accumulator. It never copies Frame Payloads, HPACK values, request bodies, or per-Stream collections. Payload bytes pass unchanged to H2 after metadata inspection.

Wire-budget violations return an I/O error and close only that connection. They do not fabricate an HTTP response or GOAWAY after rejecting bytes that H2 has not parsed. H2 continues to own RFC protocol errors, `RST_STREAM`, SETTINGS, HPACK, flow control, and GOAWAY; excessive H2-generated local error resets are separately proven to emit `ENHANCE_YOUR_CALM`.

Graceful shutdown delegates to Hyper's connection grace API, waits for already accepted Stream work, rejects later Stream creation after GOAWAY, and is bounded by `drainTimeoutMs`. A deadline expiry terminates remaining connection tasks rather than waiting without bound.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- `cargo test -p sdkwork-webserver-core --test webserver_config` passed 19 tests, including finite HTTP/2 field ranges and the Frame/new-Stream/reset plus 64 MiB encoded-header cross-field constraints.
- `cargo test -p sdkwork-api-webserver-standalone-gateway` passed 14 unit tests, 21 real data-plane integration tests, and 3 raw HTTP/1 connection tests. HTTP/2 evidence uses real Rustls TLS and ALPN, not an in-process Handler substitute.
- The HTTP/2 parser unit corpus proves fragmented Preface/Frame/Header Block input, fixed-window Frame/new-Stream/reset limits, Continuation count, invalid Preface, interleaving, and cross-stream Continuation rejection.
- Real TLS/H2 tests prove configured concurrent-Stream, maximum-Frame, and decoded Header List SETTINGS; PING Frame flood, new-Stream churn, `RST_STREAM` churn, oversized encoded Header Blocks, and H2-generated local-error reset exhaustion; each abusive connection is isolated and a fresh H2 connection remains healthy.
- The H2 local-error test sends malformed `content-length: 1` plus immediate `END_STREAM` requests. This exercises H2's documented protocol-error reset path and observes `ENHANCE_YOUR_CALM` GOAWAY after the configured threshold; oversized Header Lists are correctly tested separately as `431` behavior.
- The graceful-drain test starts an incomplete request Body, begins shutdown after the Stream is active, observes new-Stream refusal after GOAWAY, completes the in-flight response, and observes process exit within the finite deadline.
- Watch integration changes `http2MaxFramesPerWindow`, observes restart-required retention of the active generation, and separately proves a request-body-only candidate remains live-reloadable.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt -- --check`, `pnpm verify`, repository documentation validation, pagination validation, API response-envelope validation, application SDK consumer-import validation, example configuration compilation, and `git diff --check` passed.

Acceptance is limited to the requirement goals and non-goals above. It does not establish exhaustive malformed-Frame/HPACK CPU fuzzing, a real Nginx HTTP/2 differential grade, distributed client-source rate limiting, 100,000 connections, a 24-hour soak, or commercial release readiness. `pnpm verify` explicitly ignored the PostgreSQL lifecycle test because no disposable PostgreSQL URL was configured; PostgreSQL parity is owned by REQ-2026-0004 and is not claimed here. The repository-wide API operation-pattern checker also retains a pre-existing human-review blocker for `GET /backend/v3/api/agent/sync`; this requirement does not alter that public API or generated SDK method.
