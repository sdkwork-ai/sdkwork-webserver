# Rust Request Data-Plane Architecture

Status: active
Owner: SDKWork maintainers
Updated: 2026-07-19
Requirements: REQ-2026-0003, REQ-2026-0005 through REQ-2026-0044
Decisions: ADR-20260715-rust-webserver-data-plane; ADR-20260716-canonical-uri-dual-representation (proposed, human review required)
Specs: RUST_CODE_SPEC.md, CONFIG_SPEC.md, SECURITY_SPEC.md, DEPLOYMENT_SPEC.md, NGINX_SPEC.md, OBSERVABILITY_SPEC.md, TEST_SPEC.md

## 1. Runtime Boundaries

```text
authored app config + host runtime config + resolved secrets
                         |
                         v
sdkwork-webserver-core: schema/model/semantic validation/compiler
                         |
                         v
                 immutable compiled app
                         |
          +--------------+--------------+
          |                             |
          v                             v
HTTP/HTTPS listeners            management listener
static/proxy/routes             app-api/backend-api
no database hot path            service/repository/database
```

The request data plane and management plane may share one packaged binary in the standalone profile, but they do not share bootstrap requirements. A data-plane-only operation can start from a verified local app configuration while the management database is unavailable.

## 2. Configuration Flow

1. Read the bounded configuration file from an explicit host runtime setting.
2. Validate the JSON document against `specs/sdkwork.webserver.config.schema.json`.
3. Deserialize with Serde models that reject unknown fields.
4. Run semantic validation for ids, references, listener conflicts, host ownership, route precedence, paths, TLS, upstreams, and budgets.
5. Canonicalize and compile immutable host/route indexes.
6. Resolve protected file and secret references at listener bootstrap, never while routing a request.
7. Hash the exact bounded source bytes with SHA-256 and expose the compiled revision through one immutable runtime generation.
8. In opt-in Watch mode, detect a changed file, build and validate the entire candidate, construct its upstream clients, and compare restart-only topology.
9. Atomically publish the candidate through `ArcSwap`; invalid or restart-only candidates retain the active generation.

Each request loads one generation before routing and uses that same generation through handler completion. Reload serialization is isolated to a reload-only Tokio mutex and is never acquired on the request path. Old generations are reclaimed when requests that borrowed them finish.

## 3. Crate Responsibilities

| Crate | Added responsibility | Forbidden responsibility |
| --- | --- | --- |
| `sdkwork-webserver-core` | Config types, file loading, semantic errors, normalized domains/paths, route matching, compiled indexes, hard limit validation. | Axum handlers, sockets, TLS I/O, SQLx, management APIs, process control. |
| `sdkwork-api-webserver-standalone-gateway` | Operation dispatch, HTTP/HTTPS binds, static service adapters, proxy transport, request limits, graceful shutdown, management/data-plane composition. | Business rules, SQL queries, generated SDK ownership, raw credential parsing. |
| `sdkwork-webserver-edge-runtime` | Existing external Nginx artifact validation/materialization until renamed or superseded by a later reviewed boundary. | Rust request-path serving. |

The existing `sdkwork-webserver-edge-runtime` name predates the current naming standard. This requirement does not expand it; a separate migration must choose a responsibility-specific replacement without breaking current consumers.

## 4. Request Flow

1. Listener accepts the socket, checks the atomic resource-pressure state, and obtains non-queuing process/listener connection capacity before task creation.
2. When configured, the connection task validates the immediate peer and parses one mandatory bounded PROXY v1/v2 Header before any TLS or HTTP bytes.
3. Rustls negotiates TLS/ALPN for HTTPS listeners.
4. Hyper/Axum parses HTTP under configured header/body/time limits and obtains one total request permit.
5. The gateway preserves raw Path and Query, validates their finite budgets, and produces one bounded canonical Path.
6. The compiled core selects listener, virtual host, and route deterministically.
7. Exact fixed `GET`/`HEAD` operations routes retain total capacity; all other routes must obtain business capacity and pass the pressure check. Rejection releases total capacity before constructing the fixed overload response.
8. The action adapter serves a fixed response, redirect, static resource, Drive WebsiteRoot mount, Knowledgebase WikiPublication, or reverse proxy.
9. Backpressure and cancellation flow through the body stream while request permits remain attached to response ownership.
10. Bounded structured telemetry records result, duration, bytes, and selected ids without secrets.

Supported `http-core-v1` constructs (including `~`/`~*` locations and the rewrite subset) execute after semantic validation. Out-of-profile or invalid constructs fail closed; the runtime never silently approximates undeclared nginx behavior.

## 5. HTTP And TLS Stack

- Axum/Hyper provide HTTP/1.1 and HTTP/2 server behavior.
- `axum-server` integrates Tokio listeners and Rustls TLS configuration.
- TLS files are protected runtime references; key bytes are not represented in the app config model or logs.
- Rustls defaults are constrained to TLS 1.2/1.3. HTTP/2 is negotiated through ALPN.
- `tower-http` static services provide established conditional/range/file behavior and operate below an approved root.
- Hyper legacy client plus Hyper-Rustls provides upstream HTTP/HTTPS pooling, streaming, HTTP/2 multiplexing, and generation-level physical connection ownership. Proxy transport does not implement redirect following.

The bounded ingress profile configures Hyper directly. HTTP/1 uses explicit parser-buffer bytes, header count, and a Tokio-timer-backed complete-header deadline. HTTP/2 uses explicit concurrent-stream, header-list, pending-reset, local-error-reset, stream send-buffer, fixed flow-control windows, and frame-size limits.

For a listener with `proxyProtocol`, the admitted TCP stream first enters a connection-local parser under one finite timeout. The immediate peer must match an immutable trusted CIDR before any bytes are consumed. Exact reads distinguish the 12-byte v2 signature from the v1 `PROXY ` prefix without reading TLS/HTTP payload bytes. v1 uses a fixed 107-byte line buffer and strict CRLF; v2 validates version, command, TCP family/protocol, address length, and configured total length, then discards remaining TLVs through a fixed 256-byte scratch buffer. `LOCAL` and `UNKNOWN` preserve the transport peer. A successful `PROXY` command creates immutable transport/client connection info for Axum; all failures close before TLS/HTTP and increment only a fixed `proxy_protocol` rejection counter.

An incremental HTTP/1 Framing Guard wraps the accepted stream after Rustls and before Hyper. It bounds and validates request-line structure, method tokens, visible-ASCII request targets, Header/Trailer field names and values, Chunked input, and Trailers; rejects original-wire TE/CL ambiguity and duplicate lengths; then passes unchanged valid bytes to Hyper. A line buffer cannot grow beyond the smaller applicable total/individual ceiling. One connection-local Atomic counter bounds complete request heads observed by the Guard but not yet submitted to the paired Service; dispatch releases a slot synchronously without retaining a Body or creating a queue. TLS ALPN `h2` streams bypass the Guard and Pipeline counter.

An incremental HTTP/2 Wire Guard wraps decrypted ALPN `h2` streams before Hyper. It validates the client Preface and Frame metadata, applies fixed-window Frame/new-Stream/`RST_STREAM` budgets, and bounds encoded Header Block bytes and `CONTINUATION` count without retaining payloads. Cross-stream Continuation, interleaving, invalid client HEADERS ids, invalid reset shape, and configured Frame oversize fail closed at connection scope. H2 remains the owner of HPACK, flow control, SETTINGS, RFC stream/connection errors, and GOAWAY. The selected Hyper server builder advertises configured concurrent Streams, Frame size, and decoded Header List size; it does not expose HPACK dynamic-table sizing, so H2 0.4.15 retains its pinned 4,096-byte default and the application exposes no fake field.

The same Hyper/H2 connection engine owns Keep-Alive PING and ACK matching. It sends one PING after `http2KeepAliveIntervalMs` without an inbound Frame, including when no Stream is active. Missing ACK at `http2KeepAliveTimeoutMs` invokes H2 `abrupt_shutdown(NO_ERROR)`, flushes GOAWAY, and closes. SDKWork adds no second PING parser or timer task. Responsive application-idle clients remain connected until the independent total connection-age deadline.

The listener owns every accepted Hyper connection Future in a bounded `JoinSet`. The accept loop uses non-queuing process/listener permit acquisition before it creates a task, TLS state, or HTTP state, so accept floods cannot bypass the active task bound. One connection-owned `maxConnectionAgeMs` Timer starts after the Acceptor chain completes and never resets on traffic. Age expiry calls Hyper `graceful_shutdown`, which disables HTTP/1 reuse or emits H2 `GOAWAY(NO_ERROR)`. The Future remains polled for at most `drainTimeoutMs`; expiry drops the complete transport/Acceptor chain and releases its permit. Process shutdown signals the same tasks, stops acceptance, drains them concurrently, aborts remaining tasks at the shared deadline, and joins all task results. No raw GOAWAY injection, socket-error approximation, per-Stream age task, or detached accepted-connection task exists.

Classic WebSocket is a deliberate exception to HTTP connection-Future ownership because Hyper transfers both sockets into `Upgraded` after a successful `101`. A runtime-level `TunnelSupervisor` therefore takes explicit ownership of both `OnUpgrade` futures, the upstream in-flight permit, and the immutable configuration generation. It retains only an active atomic count, one Watch shutdown channel, and a drain notification; completed task handles are not accumulated. Each task adapts the upgraded sockets directly to Tokio and runs `copy_bidirectional_with_sizes` with fixed 16 KiB buffers in each direction while selecting over upgrade completion, runtime shutdown, the connection-age hard deadline, and I/O completion. Process shutdown first stops listener acceptance, then signals tunnels and gives them only the remainder of the same drain deadline.

The decrypted HTTP/1 wire guard shares a connection-local upgrade control with its paired Service. Only a proxy request that has passed HTTP/1.1 GET, exact token, and absent Body-framing validation activates raw-byte mode. The upstream request regenerates canonical `Connection: upgrade` and `Upgrade: websocket`; `Sec-WebSocket-*` metadata remains end to end. An upstream `101` must present the matching response tokens and no Body framing. Any local failure, upstream non-101 rejection, or invalid `101` closes the downstream connection after its bounded response, preventing a raw-mode connection from returning to HTTP parsing. HTTP/2-only requests never enter this path; RFC 8441 extended CONNECT is not implemented.

The proxy bridge preserves `http_body::Frame` values in both directions. Request Data frames are counted while Trailer frames are checked against the same finite Trailer budget and forbidden-field policy. The Hyper upstream client receives the guarded Body directly, and its `Incoming` response remains an HTTP Body, so valid response Trailer frames remain visible. Client hop-specific `TE` is removed and a canonical `TE: trailers` is generated for the upstream; validated `Trailer` declarations remain end-to-end metadata. HTTP/1 downstream emission follows Hyper's `TE: trailers` recipient signal. Undeclared cross-protocol Trailer synthesis, complete gRPC behavior, broader HTTP/2 HPACK malformed-input conformance, client source accounting, and live certificate-map rotation require later focused requirements.

The upstream client configures HTTP/1 maximum parser-buffer bytes, conditionally configures a non-default maximum field count, and configures the H2 maximum Header List from the immutable upstream policy. A response is not exposed to the proxy adapter until an allocation-free pass checks `HeaderMap::len()` and accumulates each occurrence's name/value/separator bytes with checked arithmetic. Rejection drops `Incoming` before Body wrapping, so no rejected response Body or Trailer is polled or forwarded. The error carries no Header content, maps to the existing generic local `502`, and participates in passive or active target failure state.

An early final upstream response changes the proxy request lifecycle from active to paused before the handler exposes the response downstream. Paused request Bodies return `Pending` without reading another client Frame. Because Hyper's upstream HTTP/1 driver may drop its request Body before resolving the response Future, an incomplete Body transfers its existing inner Body object into one deferred ownership slot. The guarded response owns that slot until Hyper processes response completion or downstream cancellation; response-wrapper Drop cancels the upstream producer and releases the client Body. HTTP/1 then carries explicit `Connection: close`; H2 completes the response before `RST_STREAM(NO_ERROR)` and keeps other Streams usable. This two-phase handoff preserves response status, Headers, Body, and Trailer validation without buffering the request.

The URI layer keeps the original Path and Query for raw proxy fidelity and produces exactly one canonical Path after bounded validation. Canonical route/static identity performs one percent-decoding pass, merges repeated slashes, resolves dot segments, preserves significant trailing slash, and rejects traversal above root, decoded control/NUL/backslash, and invalid UTF-8. Authored route paths are validated directly as canonical Path values and are never percent-decoded again, so decoded `?`, `#`, and `%` remain matchable Path data. A proxy without URI rewriting retains the raw request URI; `stripPrefix` uses the canonical Path and the original Query. This behavior is implemented under REQ-2026-0018 but remains draft until human review accepts proposed ADR-20260716 and its documented Windows Nginx security differences.

The admitted response Body owns a reusable idle Timer and the request permit. Pending producer polls register the Hyper task Waker; meaningful Data/Trailer progress resets the deadline, while empty Data does not. A separate accepted-stream wrapper sits outside TLS and both Wire Guards and times continuously Pending `AsyncWrite` write/flush/shutdown calls. This separation covers both a stalled producer and a slow-reading downstream without collecting response data.

HTTP/1 connection handling enables Hyper write-side half-close so EOF after a complete request does not suppress the response. The Handler accepts only one exact HTTP/1.1 `Expect: 100-continue`; known fixed-length overflow returns `413` before Body polling, other expectations return `417`, and the proxy removes `Expect` after the listener has completed the client negotiation. Transfer-Encoding is accepted only for HTTP/1.1 and remains subject to the original-wire Framing Guard. Hyper service readiness serializes Pipeline dispatch, its parser buffer bounds unread bytes, and `http1MaxPipelineDepth` independently bounds fully read request heads awaiting dispatch. An over-depth connection closes because a parser-level synthetic response could violate existing Pipeline response order.

## 6. Concurrency And Memory

- Configuration and route indexes are immutable after compile.
- The request path performs one lock-free `ArcSwap::load_full`; reload never mutates a live route or upstream collection.
- A candidate is fully compiled and all Hyper/Rustls upstream clients, TLS contexts, physical connection limits, and pools are built before the active pointer changes.
- Configuration input is read through a `MAX_CONFIG_BYTES + 1` bounded reader, closing the metadata/read replacement allocation race.
- HTTP/1 line memory starts at no more than the request-line budget and grows only to the smaller applicable total and individual field ceiling; rejected wire bytes are not copied into diagnostics.
- TLS PEM reads are bounded to 1 MiB per certificate or private-key file before Rustls parsing.
- TLS startup validates every leaf certificate's current validity, SAN coverage, and private-key match before building immutable Exact and single-label Wildcard hash indexes; a handshake performs no scan across the policy certificate collection.
- Request handlers do not hold locks across `.await`.
- Proxy request Frames use only atomic lifecycle reads and `AtomicWaker` registration. The early-response deferred-Body slot uses one non-nested standard mutex only for ownership transfer/release; it is never touched per Frame, never held across `.await`, and destructors run after the guard is released.
- One process-wide total Semaphore bounds admitted requests across listeners with `try_acquire_owned`, so overload creates no waiter queue. When resource pressure is configured, a second smaller business Semaphore leaves the operations reserve. Successful classifications keep their permit set in the response Body until end/error/drop; pressure or business-capacity rejection releases total capacity synchronously before returning.
- Each admitted response has one reusable producer-idle Timer; each connection lazily allocates at most one reusable write deadline after the first Pending operation. Neither timeout adds a response queue or payload copy.
- Proxy bodies are polled as bounded Data/Trailer frames; no `to_bytes`, `collect`, or data-only conversion is permitted on the proxy path.
- Upstream response Header parsing is bounded before materialization for HTTP/1 and H2, then checked in one linear pass over an already bounded `HeaderMap`; no Header value is copied into a second collection, diagnostic, or log.
- Default weighted round-robin uses one generation-local standard Mutex protecting boxed signed-current-weight and recovery-marker arrays sized to the fixed target vector. One short linearized transaction adds effective weights, selects by greatest current weight with stable ties, and subtracts the eligible total; it never allocates, performs I/O, or crosses `.await`. Health transitions reset the corresponding phase under the same lock, retry exclusion only skips, primary/backup phases remain independent, and a half-open race performs at most one fallback scan. Optional least-connections remains independent, adding one generation-local shared atomic active-request counter per target and overflow-safe integer ratio comparison. Neither strategy stores an expanded schedule or request-level collection; activity ownership continues through HTTP response or WebSocket tunnel lifetime.
- Optional health-recovery slow start adds one immutable duration and one atomic generation-relative start offset per target. Round-robin and least-connections lazily derive the same monotonic effective integer weight without a timer task, queue, request allocation, wall clock, or stale compare-exchange reset.
- Non-proxy request bodies are stream-discarded and counted before action execution; fixed, Chunked, and HTTP/2 no-length bodies share one application Body budget without Body-sized collection.
- Static files are opened from one capability handle for the configured root. Request, index, and
  SPA fallback paths are validated; every directory component uses capability-relative no-follow
  opening; the final regular file is opened no-follow; and the response streams only from that
  already-open handle. Path replacement therefore cannot redirect an active response. Immutable
  read-only roots remain defense in depth against untrusted writers, hard links, and mount changes.
- Connection, body, header, timeout, route, host, upstream, target, and config byte limits are validated before serving.
- HTTP/2 concurrent decoded-header, encoded-header, and send-buffer products are each capped at 64 MiB per connection. The Wire Guard holds constant parser state and no payload or per-Stream collection; protocol-limit changes are Restart-only and cannot partially replace a live listener generation.
- Active HTTP/2 decoded Header List and send-buffer products and the connection-level encoded Header Block product are each capped at 1 GiB globally. Optional RSS/Working Set, finite cgroup v2, FD/HANDLE, and event-loop-lag admission adds measured process headroom; it does not replace hard container limits or load/soak evidence.
- The global connection/header-window product is capped at 1 GiB; Chunk Size lines and Trailer collections have separate finite limits.
- Listener tasks are supervised. A bind or TLS bootstrap failure prevents readiness and terminates the requested data-plane operation.
- Listener and accepted-connection tasks are supervised. Shutdown and maximum-age retirement use finite drain deadlines; no detached connection or listener task remains after process exit.
- Active health adds exactly one supervised Tokio task per immutable generation. It owns one deadline entry per checked target and directly polls at most `maxConcurrentHealthChecks` probe futures; no target task, waiter, or overlapping probe exists. Cancellation drops all in-flight futures, and reload/shutdown await the scheduler handle without holding a request-path or network-I/O lock.
- Resource pressure adds exactly one supervised process-lifetime sampler, one awaited bounded blocking OS sample at a time, and fixed atomic state. It performs no per-request allocation, no overlapping sample, and no detached per-sample task; shutdown cancels and joins it.

The foundation establishes bounded behavior but does not yet satisfy the parent 100,000-connection or 24-hour soak targets until dedicated load and memory evidence exists.

## 7. Operation Modes

| Mode | Database | Behavior |
| --- | --- | --- |
| Default management mode | Required by current control plane | Existing app-api/backend-api and service health behavior. |
| `db-migrate` | Required | Existing database migration-only behavior. |
| `validate` | Not used | Validate and compile one Web Server app configuration, print redacted summary, exit non-zero on any blocker. |
| `data-plane` | Not used | Start configured HTTP/HTTPS application listeners, optionally enable a separate loopback host operations listener, optionally Watch the config when `deployment.reload.mode=watch`, and drain all owned tasks on shutdown. |
| Future combined mode | Management optional after startup policy | Start isolated management and request listeners with separate readiness and failure policies. |

## 8. Implementation Status

| Capability | Status at document update |
| --- | --- |
| Product PRD and bounded runtime foundation requirement | Defined; REQ-2026-0003 accepted |
| Architecture decision | Accepted |
| Machine configuration schema | Implemented and verified for the declared foundation profile |
| Core config model/compiler | Implemented; strict validation and immutable host/route indexes verified |
| HTTP listener and fixed/redirect routes | Implemented and real-socket tested |
| Static and streaming proxy routes | Implemented and real-socket tested; proxy bodies are not fully collected. Static delivery uses capability-relative per-component no-follow opening and streams from the stable open handle. Windows and Linux tests cover path replacement, final/intermediate symlink rejection, directory index/redirect, SPA fallback, conditional requests, Range, HEAD, and MIME behavior. |
| Drive and Knowledgebase application-config resources | Implemented: `drive`/`knowledgebase` resources in `sdkwork.webserver.config.json` are assembled fail-closed from environment-owned provider SDK credentials, validated before startup, served through the shared bounded provider registry and resolution cache (GET/HEAD, conditional requests, Range, SPA candidates, wiki routes/redirects, resource-level locale default), and watched reloads referencing an unassembled provider are rejected with the active generation retained |
| HTTPS listener | Implemented and tested with Rustls TLS, HTTP/2 ALPN, immutable multi-certificate Exact/Wildcard SNI selection, leaf SAN/time/key activation checks, and fail-closed unknown/no-SNI behavior |
| Connection admission and graceful drain | Implemented and tested under connection saturation |
| Same-topology local configuration reload | Implemented with SHA-256 revisions, `ArcSwap`, failed-candidate retention, and concurrent real-socket tests |
| Bounded protocol ingress | Implemented for HTTP/1 header bytes/count/deadline, pre-Hyper original-wire Chunked/Trailer framing, all-action Body limits, and HTTP/2 stream/header/reset/flow-control/send-buffer settings; complete parser and abuse conformance remains pending |
| Bounded proxy Trailer fidelity | Implemented for declared HTTP/1 request/response Trailers and HTTP/2 trailing HEADERS with declaration/frame limits, forbidden-field validation, and no Body collection |
| HTTP/1 connection semantics | Implemented and raw-socket tested for HTTP/1.0 default host/Keep-Alive/ordered Pipeline, strict Expect/Continue, proxy Expect termination, TLS Continue, and complete/truncated half-close; complete differential conformance remains pending |
| HTTP/1 request-field budgets | Implemented in Schema/Core/Wire Guard for request line, method, target, Header/Trailer name and value; plain/TLS raw-socket and Restart-only reload evidence added |
| HTTP/2 abuse and drain budgets | Implemented in Schema/Core and a constant-memory decrypted Wire Guard; real TLS/H2 tests cover SETTINGS, Frame/new-Stream/reset churn, encoded blocks, H2 local-error GOAWAY, connection recovery, graceful drain, and Restart-only reload |
| Process request admission | Implemented as one non-queuing cross-listener Semaphore with response Body lifetime ownership; real HTTP/1 and TLS/H2 tests cover overload, completion, H2 reset cancellation, recovery, and Restart-only reload |
| Response progress timeouts | Implemented separately for meaningful response Body Frame gaps and downstream write/flush/shutdown Pending time; real HTTP/1, slow-reader, TLS/H2, recovery, and Restart-only reload tests added |
| Request Body progress timeouts | Implemented as one all-action zero-copy Body wrapper with distinct first-meaningful-Frame and later progress deadlines; real HTTP/1, proxy, TLS/H2, one-permit recovery, empty-Frame, and Restart-only reload tests added |
| HTTP/1 Keep-Alive idle timeout | Implemented as a protocol-aware connection Stream/Service activity pair; plain/TLS H1 idle, one-connection permit release, upload, long response, Pipeline, H2 bypass, recovery, and Restart-only tests pass |
| URI and Query component budgets | Accepted allocation-free cross-H1/H2 raw/decoded Path, segment, Query/parameter/component and percent-safety checks; real H1/H2/proxy/reload tests pass |
| Canonical URI dual representation | Implemented draft with canonical route/static/rewrite identity and raw no-rewrite proxy fidelity; real Nginx/H1/H2/static/proxy evidence exists, but proposed ADR-20260716 requires human review |
| HTTP/1 Pipeline depth | Accepted with one connection-local pending-head counter, plain/TLS over-depth close, one-connection recovery, H2 bypass, Restart-only reload, and Nginx 1.26.2 difference evidence |
| HTTP/2 Keep-Alive PING | Accepted with Hyper/H2 idle PING, finite ACK timeout, `GOAWAY(NO_ERROR)`, H1 isolation, one-connection recovery, Restart-only reload, and pinned Nginx evidence |
| Proxy early-response request lifecycle | Implemented with two-phase pause/cancel ownership, complete response preservation, HTTP/1 close and upstream-pool eviction, H2 `NO_ERROR` Stream cancellation/reuse, timeout/admission recovery, and pinned Nginx evidence |
| Bounded connection maximum age | Implemented with one connection-owned Timer, Hyper-aware HTTP/1/H2 graceful retirement, finite in-flight drain/cancellation, fresh-connection recovery, and Restart-only Watch tests |
| Bounded upstream DNS and SSRF policy | Implemented with shared asynchronous system-resolver profiles, non-queuing query admission, finite answer/timeout/idle-pool bounds, literal and per-resolution address checks, rebinding rejection, and real default-deny/explicit-local proxy evidence |
| Upstream TLS identity and pool isolation | Implemented with system/custom/combined trust, bounded protected CA and mTLS identity files, TLS 1.2/1.3 constraints, hostname verification, startup/reload fail-closed construction, and per-upstream/per-generation client pool isolation |
| Upstream admission and passive health | Implemented with non-queuing request-lifecycle permits, fixed-cardinality atomic target health, configured failure statuses, finite ejection, one half-open probe, healthy-target continuation, no hidden retry, and fresh generation state |
| Supervised active upstream health | Implemented with one generation-owned scheduler task, fixed target deadlines/state, bounded concurrent probe futures, status/timeout/Body ceilings, independent active/passive gating, and explicit Watch/shutdown cancel plus join |
| Adaptive resource pressure admission | Implemented for Windows Working Set/HANDLE, Linux RSS/FD/finite cgroup v2, event-loop lag, effective reserve/hysteresis validation, total/business request partition, pre-task socket close, HTTPS/H2 Stream isolation, recovery, Restart-only Watch, and supervised shutdown |
| Bounded upstream physical connections | Implemented with an aggregate generation-level non-queuing cap across DNS/TCP/TLS, active HTTP/1, multiplexed H2 and idle pools; local saturation preserves target health, Watch/shutdown close idle sockets, and real socket counters verify the ceiling |
| Bounded target physical connections | Implemented with optional unique-authority non-queuing target caps beneath the aggregate upstream cap; both permits share exact socket ownership through H1/H2/idle, distinct targets remain isolated, and fixed aggregate capacity metrics expose no authority labels |
| Bounded upstream response Headers | Implemented with HTTP/1 parser bytes/count, H2 Header List bytes, exact decoded occurrence accounting, generic fail-closed `502`, passive/active health classification, Watch generation replacement, and real HTTP/1/HTTPS-H2 recovery evidence |
| Smooth weighted upstream targets | Implemented with exact stable 3:1 and 5:1:1 sequences, equal-weight order, process-local concurrent linearization, fixed arrays, contention metrics, independent primary/backup phases, retry exclusion, atomic health-phase reset, one half-open fallback, Watch replacement, and real dual-origin distribution/recovery evidence |
| Backup upstream targets | Implemented as immutable primary/backup roles with primary-first bounded scans, weighted health-aware backup fallback, primary half-open precedence, retry composition, and atomic Watch role replacement/all-backup rejection |
| Weighted least-connections | Implemented with typed default-compatible strategy selection, generation-local RAII active-request counters, overflow-safe weighted comparison, primary/backup and health composition, retry replacement, streaming cancellation, H2 Stream/socket separation, WebSocket tunnel ownership, and Watch isolation evidence |
| Weighted random-two least-connections | Implemented with strict typed selection, one generation-local atomic SplitMix64 state, multiply-high weighted sampling without replacement, distinct candidates, overflow-safe active-load comparison, single-target and bounded race fallback, streaming activity evidence, and no request allocation/loop/lock |
| Direct-peer IP-hash affinity | Implemented with exact Nginx IPv4/IPv6 key bytes and recurrence, nominal weighted ranges, health-preserving deterministic rehash, twenty-one mappings plus one smooth fallback, primary/backup and retry composition, spoofed-forwarding isolation, and no client state/ring/allocation |
| Upstream recovery slow start | Implemented with bounded typed target duration, active/passive eligibility transitions, monotonic generation-relative integer ramping shared by both selection strategies, restart/stale-completion safety, real 1:1-to-4:1 recovery traffic, and invalid Watch retention |
| Classic WebSocket reverse proxy | Implemented for bounded HTTP/1.1 WebSocket/WSS tunnels with strict handshake validation, fixed directional buffers, retained upstream admission, immutable generation ownership, hard lifetime, and supervised shared-budget shutdown |
| Management HTTP metrics | Implemented with one shared app-api/backend-api/`/metrics` framework registry, bounded series and label bytes, fixed unmatched-route cardinality, dropped-series counters, and fail-closed canonical dimensions |
| Data-plane operations metrics | Implemented with a separate loopback-only bounded HTTP/1 listener and fixed saturating atomics for connection/request/upstream/reload/tunnel events plus aggregate target-health and resource-pressure gauges; tenant virtual hosts do not mount the registry |
| Data-plane RED and capacity metrics | Implemented with fixed seconds histograms through response Body/upstream Header lifecycle, streaming Body and successful tunnel byte counters, fixed protocol/DNS outcomes, cancellation-safe DNS active ownership, and current-generation request/physical-connection capacity snapshots |
| Bounded safe upstream retries | Implemented as opt-in sequential distinct-target failover for Body-end-of-stream idempotent methods, with exact supported Nginx condition tokens, one retained request permit, fixed stack attempted-target state, finite attempt/total deadlines, passive-health accounting, cancellation-safe probe ownership, and fixed retry metrics |
| Full repository verification | `pnpm verify` and strict full-workspace Clippy pass |
| Dynamic certificate rotation and restart recovery | Implemented for `tlsRuntime: assignment` with bounded material resolution, SAN/time/key/fingerprint validation, exact/wildcard SNI, listener-compatible TLS/ALPN policy, atomic Rustls reload, failed-candidate last-known-good retention, and independent A/B snapshot recovery |
| Website persisted rollback, listener bind/topology hot handoff, executable upgrade, complete Nginx profile, cache, cluster rollout | Not implemented; later requirements |

No planned row may be reported as implemented until its verification evidence passes.

## 9. Verified Foundation And Remaining Boundary

The accepted foundation evidence covers configuration compilation, process bootstrap, real HTTP/HTTPS sockets, exact/default host selection, exact/prefix route precedence, fixed/redirect/static/proxy actions, streaming proxy transport, request-size rejection, finite timeouts, non-queuing connection admission, traversal and authority-confusion rejection, TLS 1.2/1.3 policy, HTTP/2 ALPN, finite shutdown drain, and same-topology local configuration reload with failed-candidate retention. Accepted REQ-2026-0006 separately adds static multi-certificate SNI selection and activation-time leaf SAN/time/key checks. Accepted REQ-2026-0007 adds bounded protocol budgets. Accepted REQ-2026-0008 adds safe original-wire Chunked/Trailer framing and all-action Body accounting. Accepted REQ-2026-0009 adds frame-preserving declared HTTP/1 and HTTP/2 request/response Trailer proxying. REQ-2026-0010 adds the focused HTTP/1 connection-state slice and records the first real Nginx 1.26.2 protocol comparison. REQ-2026-0011 adds pre-allocation request-line and individual field ceilings. REQ-2026-0012 adds constant-memory HTTP/2 Frame/Header Block abuse controls and real graceful GOAWAY drain evidence. REQ-2026-0013 adds non-queuing process request admission held through streaming response completion or cancellation. REQ-2026-0014 adds separate response producer-idle and downstream write-stall deadlines. REQ-2026-0015 adds distinct request Body start/progress deadlines, HTTP/1 close behavior, H2 Stream isolation, and proxy timeout classification. REQ-2026-0016 adds protocol-scoped HTTP/1 Keep-Alive idle reaping without misclassifying active uploads, responses, or pipelines. Accepted REQ-2026-0017 adds bounded URI and Query components. REQ-2026-0018 adds the implemented but not yet accepted dual raw/canonical URI behavior. Accepted REQ-2026-0019 adds the HTTP/1 pending Pipeline depth boundary. Accepted REQ-2026-0020 adds H2 PING/ACK failure detection. REQ-2026-0021 adds bounded early-response pause/cancel ownership with complete response preservation and protocol-scoped connection behavior. REQ-2026-0022 adds finite connection lifetime, protocol-aware graceful retirement, and owned connection-task drain. REQ-2026-0023 adds bounded asynchronous system resolution, explicit private-range authorization, per-answer rebinding defense, and finite idle pool lifetime. REQ-2026-0024 adds verified private trust, mTLS identity, TLS version policy, and security-context pool replacement. REQ-2026-0025 adds per-upstream request-lifecycle admission and passive target ejection/recovery. REQ-2026-0026 adds bounded supervised active checks and deterministic generation/shutdown ownership. REQ-2026-0027 adds adaptive process pressure admission, health-priority request reserve, HTTPS/H2 isolation, hysteretic recovery, and sampler lifecycle evidence. REQ-2026-0028 adds a real generation-level physical connection ceiling, bounded dual-stack fallback, H2 multiplexing under a cap of one, and prompt retired-pool closure. REQ-2026-0029 adds parser-level and decoded upstream response Header budgets with HTTP/1/H2, health-state, and Watch recovery evidence. REQ-2026-0030 makes relative target weights executable with bounded health-aware dual-origin and Watch evidence. REQ-2026-0031 adds bounded classic HTTP/1.1 WebSocket/WSS proxy tunnels with explicit lifetime and shutdown ownership. REQ-2026-0032 makes management HTTP metrics real and bounds framework metric cardinality. REQ-2026-0033 adds isolated fixed-cardinality data-plane operations metrics without tenant-route exposure. REQ-2026-0034 adds bounded request/upstream histograms, streaming byte counters, fixed protocol/DNS results, and authoritative Semaphore capacity snapshots without fabricating idle-pool state. REQ-2026-0035 adds bounded distinct-target retries for replay-safe requests while retaining all earlier admission, transport, health, and metric ownership boundaries. REQ-2026-0036 adds optional authority-level socket limits beneath the aggregate upstream bound without approximating physical connections from requests or H2 Streams. REQ-2026-0037 adds primary/backup target tiers that reuse the same bounded weighted, health, retry, TLS, capacity, and generation mechanisms. REQ-2026-0038 adds generation-local weighted least-connections with streaming, H2, WebSocket, retry, and Watch ownership evidence. REQ-2026-0039 adds bounded active/passive recovery slow start shared by both selection strategies without per-request state.

REQ-2026-0040 adds fixed-state process-local smooth weighted round robin with stable exact sequences, concurrent linearization, health/slow-start phase reset, primary/backup and retry composition, contention telemetry, poison recovery, and Watch isolation evidence.

REQ-2026-0041 adds bounded weighted random-two least-connections with one generation-local atomic scheduling sequence, distinct sampling without replacement, effective-weight and active-load comparison, single-target/race fallback, and real streaming activity evidence.

REQ-2026-0042 adds bounded direct-peer IP-hash affinity with exact Nginx IPv4/IPv6 hash vectors, weighted deterministic mapping, health/retry/backup fallback, slow-start incompatibility validation, spoofed-forwarding isolation, and real socket recovery evidence.

REQ-2026-0043 adds listener-local trusted-proxy real-IP with no trust by default, bounded single-XFF parsing, Nginx-compatible recursive and non-recursive selection, canonical upstream identity, effective-IP IP-hash/retry composition, atomic Watch policy changes, and real HTTP/HTTPS/H2 evidence. It intentionally excludes PROXY protocol, RFC 7239 `Forwarded`, hostname trust sources, and dynamic cloud CIDR discovery.

REQ-2026-0044 adds mandatory trusted-source HAProxy PROXY v1/v2 before TLS/HTTP, finite preface time/byte budgets, TCP4/TCP6 plus `UNKNOWN`/`LOCAL`, bounded TLV discard, effective-IP routing composition, fixed-cardinality rejection telemetry, and Restart-only policy enforcement. REQ-2026-0046 extends that parser with exact TLV framing and one streaming Castagnoli CRC32C digest under `ignore`, `validate-if-present`, or `required`; it retains only fixed metadata/value/scratch arrays and never trusts non-CRC TLVs. CRC remains integrity detection rather than peer authentication. AF_UNIX/UDP families, outbound `send-proxy`, and insecure optional auto-detection remain excluded.

REQ-2026-0047 removes false external-Nginx validation and reload success. The host adapter validates the exact staged site through an isolated generated main config and real `nginx -t`, bounds candidate/diagnostic/time/process state, confines domain paths, atomically persists only valid candidates, and propagates disabled, unavailable, timeout, syntax, deploy, and reload failures. Backend request tasks isolate blocking host work through `spawn_blocking`; Repository code no longer claims syntax validity without an edge provider. Database desired/active state and an external Nginx master still require a later durable reconciliation state machine for cross-system atomicity.

This evidence does not establish the parent PRD's commercial release. No claim is made yet for hard allocator/OOM immunity, CPU/PSI/disk pressure governance, per-tenant fairness, persisted operator rollback, live certificate-map rotation and served-fingerprint convergence, listener socket handoff, executable upgrade, undeclared cross-protocol Trailer synthesis, full gRPC proxy conformance, complete HTTP/1 parser differential/fuzz and timeout conformance, exhaustive HTTP/2 malformed-frame/HPACK CPU/fuzz and Nginx differential conformance, RFC 8441 or WebSocket frame/heartbeat/idle policy, SSE heartbeat, PCRE2 location semantics, Nginx import/render/differential conformance, trust or forwarding of non-CRC PROXY v2 TLVs, cryptographic PROXY Header authentication, PROXY AF_UNIX/UDP or outbound `send-proxy`, RFC 7239 `Forwarded`, dynamic proxy CIDR discovery, custom DNS transport or authoritative TTL/cache/stale/CNAME behavior, upstream pinning/revocation/dynamic secret providers, Nginx shared-zone/cross-process/cluster connection, current-weight, active-load, and slow-start accounting, accept/TLS/cache phase telemetry, authoritative Hyper idle-pool occupancy, tracing/exporters, authenticated remote operations, dashboards/alerts, cluster-global health, arbitrary/consistent-hash and sticky balancing, sub-integer/exact Nginx slow-start ticks, or Nginx-internal tie behavior, non-idempotent/idempotency-key or Body replay, shared retry budgets, hedging and cluster circuit state, cache/compression/rate limiting, complete production observability, 100,000 concurrent connections, 24-hour soak, chaos/failover, Kubernetes high availability, backup/restore, signed SBOM/provenance, or commercial support operations.
