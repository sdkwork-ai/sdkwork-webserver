# SDKWork Web Server Nginx Compatibility PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-web
Updated: 2026-07-16
Parent: [PRD.md](PRD.md)
Specs: NGINX_SPEC.md, REQUIREMENTS_SPEC.md, SECURITY_SPEC.md, PERFORMANCE_SPEC.md, TEST_SPEC.md

## 1. Purpose

Define the supported Nginx OSS HTTP compatibility contract for the SDKWork Rust data plane and its configuration tools. Compatibility is an explicit, versioned product profile with conformance evidence. It is not a claim that arbitrary Nginx configuration, modules, or implementation details can execute unchanged.

Operators enable or disable the compatibility surface with
`[nginx].enabled` (`nginx.enabled`) in `deployments/webserver/server.common.toml`
(default `true`). When disabled, the module is Rust-native TOML only: sidecar
W16 checks are skipped and `tools/import-nginx-conf.mjs` refuses activation.
Living capability status for `http-core-v1` is tracked in
`specs/nginx-gap.catalog.json`.

The Rust engine is the primary runtime. Nginx import and rendering are migration and interoperability capabilities. A configuration may activate on the Rust engine only when every behavior-affecting directive is mapped to the normalized intermediate representation or rejected with a blocking diagnostic.

## 2. Compatibility Profile And Grades

The initial profile is `http-core-v1`. Every import, normalized revision, validation report, rendered file, and deployment records the profile and target Nginx version used for evaluation.

Compatibility is reported independently at four grades:

| Grade | Meaning |
| --- | --- |
| Parse and round-trip | Supported source syntax, comments, includes, quoting, escaping, and source locations can be parsed; preserved opaque text is never executable by the Rust engine. |
| Semantic | Supported directives compile to an equivalent normalized model with the same precedence, inheritance, defaults, and validation behavior. |
| Behavioral | Requests produce equivalent routing, status, headers, body, TLS selection, proxy behavior, and failure behavior within documented comparison rules. |
| Operational | Start, validation, reload, drain, logging, metrics, failure detection, and rollback meet the declared operational contract. |

The UI and API must show the lowest achieved grade, blocking diagnostics, warnings, intentional differences, and evidence revision. A single `compatible: true` flag is insufficient.

## 3. Supported V1 Directive Surface

The conformance catalog, not this summary, is the machine-readable authority for individual directive support. V1 product scope includes the following common Nginx OSS HTTP groups:

| Group | Required behavior |
| --- | --- |
| Structure | `http`, `server`, `location`, bounded and sandboxed `include`, directive context validation, inheritance, and deterministic source order. |
| Listeners and hosts | `listen`, `server_name`, default server, IPv4/IPv6, HTTP/1.0 compatibility, HTTP/1.1, HTTP/2 mapping, Proxy Protocol policy, and host validation. |
| Static content | `root`, `alias`, `index`, `try_files`, `autoindex` policy, MIME types, conditional requests, ranges, and precompressed assets. |
| Proxying | `proxy_pass`, upstream URI mapping, request/response headers, forwarding identity, buffering, streaming, WebSocket upgrade, SSE, timeouts, body limits, and bounded retries. |
| Upstreams | `upstream`, server weights, backup/drain behavior, failure policy, keepalive, round robin, least connections, IP hash, and supported hash policies. |
| Routing | Exact, prefix, `^~`, case-sensitive regex, case-insensitive regex locations, named-location mapping where supported, `return`, and the supported `rewrite` subset. |
| TLS | `ssl` listeners, certificate selection, protocols, policy-safe cipher mapping, session behavior, client certificate validation, OCSP policy, and SNI. |
| Response behavior | Header transforms, error-page mapping, redirects, gzip, cache control, proxy cache subset, access control, connection limits, and rate limits. |
| Operations | Config test, explain, source-mapped diagnostics, atomic reload, graceful drain, access/error logs, and revision status. |

Every directive has a catalog entry containing allowed contexts, argument grammar, inheritance, normalized field, runtime support, renderer support, risk classification, diagnostics, comparison fixture, and known differences.

## 4. Explicitly Excluded V1 Surface

V1 excludes:

- Arbitrary third-party or dynamically loaded Nginx modules.
- OpenResty/Lua, Perl, njs, and embedded executable configuration.
- Nginx Plus proprietary APIs, active health-check implementation details, key-value zones, and commercial-only features unless separately licensed and specified.
- Mail proxy and generic TCP/UDP `stream` proxying.
- OS-specific directives that cannot be reproduced safely and portably.
- Embedded CGI, FastCGI, uWSGI, SCGI, and arbitrary forward-proxy/`CONNECT` execution. These require separate protocol and security requirements; dynamic apps use HTTP/gRPC upstreams in the declared V1 profile.
- Arbitrary directives carried as opaque text into Rust activation.

The importer may preserve unsupported source text only for inspection or Nginx-target round-trip. Preserved text is marked `notExecutableByRust`, cannot contribute hidden behavior, and blocks Rust publication when it is in an active context.

## 5. Virtual Host Selection

For each effective listener, request host selection follows the declared Nginx profile:

1. Normalize the authority safely, including port removal, case handling, IDNA policy, and invalid-host rejection.
2. Prefer an exact `server_name` match.
3. Prefer the longest matching leading wildcard name.
4. Prefer the longest matching trailing wildcard name.
5. Evaluate regular expressions in authored order and select the first match.
6. Use the listener's single configured default server when no name matches.

The compiler must preserve distinctions between exact, wildcard, regex, empty, and default names. It must reject conflicting defaults, ambiguous normalized ownership, unsafe regex, and host values that would be interpreted differently across supported platforms. SNI certificate selection and HTTP host selection are validated together, but they remain distinct protocol decisions.

## 6. Location And Route Selection

Within the selected virtual host, path selection follows the supported Nginx location algorithm:

1. An exact location match wins immediately.
2. The engine remembers the longest matching prefix.
3. A matching `^~` prefix suppresses regex evaluation for that selection level.
4. Otherwise, regex locations are evaluated in effective authored order and the first match wins.
5. When no regex wins, the remembered longest prefix wins.

Nested locations, named locations, internal redirects, `try_files`, error pages, and rewrites must compile into an explicit bounded state machine that preserves the supported Nginx phase order. The compiler rejects cycles or a path whose maximum internal redirect/rewrite count cannot be bounded. SDKWork-only composite match conditions are not rendered as Nginx-compatible unless a renderer can prove equivalent behavior.

## 7. Variables, Regex, And URI Semantics

- The compatibility catalog declares a finite Nginx variable subset, its availability phase, mutability, escaping, and missing-value behavior.
- PCRE2 is used for configurations that request Nginx-compatible regular expressions. Compile time, match time, recursion/depth, capture count, and total regex count are bounded.
- URI parsing distinguishes raw request target, normalized path, query, decoded captures, filesystem path, and upstream URI. Percent decoding must occur only at the declared phase.
- Captures and variables are typed as tainted input. Redirects, headers, logs, paths, and upstream targets apply context-specific validation and escaping.
- Differences caused by Nginx build options, PCRE version, operating system, filesystem case sensitivity, or IDNA implementation are surfaced as compatibility constraints.

## 8. Proxy And Streaming Semantics

The compatibility profile defines:

- `proxy_pass` URI replacement for prefix, regex, named, and variable-bearing forms.
- Hop-by-hop header removal and explicit WebSocket upgrade handling.
- `Host`, `X-Forwarded-*`, and standardized `Forwarded` policy without trusting unapproved inbound proxy headers.
- Streaming request/response bodies, backpressure, cancellation, trailers where supported, SSE flushing, and bounded buffering.
- Connection reuse, upstream TLS SNI and verification, connect/read/write timeouts, and response timeout behavior.
- Retry eligibility by method, request commitment, failure type, attempt count, retry budget, and remaining deadline.

The engine must never buffer an unbounded body to emulate a directive. When exact compatibility would require unsafe or unbounded buffering, compilation fails or requires an explicit bounded policy with a documented behavioral grade.

## 8.1 HTTP Protocol Compatibility

The Nginx compatibility corpus includes strict HTTP/1.0/1.1 and HTTP/2 request/response behavior, not only route selection. It compares request-target parsing, Host/authority, header normalization, chunking, trailers, keep-alive, connection close, `Expect: 100-continue`, HEAD, no-body responses, range behavior, WebSocket upgrade, HTTP/2 pseudo-headers, flow control, and graceful shutdown.

Security differences are never relaxed to reproduce an unsafe reference behavior. Ambiguous `Content-Length`, `Transfer-Encoding` conflicts, obsolete header folding, invalid characters, request smuggling, oversized compressed headers, rapid reset, and other protocol-confusion inputs fail closed. An intentional security hardening difference is cataloged and tested rather than hidden behind the behavioral compatibility grade.

### 8.2 Current HTTP/1 Connection Evidence

[REQ-2026-0010](../requirements/REQ-2026-0010-http1-connection-semantics.md) is the first raw-socket HTTP/1 connection slice compared with a real Nginx binary. It proves equivalent default-server selection, ordered HTTP/1.0 Keep-Alive Pipeline handling, HTTP/1.0 Transfer-Encoding rejection, valid `100 Continue`, and complete Body half-close behavior for the tested fixtures.

The slice intentionally returns `417` for unsupported expectations and refuses to execute non-proxy actions after a truncated declared Body even where the reference Nginx `return` path ignores those inputs. Hyper 1.10.1 also preserves an HTTP/1.0 response status line for an HTTP/1.0 peer while Nginx 1.26.2 emits HTTP/1.1. These differences lower the current Behavioral grade and remain visible in the requirement matrix; they are not represented as complete Nginx compatibility.

### 8.3 Current HTTP/2 Abuse And Drain Evidence

[REQ-2026-0012](../requirements/REQ-2026-0012-bounded-http2-abuse-and-drain.md) proves an SDKWork safety slice over real TLS/ALPN H2 connections: configured SETTINGS, finite Frame/new-Stream/reset windows, encoded Header Block and Continuation bounds, H2 `ENHANCE_YOUR_CALM` local-error reset protection, isolated recovery, and graceful GOAWAY drain. These are runtime correctness and hardening results, not an Nginx Behavioral compatibility grade. A real Nginx HTTP/2 differential corpus, HPACK CPU corpus, and exhaustive malformed-Frame suite remain required before claiming HTTP/2 compatibility.

[REQ-2026-0014](../requirements/REQ-2026-0014-response-progress-timeouts.md) adds finite response producer-idle and downstream write-stall behavior with real Socket evidence. The connection write control is conceptually aligned with Nginx `send_timeout` progress semantics, but no Nginx differential fixture has yet established exact timing, buffering, status-line, or connection-close equivalence; the compatibility grade remains unchanged.

### 8.4 Current URI Normalization Evidence

[REQ-2026-0018](../requirements/REQ-2026-0018-canonical-uri-normalization.md) introduces separate raw and canonical URI representations. A pinned loopback Nginx 1.26.2 fixture confirms equivalent percent decoding, repeated-slash merging, dot-segment resolution, encoded-slash routing, trailing-slash preservation, and above-root rejection for the recorded corpus. Route selection and static identity use the canonical Path; no-rewrite proxying retains the original Path and Query, while `stripPrefix` rewrites the canonical Path and retains the original Query.

This evidence is draft, not a Behavioral compatibility grade. SDKWork intentionally returns `400` for a decoded backslash and invalid UTF-8, including where the tested Windows Nginx build treats backslash as a separator. ADR-20260716 remains proposed and requires human review before the requirement or compatibility behavior can be accepted.

### 8.5 Current HTTP/1 Pipeline Depth Evidence

[REQ-2026-0019](../requirements/REQ-2026-0019-bounded-http1-pipeline-depth.md) adds an SDKWork request-count ceiling in addition to the shared parser-byte ceiling. A real Nginx 1.26.2 loopback probe accepted 64 pipelined HTTP/1.1 requests in one write and emitted all 64 ordered `200` responses because Nginx exposes no equivalent pending-head count. SDKWork closes the connection when complete request heads awaiting Service dispatch exceed `http1MaxPipelineDepth`.

This is an intentional safety and scheduling-fairness difference, not a claimed Nginx Behavioral match. Accepted requests remain ordered, request Bodies are never buffered by the counter, TLS applies the limit after decryption, and H2 is outside its scope.

### 8.6 Current HTTP/2 Keep-Alive PING Evidence

[REQ-2026-0020](../requirements/REQ-2026-0020-http2-keep-alive-ping-timeout.md) adds proactive HTTP/2 liveness detection through Hyper/H2. The pinned Nginx 1.26.2 binary negotiated TLS ALPN `h2` but emitted no PING during the recorded 2,500 ms idle window; it emitted only SETTINGS, a connection WINDOW_UPDATE, and SETTINGS ACK. SDKWork with a 1,000 ms interval emitted PING and, when ACK was withheld for 300 ms, sent `GOAWAY(NO_ERROR)` and closed.

This is an operational difference scoped to the pinned interval and build, not evidence that every Nginx build can never emit PING. A compliant client that ACKs remains connected; healthy-idle maximum lifetime remains a separate policy.

## 9. Include And File Security

- Include paths resolve from an approved import root, not the process working directory.
- Absolute paths, parent traversal, symlink escape, device paths, alternate data streams, network shares, and include cycles are rejected unless an explicit administrative import policy permits a read-only source.
- Glob expansion is deterministic, sorted according to the declared profile, and bounded by file count, depth, individual size, and total bytes.
- Imported files retain canonical file identity, checksum, line/column source map, and include ancestry.
- Runtime static roots and certificate references are not dereferenced merely because an imported Nginx file names them; resource probing is a separate controlled stage.

## 10. Import, Normalize, Explain, And Render

The workflow is:

1. Parse all approved sources without activating them.
2. Resolve contexts, includes, inheritance, variables, and defaults.
3. Classify every directive as supported, conditionally supported, intentionally different, preserved-only, or unsupported.
4. Compile supported behavior to a canonical normalized intermediate representation.
5. Run schema, semantic, security, resource-budget, and compatibility validation.
6. Present source-mapped diagnostics, normalized diff, compatibility grades, and required operator decisions.
7. Render to the selected Nginx target or publish an immutable Rust snapshot only after all target-specific blockers are resolved.

Rendering is deterministic and idempotent: normalizing rendered output must reproduce the same canonical model. Generated files include provenance and checksums but never inline secret material unless written directly to an access-controlled runtime secret mount.

For the SDKWork Nginx deployment profile, rendering and deployment follow `NGINX_SPEC.md`:

- Linux site files use `/etc/nginx/sites-enabled/sdkwork/<domain>.conf` with the complete public domain as the filename stem.
- Certificate paths default to `/opt/certs/letsencrypt/live/<cert-name>/fullchain.pem` and `privkey.pem` when that protected-file profile is selected.
- Plan reports the canonical target, upstream, certificate references, checksum, validation command, reload action, and post-reload probes without writing.
- Render writes only to a staging target. Deploy uses an atomic file replacement with ownership/mode checks and retains a bounded previous artifact.
- Deployment runs the real selected Nginx binary's configuration test for the exact staged content, reloads through the declared service manager only after validation, and verifies public health/readiness and expected content/TLS behavior.
- A written file, zero exit from a wrapper that did not execute Nginx, sent reload signal, or database status change is not deployment success.

## 11. Validation And Failure Behavior

- Unknown or unsupported directives in an active Rust context are errors, not warnings.
- Wrong context, invalid arity, duplicate singleton directives, unresolved variables, missing references, conflicting listeners, and unsafe path or TLS dependencies are errors.
- A process exit code, parser return value, or control-plane database update alone is not proof of successful activation.
- Validation succeeds only when the intended engine completes its real validation path and returns verifiable evidence for the exact content checksum.
- Activation succeeds only after the target nodes report the intended revision and readiness probes verify served behavior.
- Failed imports, validation, rendering, reloads, and rollbacks return structured diagnostics and retain the last verified active revision.

## 12. Conformance Program

The repository must maintain a versioned corpus containing positive, negative, edge, security, and load fixtures. Each fixture records source configuration, requests, environment assumptions, expected normalized model, and comparison rules.

The test harness runs the same fixture against the selected stable Nginx OSS reference and the Rust engine and compares, as applicable:

- Listener and virtual-host selection.
- Location, rewrite, redirect, `try_files`, and error-page behavior.
- Status, headers, body bytes or documented semantic body comparison.
- Static path resolution, ranges, cache validators, MIME, and compression negotiation.
- Upstream request URI, headers, body streaming, retry, timeout, and WebSocket/SSE behavior.
- TLS protocol, ALPN, SNI certificate, client authentication, and failure alerts.
- Config validation, source diagnostics, reload, connection drain, and failure rollback.
- Bounded memory and latency under slow clients, large streams, upstream failure, and repeated reload.

Reference versions, build flags, kernel settings, certificates, fixture artifacts, and expected intentional differences are pinned in test metadata. A compatibility regression blocks release unless a reviewed profile-version change and migration note explicitly accepts it.

## 13. Observability And Commercial Support

- Request diagnostics identify the selected listener, host, route, upstream, configuration revision, and compatibility profile without exposing secrets or unbounded labels.
- Import and runtime metrics count directives by classification, validation failures, intentional differences, reload outcomes, and conformance regressions.
- Support bundles include redacted normalized configuration, checksums, diagnostic codes, versions, node convergence, and recent bounded events.
- Product documentation publishes a searchable directive support matrix, differences from Nginx, migration guide, version policy, and deprecation schedule.
- Compatibility claims in marketing, UI, APIs, and release notes must name the profile and evidence version.

## 14. Acceptance Criteria

- Every parsed directive receives a catalog classification and source-mapped diagnostic where applicable.
- No active unsupported behavior is silently ignored, approximated, or reported as deployed.
- Server-name, location, URI mapping, proxy streaming, static files, TLS, and reload conformance suites pass for the pinned Nginx reference.
- Import-normalize-render-normalize is idempotent for the supported corpus.
- Regex, include, rewrite, body, buffer, retry, connection, and configuration limits are enforced under adversarial tests without OOM.
- Existing accepted connections survive compatible reloads; invalid revisions never replace the last verified revision.
- The public support matrix and machine-readable catalog agree with implementation and conformance evidence.
