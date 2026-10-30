# Nginx Final-Gap Feasibility And Closing Sequence (`http-core-v1`) - 2026-10-30

| Field | Value |
| --- | --- |
| Report date | 2026-10-30 |
| Repository | `sdkwork-webserver`, branch `main`, HEAD `e43fa1c9d2cda8b1c24217cab72cb18cd9ca25c7` |
| Scope | final-gap feasibility and closing sequence for the **172 behavior-gap rows** and the **64 out-of-profile directives** remaining after the stream/OCSP wave |
| Compatibility profile | `http-core-v1` ([PRD-nginx-compatibility.md](../product/prd/PRD-nginx-compatibility.md) §2-§4) |
| Reference implementation | nginx 1.29.6, nginx.org official Linux package (the differential oracle build) |
| Companion reports | [2026-10-30-nginx-commercial-compatibility-assessment.md](2026-10-30-nginx-commercial-compatibility-assessment.md), [NGINX-COMPATIBILITY-MATRIX.md](../guides/operator/NGINX-COMPATIBILITY-MATRIX.md), [nginx-gap.catalog.json](../../specs/nginx-gap.catalog.json) |
| Directive inventory | [nginx-directive-inventory.json](../../specs/nginx-directive-inventory.json) - 744 directives; canonical-JSON sha256 `28c70ea443c01288e876d912ab7bdfa8933a8a8ffa3bbc7e5c74de6584f29d15` (catalog `inventory.digest`) |
| Support catalog | [nginx-directive-support.catalog.json](../../specs/nginx-directive-support.catalog.json) - 744 rows, sha256 `3669202ba3115739a2985fca95151836baf71e039f4979e220875c4d9017d2a2` |
| Audit ledger | [2026-10-30-nginx-directive-by-directive-audit.md](2026-10-30-nginx-directive-by-directive-audit.md) (744 rows, sha256 `85fd3f427304965921dd5d0a80671d47099948d3ca8ce7b2fb24eeaf90cfe63b`) |
| Method | read-only: catalog + audit recomputation, repo source inspection, pinned-crate source inspection under `~/.cargo/registry` |
| Not evidence | no behavioral gate was re-run for this report; every count below is derived from the committed catalog/audit, and the report claims no implementation the catalog does not claim |

## Status And Authority

- The source of truth for this report is the committed **catalog** (classification per
  directive) plus the generated **audit ledger** (operator-facing decision per
  directive). The report does not edit either artifact and does not reclassify any row;
  it recommends classification moves where the honest close is "do not implement",
  and names the owner action (catalog + matrix resync) for each.
- "Behavior gap" in this report is the audit decision `behavior-gap` (172 rows), which
  spans two catalog classifications: 10 `accepted-unenforced` rows and 162
  `unsupported-fail-closed` rows. "Out of profile" is the audit decision
  `out-of-profile` (64 rows, all catalog `excluded`).
- Effort classes are planning estimates, not commitments: **S** = one directive or a
  small mechanical mapping over an existing model field; **M** = one coherent wave
  slice (model + materializer + data-plane path + differential evidence, several files);
  **L** = a new engine or architecture change (subrequest executor, protocol adapter
  family, TLS-fetch owner); **XL** = a new transport/protocol subsystem (HTTP/3, mail
  listener plane).
- Every wave in the closing sequence keeps the release gates unchanged. No gate is
  lowered or skipped; each wave adds evidence and raises the manifest case floor.

## 0. Executive Summary

The remaining program is **finite and mostly ordinary work**, with a small, precisely
identifiable dependency-blocked tail:

| Backlog class | Rows | Meaning |
| --- | ---: | --- |
| Implementable in profile (execute the bounded form, fail closed on unsupported forms) | **145** | ordinary model + data-plane + differential work |
| Recommended "runtime-owned / documented difference" without new code | **20** | `*_temp_path`/`*_max_temp_file_size`/`*_temp_file_write_size`/`*_buffers`/`*_busy_buffers_size` (15, no disk temp spool), `open_file_cache*` (3, performance-only), `grpc_ssl_certificate_cache` + `uwsgi_ssl_certificate_cache` (2, config-time material cache) |
| Blocked by the pinned dependency stack | **5** | `chunked_transfer_encoding` (hyper HTTP/1 encoder), `ssl_stapling`/`ssl_stapling_verify` http+stream (rustls static `CertifiedKey.ocsp` only) |
| Not implementable on the pinned TLS stack (close as fail-closed) | **2** | `grpc_ssl_conf_command`, `uwsgi_ssl_conf_command` (OpenSSL command strings have no rustls equivalent) |
| **total behavior-gap** | **172** | |

Out-of-profile (64): the mail proxy (51) and GeoIP (8) are implementable **only after a
PRD §4 profile amendment** (L and M respectively); embedded Perl (4) and dynamic module
loading (1) are genuinely not implementable in this product. HTTP/3 (9 rows currently
`unsupported-fail-closed`, not part of the 172) is also a profile question: PRD §4 lists
HTTP/3 as excluded while the catalog rejects it as fail-closed, and a real HTTP/3 wave is
**XL**.

Recommended closing order (in-profile first, because it needs no requirement change):
**7A FastCGI residual -> 7B uWSGI residual -> 7C SCGI residual -> 7D memcached+gRPC
residual -> stream advanced -> subrequest engine -> module-sized modules -> HTTP
leftovers**, then the optional profile-expansion program (**mail wiring -> GeoIP ->
HTTP/3**). After the in-profile sequence the counter projection is **enforced 348 /
runtime-owned 163 / behavior-gap 7 / out-of-profile 64 / fail-closed 145 /
security-difference 17 = 744**, with the last 7 rows exactly the dependency-blocked
tail; after the Lead records those 7 as explicit decisions the audit's
"implementable backlog" is **zero**.

The single largest item is the **re-entrant subrequest engine** (7 rows, L); the largest
surface is the **FastCGI/uWSGI/SCGI/memcached residual toolkit** (7A-7D, 119 rows: two
M slices, one M/L slice and one S/M slice once the toolkit exists). The largest single risk is
`chunked_transfer_encoding` because it is the only backlog row that is *wire-visible and
pinned-blocked*; the report documents three workarounds and their fidelity costs.

## 1. Ground Truth (Recomputed, Read-Only)

### 1.1 Decision Counts (744 Rows)

Reproduced from the [generated audit ledger](2026-10-30-nginx-directive-by-directive-audit.md)
(section 1.1) and re-parsed row-by-row for this report:

| Decision | Count | Meaning |
| --- | ---: | --- |
| `enforced` | 203 | executed by the Rust data plane with committed evidence |
| `runtime-owned` | 143 | no client-visible effect; the runtime owns the subsystem by design |
| `security-difference` | 17 | intentionally stricter/safer than nginx, spec-justified |
| `behavior-gap` | **172** | client-visible nginx behavior not produced yet (planned backlog) |
| `out-of-profile` | **64** | excluded from the declared `http-core-v1` profile |
| `fail-closed` | 145 | rejected at materialization, not planned |
| **total** | **744** | every inventory row appears exactly once |

### 1.2 Catalog Classification x Audit Decision

| Catalog classification | Rows | `enforced` | `runtime-owned` | `security-difference` | `behavior-gap` | `out-of-profile` | `fail-closed` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `executable` | 203 | 203 | 0 | 0 | 0 | 0 | 0 |
| `accepted-unenforced` | 170 | 0 | 143 | 17 | 10 | 0 | 0 |
| `unsupported-fail-closed` | 307 | 0 | 0 | 0 | 162 | 0 | 145 |
| `excluded` | 64 | 0 | 0 | 0 | 0 | 64 | 0 |
| **total** | **744** | 203 | 143 | 17 | 172 | 64 | 145 |

The 10 `accepted-unenforced` behavior-gap rows are exactly: `chunked_transfer_encoding`,
`open_file_cache`, `open_file_cache_min_uses`, `open_file_cache_valid`,
`ssl_stapling`, `ssl_stapling_verify`, `ssl_trusted_certificate`,
`stream:ssl_stapling`, `stream:ssl_stapling_verify`, `stream:ssl_trusted_certificate`
(the accepted-but-ignored forms of an otherwise real feature); all other 162 gap rows
are rejected at load today.

### 1.3 The 172-Row Backlog, By Category (Exact Directives)

Categories and membership are the audit generator's `REMAINING_GAP_CATEGORIES` set
([tools/gen-nginx-directive-audit.mjs](../../tools/gen-nginx-directive-audit.mjs)), cross-checked
against the [support catalog](../../specs/nginx-directive-support.catalog.json); the audit
renderer emits the same set in the [audit ledger](2026-10-30-nginx-directive-by-directive-audit.md) §3.2.

| # | Category | Rows | Exact directives |
| --- | --- | ---: | --- |
| 1 | HTTP/1 framing policy | 1 | `chunked_transfer_encoding` |
| 2 | re-entrant subrequest engine | 7 | `add_before_body`, `add_after_body`, `addition_types`, `auth_request`, `auth_request_set`, `mirror`, `mirror_request_body` |
| 3 | module-sized functional modules | 17 | `userid`, `userid_domain`, `userid_expires`, `userid_flags`, `userid_mark`, `userid_name`, `userid_p3p`, `userid_path`, `userid_service`, `ancient_browser`, `ancient_browser_value`, `modern_browser`, `modern_browser_value`, `random_index`, `degradation`, `degrade`, `slice` |
| 4 | HTTP/2 DATA chunking | 1 | `http2_chunk_size` |
| 5 | static open-file cache | 3 | `open_file_cache`, `open_file_cache_min_uses`, `open_file_cache_valid` |
| 6 | OCSP dynamic fetch / refresh / verification | 6 | `ssl_stapling`, `ssl_stapling_verify`, `ssl_trusted_certificate`, `stream:ssl_stapling`, `stream:ssl_stapling_verify`, `stream:ssl_trusted_certificate` |
| 7 | stream advanced gaps | 18 | `stream:proxy_protocol_timeout`, `stream:pass`, `stream:server_name`, `stream:proxy_bind`, `stream:proxy_half_close`, `stream:proxy_next_upstream_timeout`, `stream:proxy_next_upstream_tries`, `stream:proxy_requests`, `stream:proxy_responses`, `stream:proxy_download_rate`, `stream:proxy_upload_rate`, `stream:proxy_ssl`, `stream:proxy_ssl_certificate`, `stream:proxy_ssl_certificate_key`, `stream:proxy_ssl_name`, `stream:proxy_ssl_server_name`, `stream:proxy_ssl_verify`, `stream:proxy_ssl_trusted_certificate` |
| 8 | gRPC residual forms | 9 | `grpc_bind`, `grpc_ignore_headers`, `grpc_socket_keepalive`, `grpc_ssl_certificate_cache`, `grpc_ssl_ciphers`, `grpc_ssl_conf_command`, `grpc_ssl_crl`, `grpc_ssl_password_file`, `grpc_ssl_session_reuse` |
| 9 | FastCGI residual forms | 32 | `fastcgi_bind`, `fastcgi_buffer_size`, `fastcgi_buffers`, `fastcgi_busy_buffers_size`, `fastcgi_cache`, `fastcgi_cache_background_update`, `fastcgi_cache_bypass`, `fastcgi_cache_key`, `fastcgi_cache_lock`, `fastcgi_cache_lock_age`, `fastcgi_cache_lock_timeout`, `fastcgi_cache_max_range_offset`, `fastcgi_cache_methods`, `fastcgi_cache_min_uses`, `fastcgi_cache_path`, `fastcgi_cache_revalidate`, `fastcgi_cache_use_stale`, `fastcgi_cache_valid`, `fastcgi_catch_stderr`, `fastcgi_force_ranges`, `fastcgi_ignore_client_abort`, `fastcgi_ignore_headers`, `fastcgi_limit_rate`, `fastcgi_max_temp_file_size`, `fastcgi_no_cache`, `fastcgi_pass_request_body`, `fastcgi_send_lowat`, `fastcgi_socket_keepalive`, `fastcgi_store`, `fastcgi_store_access`, `fastcgi_temp_file_write_size`, `fastcgi_temp_path` |
| 10 | uWSGI residual forms | 44 | `uwsgi_bind`, `uwsgi_buffer_size`, `uwsgi_buffers`, `uwsgi_busy_buffers_size`, `uwsgi_cache`, `uwsgi_cache_background_update`, `uwsgi_cache_bypass`, `uwsgi_cache_key`, `uwsgi_cache_lock`, `uwsgi_cache_lock_age`, `uwsgi_cache_lock_timeout`, `uwsgi_cache_max_range_offset`, `uwsgi_cache_methods`, `uwsgi_cache_min_uses`, `uwsgi_cache_path`, `uwsgi_cache_revalidate`, `uwsgi_cache_use_stale`, `uwsgi_cache_valid`, `uwsgi_force_ranges`, `uwsgi_ignore_client_abort`, `uwsgi_ignore_headers`, `uwsgi_limit_rate`, `uwsgi_max_temp_file_size`, `uwsgi_no_cache`, `uwsgi_pass_request_body`, `uwsgi_socket_keepalive`, `uwsgi_ssl_certificate`, `uwsgi_ssl_certificate_cache`, `uwsgi_ssl_certificate_key`, `uwsgi_ssl_ciphers`, `uwsgi_ssl_conf_command`, `uwsgi_ssl_crl`, `uwsgi_ssl_name`, `uwsgi_ssl_password_file`, `uwsgi_ssl_protocols`, `uwsgi_ssl_server_name`, `uwsgi_ssl_session_reuse`, `uwsgi_ssl_trusted_certificate`, `uwsgi_ssl_verify`, `uwsgi_ssl_verify_depth`, `uwsgi_store`, `uwsgi_store_access`, `uwsgi_temp_file_write_size`, `uwsgi_temp_path` |
| 11 | SCGI residual forms | 30 | `scgi_bind`, `scgi_buffer_size`, `scgi_buffers`, `scgi_busy_buffers_size`, `scgi_cache`, `scgi_cache_background_update`, `scgi_cache_bypass`, `scgi_cache_key`, `scgi_cache_lock`, `scgi_cache_lock_age`, `scgi_cache_lock_timeout`, `scgi_cache_max_range_offset`, `scgi_cache_methods`, `scgi_cache_min_uses`, `scgi_cache_path`, `scgi_cache_revalidate`, `scgi_cache_use_stale`, `scgi_cache_valid`, `scgi_force_ranges`, `scgi_ignore_client_abort`, `scgi_ignore_headers`, `scgi_limit_rate`, `scgi_max_temp_file_size`, `scgi_no_cache`, `scgi_pass_request_body`, `scgi_socket_keepalive`, `scgi_store`, `scgi_store_access`, `scgi_temp_file_write_size`, `scgi_temp_path` |
| 12 | memcached residual forms | 4 | `memcached_bind`, `memcached_buffer_size`, `memcached_gzip_flag`, `memcached_socket_keepalive` |
| | **total** | **172** | |

### 1.4 The 64 Out-Of-Profile Directives (Exact)

All 64 carry catalog classification `excluded` and PRD §4 as authority. They are listed
here by module; the [audit ledger](2026-10-30-nginx-directive-by-directive-audit.md)
§2 (`mail` group and the two excluded HTTP modules) is authoritative for each row.

| Group | Module | Rows | Directives |
| --- | --- | ---: | --- |
| mail | `ngx_mail_module` | 1 | `mail` |
| mail | `ngx_mail_core_module` | 9 | `error_log`, `listen`, `max_errors`, `protocol`, `resolver`, `resolver_timeout`, `server`, `server_name`, `timeout` |
| mail | `ngx_mail_ssl_module` | 20 | `ssl_certificate`, `ssl_certificate_compression`, `ssl_certificate_key`, `ssl_ciphers`, `ssl_client_certificate`, `ssl_conf_command`, `ssl_crl`, `ssl_dhparam`, `ssl_ecdh_curve`, `ssl_password_file`, `ssl_prefer_server_ciphers`, `ssl_protocols`, `ssl_session_cache`, `ssl_session_ticket_key`, `ssl_session_tickets`, `ssl_session_timeout`, `ssl_trusted_certificate`, `ssl_verify_client`, `ssl_verify_depth`, `starttls` |
| mail | `ngx_mail_auth_http_module` | 4 | `auth_http`, `auth_http_header`, `auth_http_pass_client_cert`, `auth_http_timeout` |
| mail | `ngx_mail_proxy_module` | 7 | `proxy_buffer`, `proxy_pass_error_message`, `proxy_protocol`, `proxy_smtp_auth`, `proxy_timeout`, `proxy_xclient`, `xclient` |
| mail | `ngx_mail_smtp_module` | 4 | `smtp_auth`, `smtp_capabilities`, `smtp_client_buffer`, `smtp_greeting_delay` |
| mail | `ngx_mail_imap_module` | 3 | `imap_auth`, `imap_capabilities`, `imap_client_buffer` |
| mail | `ngx_mail_pop3_module` | 2 | `pop3_auth`, `pop3_capabilities` |
| mail | `ngx_mail_realip_module` | 1 | `set_real_ip_from` (mail context) |
| http | `ngx_http_geoip_module` | 5 | `geoip_city`, `geoip_country`, `geoip_org`, `geoip_proxy`, `geoip_proxy_recursive` |
| stream | `ngx_stream_geoip_module` | 3 | `geoip_city`, `geoip_country`, `geoip_org` |
| http | `ngx_http_perl_module` | 4 | `perl`, `perl_modules`, `perl_require`, `perl_set` |
| core | `ngx_core_module` | 1 | `load_module` |
| | **total** | **64** | |

### 1.5 Context Rows (Not Backlog, For Completeness)

- **145 `fail-closed`**: rejected at materialization and *not planned* in the declared
  profile. They include the content-transform module families (`image_filter`, `ssi`,
  `xslt`, `mp4`, `flv`, `gunzip`, `early_hints`), the remaining HTTP `rewrite`
  internals (`if`, `break`, `rewrite_log`, `uninitialized_variable_warn`), `limit_except`,
  `satisfy`, `aio`, `post_action`, `limit_rate`, most `proxy_*` cache-lock/store/SSL
  knobs (31 rows), HTTP/2 `http3/QUIC` (8+1), and stream `ssl_certificate_cache/crl/
  password_file/conf_command/ocsp/ech/alpn/handshake_timeout/reject_handshake/session_ticket_key/
  stapling_responder` (14). Several of these are technically implementable; they are
  outside the declared required surface, so promoting any of them is a **requirement
  decision**, not a missing-code decision. Full row list: audit ledger §2.
- **143 `runtime-owned`**: no client-visible behavior (process/event-loop/buffer/TLS-
  default/logging/upstream-policy ownership). Not work.
- **17 `security-difference`**: intentionally stricter (e.g. hop-by-hop stripping where
  nginx 1.29.6 forwards, `nosniff` default). Not work; keep the difference documented.

### 1.6 Findings From The Recomputation

- **F1 (latent silent-drop candidate, code defect class).** The typed config surface
  declares source-address bind and socket keep-alive on the CGI-family resources:
  `bindAddress`/`socketKeepalive` are in the config schema
  ([config schema](../../specs/sdkwork.webserver.config.schema.json) lines 1451/1456, 1541/1546, 1693/1698)
  and in the model ([model.rs](../../crates/sdkwork-webserver-core/src/config/model.rs)
  `FastCgi`/`Memcached`/`CgiUpstream` `bind_address`/`socket_keepalive`, lines
  1495/1499, 1541/1545, 1579/1583), and validation reasons about them for unix targets
  ([validate.rs](../../crates/sdkwork-webserver-core/src/config/validate.rs), the `bindAddress`/unix check),
  **but no adapter consumes them** ([cgi_upstream.rs](../../crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cgi_upstream.rs)
  never reads `bind_address`/`socket_keepalive`; the only consumer of `bind_address` in
  the gateway is the unrelated provider-event ingress). This is the same defect class as
  the two manifest defects fixed in Wave 2 (`proxyConnectTimeout*`, `chainFile`): a
  declared key that is silently dropped. The 7A/7D waves must either consume the fields
  (`socket2` is already a gateway dependency) or remove them from schema/model and fail
  closed. **Worktree note:** while this report was being written, the in-flight 7A code
  wave was observed wiring `fastcgi_bind`/`fastcgi_socket_keepalive` through a shared
  connector (`cgi_upstream.rs` + `mapping.rs`), i.e. exactly the recommended resolution;
  re-verify at commit and apply the same wiring to `uwsgi_*`/`scgi_*`/`memcached_*`/`grpc_*`
  in their waves.
- **F2 (decision consistency across protocol families).** Equivalent nginx semantics are
  classified differently depending on the module: `proxy_bind`, `proxy_socket_keepalive`,
  `proxy_limit_rate`, `proxy_cache_lock*`, `proxy_ignore_headers`, `proxy_store*` are
  *fail-closed* (not planned), while the same sub-family under
  `fastcgi_*`/`uwsgi_*`/`scgi_*`/`grpc_*`/`memcached_*` is *backlog*; and
  `proxy_temp_path` is *accepted-unenforced* while `fastcgi_temp_path`/`uwsgi_temp_path`/
  `scgi_temp_path` are *rejected*. Closing 7A-7D without aligning these decisions will
  re-create the same inconsistency in the opposite direction. Recommendation: the
  residual toolkit defines one decision per semantic class (prefer the HTTP-proxy
  precedent) and the Lead re-syncs the catalog/audit once, rather than letting each
  family choose.
- **F3 (profile-boundary drift, HTTP/3).** PRD §4 lists "Mail proxy and HTTP/3 (QUIC)"
  as excluded, and the catalog marks mail `excluded` (51), but all 8 `ngx_http_v3_module`
  rows plus `events:quic_bpf` are `unsupported-fail-closed` (9), i.e. the same profile
  exclusion is encoded as two different decisions. Before an HTTP/3 wave this must be
  made consistent, otherwise the counter movement is unpredictable.
- **F4 (gap-catalog note lag).** The [nginx-gap.catalog.json](../../specs/nginx-gap.catalog.json) notes still quote earlier
  counts (e.g. "203 executable / 171 accepted-unenforced / 306 fail-closed / 64 excluded")
  while the committed catalog is 203/170/307/64; and `tls.ocsp-*` notes pre-date the
  static-stapling wave (the static `ssl_stapling_file` form executes, gap catalog row
  `tls.ocsp-static-stapling` is `partial`). The Lead-owned catalog regeneration should
  refresh these notes; this report does not edit them.
- **F5 (mail substrate state).** [sdkwork-webserver-mail-protocol](../../crates/sdkwork-webserver-mail-protocol) is present and
  green (45 tests: 37 lib + 8 integration) with POP3/IMAP/SMTP dialog state machines,
  SASL PLAIN/LOGIN, STARTTLS boundaries, dot-stuffing, XCLIENT/PROXY v1, frontend
  transition table and backend relay helpers. There is **no** mail listener plane in the
  gateway data plane (`data_plane/mod.rs` has no mail module), and no mail model /
  materializer. That is exactly the wiring gap.
- **F6 (evidence grade).** All completed waves are backed by the differential harnesses
  and/or integration tests; 82 executable rows were spot-checked by the audit. New waves
  must cite **behavioral** evidence (a differential case or a wire/integration test),
  not only "materializes into a model field", or the audit will record a weak citation.

## 2. Feasibility By Backlog Category

### 2.1 HTTP/1 Framing Policy - `chunked_transfer_encoding` (1, class L)

**What nginx does.** With `chunked_transfer_encoding off` (http/server/location), a
response whose body length is unknown is sent with `Connection: close` and no
`Transfer-Encoding: chunked` (close-delimited body, HTTP/1.1 status line kept); the
connection cannot be reused.

**Current state.** `accepted-unenforced` with a truthful diagnostic: the value has no
effect because the pinned hyper HTTP/1.1 encoder chooses the framing.

**Feasibility analysis (pinned dependency, verified in source).**
`hyper` 1.12.0 `proto/h1/role.rs` (Server `encode`) picks the encoder as follows: an
unknown-length response is `Encoder::close_delimited()` **only** when
`msg.head.version == HTTP_10` or the method/status cannot be chunked (HEAD/1xx/204/304);
otherwise it unconditionally writes `transfer-encoding: chunked`. There is no public
builder knob for close-delimited HTTP/1.1 framing. Three workarounds exist, none of them
faithful within the current stack:

1. **Bounded buffering to synthesize `Content-Length`.** Works when the body fits the
   configured buffer ceiling; a streaming/oversized body would still have to chunk. This
   is an opt-in partial behavior, not parity, and it contradicts the runtime's streaming
   proxy model.
2. **Response version HTTP/1.0.** Setting the response version to `HTTP_10` makes hyper
   use `close_delimited`, but the status line becomes `HTTP/1.0`, which is a wire-visible
   difference from nginx (nginx answers `HTTP/1.1` + `Connection: close`).
3. **Patch/vendor hyper** (or replace the h1 response path). A small `role.rs` change can
   honor an explicit "close-delimited" marker; carrying a vendored hyper via
   `[patch.crates-io]` is a supply-chain and maintenance cost (security updates, audit
   evidence, Cargo.lock pinning), and a bespoke h1 response writer means owning response
   framing/keep-alive semantics outside hyper.

**Verdict.** Blocked by the pinned dependency for faithful parity. Recommended close:
keep the accepted-unenforced diagnostic, state the limitation in the operator matrix, and
track an upstream hyper feature request (opt-in close-delimited HTTP/1.1 response). If
the Lead wants a product answer now, the bounded-buffering partial (1) is the only
defensible one; do not ship the HTTP/1.0 trick. **Effort: L** (pin only; S upstream patch
+ L vendoring/maintenance if chosen).

### 2.2 Re-entrant Subrequest Engine - 7 Rows (class L, largest single item)

`add_before_body`, `add_after_body`, `addition_types`, `auth_request`,
`auth_request_set`, `mirror`, `mirror_request_body`.

**What nginx does.** A filter/access phase issues a *subrequest*: an internal request
that re-runs location selection and the content phase for another URI (same server
block), whose response is either concatenated into the main body (addition), used as an
authorization decision (auth_request: 2xx allow, 401/403 deny, other -> 500), or
fire-and-forget (mirror). `auth_request_set` copies a variable from the auth subrequest
response (e.g. `$upstream_http_x_user`) into the main request; `mirror_request_body on`
tees the client body to the mirror.

**Why it is not just another handler.** The current engine performs only *internal
redirects* (rewrite `last`, error_page, one-hop named locations), all of which continue
the same request through the existing content dispatch. A subrequest is a **nested
request**: it needs its own route selection, its own headers/method/body, its own
response, and its own bounded lifecycle, while sharing the client connection, the config
generation, and the outer request's identity/logging.

**Bounded design sketch.**
- Extract the dispatch core of `route_admitted_request` in
  [handler.rs](../../crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/handler.rs) into an
  internal request executor that takes a synthetic `http::Request<Body>`, the selected
  config generation, the scheme/authority/client facts, and a depth/cycle token — no
  socket, no connection permits.
- Budgets: max subrequest depth (nginx allows nesting; the product implementation should
  allow depth 1 and reject deeper declarations/uses with a located diagnostic), max
  subrequests per main request, per-subrequest timeout, bounded body buffers for
  addition/mirror (reuse `limits.max_request_body_bytes` and add a response-side bound).
- Phase semantics to specify and test: whether location gates (`allow`/`deny`,
  `auth_basic`, `limit_req`, `limit_conn`) and `add_header` apply to the subrequest and
  to the main response (nginx runs the access/rewrite phases for subrequests in its own
  way; mirror/auth_request semantics differ). This spec must be written before code.
- `auth_request`: subrequest response status -> main response mapping, `WWW-Authenticate`
  forwarding, `auth_request_set` variable table (a small named result map threaded into
  the response-variable expansion the runtime already has for `add_header`/`return`).
- `mirror`: fire-and-forget with a bounded queue and a hard timeout; body tee is the
  only place `mirror_request_body on` differs from `off`.
- Additions: body concatenation requires the response body to be read to the end within
  the bound; interacts with gzip/range (nginx applies the addition filter before gzip, so
  content-length/ETag semantics change) — the wave must decide and test the interaction
  matrix (static, proxy, return bodies).

**Dependencies.** No new crates. Requires the dispatch refactor, a subrequest budget type
in `config/model.rs` (declaration validation), mapping for the seven directives, and a
new differential slice (`tests/nginx/differential-subrequest/` + manifest slice +
orchestrator) proving addition/auth/mirror against nginx 1.29.6, plus depth/cycle unit
tests. **Effort: L** (one wave, mostly `handler.rs` refactor + filter pipeline).

### 2.3 Module-Sized Functional Modules - 17 Rows (class M, `slice` is L)

| Module | Rows | What it needs | Effort |
| --- | ---: | --- | --- |
| `userid` | 9 | `$uid_got`/`$uid_set` variables, `uid` cookie generation (nginx: base64 of pid/start time/counter, `service`/`path`/`domain`/`expires`/`p3p`/`mark`/`flags`), response filter that sets the cookie and exposes the variables | M |
| `browser` | 4 | `$modern_browser`/`$ancient_browser` UA classification from authored `modern_browser`/`ancient_browser` version lists (regex/family grammar), result variables consumed by existing variable expansion | M |
| `random_index` | 1 | location handler: when the URI maps to a directory, pick a random regular file instead of `index` (bounded directory read; nginx sorts/indexes and picks randomly) | S |
| `degradation` | 2 | `$degradation` variable and `degrade` handler triggered by an allocation threshold. nginx measures its own pool allocation; the Rust runtime must map this onto an explicit, documented memory-pressure signal (the existing `resource_pressure` controller is the natural source) and accept that the trigger metric differs | M (documented difference) |
| `slice` | 1 | `slice <size>` splits a proxied response into cached range subrequests (`$slice_range`, cache key includes the range); requires the cache engine + range engine + upstream request coalescing. This is a cache feature, not a small module | L |

**Dependencies.** `userid` needs a response-filter hook and a per-process cookie
signing/identity policy (nginx's cookie is not signed; the runtime should at least keep
the same wire format). `browser` needs the variable vocabulary extended and a UA parser
(bounded; no new crate strictly required). `degradation` needs a policy decision on the
pressure signal. `slice` depends on the proxy cache semantics being extended (overlaps
with 7A cache toolkit). **Effort: M together (17 rows, one or two waves), with `slice`
as a separate L if required.**

### 2.4 HTTP/2 DATA Chunking - `http2_chunk_size` (1, class M)

**What nginx does.** Caps the size of body chunks sliced into HTTP/2 DATA frames
(default 8k); a smaller value raises frame count/overhead.

**Current catalog state.** `unsupported-fail-closed`, described as "hyper/h2 API gap".

**Feasibility analysis (verified in pinned source).** There is no direct knob: the pinned
`hyper` 1.12.0 h2 server forwards each body frame chunk through
`h2::SendStream::send_data` (`proto/h2/mod.rs`, `PipeToSendStream::poll`), and `h2`
0.4.20 splits a chunk further only for peer `SETTINGS_MAX_FRAME_SIZE`/flow control. That
means a **response-body adapter that yields at most N bytes per data frame** realizes
nginx's observable contract (DATA-frame size <= `http2_chunk_size`). h2 may split a chunk
further; it does not merge distinct `send_data` calls in the server path. Residual risk:
the exact frame-size distribution must be proven by a raw HTTP/2 wire test (the repo has
[http2_tuning.rs](../../crates/sdkwork-api-webserver-standalone-gateway/tests/http2_tuning.rs) for SETTINGS; a
DATA-frame test needs a small frame-capturing client).

**Verdict.** Implementable without an upstream change; the catalog's "blocked" wording
should be revised to "no direct knob; body-sizing adapter executes the bounded
semantics". **Effort: M** (body adapter + wire test + config mapping).

### 2.5 Static Open-File Cache - 3 Rows (class S; recommend runtime-owned)

`open_file_cache`, `open_file_cache_min_uses`, `open_file_cache_valid`.

**What nginx does.** Caches open file descriptors, file metadata and directory listings
so a hot static path avoids `open`/`stat`. The contract is deliberately staleness-
tolerant (nginx documents that cached errors/metadata may be stale; `open_file_cache`
is a performance cache, not a correctness feature). The response bytes are unchanged.

**Verdict.** No client-visible behavior; the runtime already owns the open policy and its
file path walk (`data_plane/static_path.rs`). Implementing an fd cache is possible (S:
LRU keyed by device+inode with validity/min-uses), but it adds unsafe-ish OS-state
management and memory for a tuning-only gap. **Recommendation: keep runtime-owned,
document the difference in the operator matrix, and move the three rows out of the
backlog by decision (Lead catalog action), not by new code.** If implemented anyway:
S, and it must never cache errors or serve differently from a fresh `open`.

### 2.6 OCSP Dynamic Fetch / Refresh / Verification - 6 Rows (class L; recommend defer)

`ssl_stapling`, `ssl_stapling_verify`, `ssl_trusted_certificate` (http) and the three
`stream:` rows.

**What already works.** Static stapling via `ssl_stapling_file`: the DER response is
validated, carried into the certificate source and fed to rustls
`CertifiedKey.ocsp`, proven on the wire by
[tls_ocsp_stapling.rs](../../crates/sdkwork-api-webserver-standalone-gateway/tests/tls_ocsp_stapling.rs)
(gap-catalog row `tls.ocsp-static-stapling` is `partial`).

**Why dynamic is not there.** rustls 0.23.46 exposes `CertifiedKey { cert, key, ocsp }`
(`crypto/signer.rs`) — a static byte field. There is **no fetch, refresh, responder
selection or OCSP-response verification API** in rustls (CRLs are supported for client
and server verification via `with_crls`, but OCSP is not). A dynamic implementation
needs: an HTTP fetch owner (reqwest is already a workspace dependency), OCSP response
parsing/verification (a new crate, e.g. an `x509-ocsp` implementation; none is in the
tree), a refresh schedule, a key-swap path (the layered certificate resolver supports
atomic replacement, so no reload is strictly required), and a fallback policy for
`ssl_stapling_verify on` (verify or serve without stapling).

`ssl_trusted_certificate` is separate and cheap: it is the trust store for client-cert
verification and for OCSP verification. Wiring it into `ClientAuthConfig.caCertificateFiles`
(both http and stream policies) reuses the cert loader and can execute as a bounded M
item, independent of dynamic stapling. It is also the typed plane's `chainFile`
counterpart already consumed by the certificate materializer.

**Industry context.** Let's Encrypt ended its OCSP service in 2025 and moved to CRL/ARI;
the declared fleet uses short-lived ACME certificates; nginx defaults `ssl_stapling off`.
The client-visible value of dynamic stapling in this product is therefore bounded and
shrinking.

**Verdict.** `ssl_trusted_certificate` (http+stream): **M, implement**. Dynamic
fetch/refresh/verify (4 rows): **L**, technically feasible only with a new verifier
dependency and a refresh owner; recommended close as a documented intentional difference
(declaration remains accepted-unenforced with the precise blocker diagnostic), not as
code work. **Effort: M + defer.**

### 2.7 Stream Advanced Gaps - 18 Rows (class M, `pass`/`server_name` push it to L)

| Sub-family | Rows | Path | Effort |
| --- | ---: | --- | --- |
| Upstream TLS client | 7 (`proxy_ssl`, `proxy_ssl_certificate`, `proxy_ssl_certificate_key`, `proxy_ssl_name`, `proxy_ssl_server_name`, `proxy_ssl_verify`, `proxy_ssl_trusted_certificate`) | Reuse [upstream_tls.rs](../../crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/upstream_tls.rs) (`build_upstream_tls_config`, already used by HTTP proxying) and `cgi_upstream.rs`; add stream upstream TLS fields to `StreamServerConfig`/`StreamTargetConfig` and a `tokio-rustls` client handshake in `stream_proxy.rs`. Keep the runtime's always-verified default unless the operator explicitly disables verification (documented stricter-than-nginx difference where nginx defaults `verify off`). | M |
| Transport controls | 5 (`proxy_bind`, `proxy_half_close`, `proxy_next_upstream_timeout`, `proxy_next_upstream_tries`, `proxy_protocol_timeout`) | `proxy_bind`/`SO_KEEPALIVE` via `socket2` (already a gateway dependency); half-close via `TcpStream::shutdown` after EOF on one direction; retry budget reuses the existing HTTP `UpstreamRetryConfig` model; `proxy_protocol_timeout` bounds the inbound PROXY-header read (the HTTP listener plane already owns an inbound PROXY parser — mirror it for stream listeners). | S/M |
| Rate/counting | 4 (`proxy_download_rate`, `proxy_upload_rate`, `proxy_requests`, `proxy_responses`) | Token-bucket throttling in the stream copy loop (reuse `limit_req` math) and response/request counters exposed to the existing stream metrics (`sdkwork-api-webserver-standalone-gateway/src/data_plane/metrics.rs`); nginx's `proxy_requests`/`proxy_responses` gate connection lifetime. | S/M |
| Routing | 2 (`pass`, `server_name`) | `server_name` makes a stream listener select a virtual server by SNI/preread value — the model currently materializes one stream server per listen; needs a per-listener stream server table plus preread-based selection. `pass <addr>` hands a connection to another stream server's address (nginx 1.25.5 `ngx_stream_pass_module`); implement as an internal dispatch to the target listener's TCP socket (bounded, no loop) or a direct in-process handoff. These two are the only structural items. | L |

**Dependencies.** No new crates for the transport/rate families. `server_name`/`pass`
need a model/selection design and new [differential-stream](../../tests/nginx/differential-stream) cases; the TLS
family needs stream client-identity configuration. Evidence: extend the existing
[differential-stream](../../tests/nginx/differential-stream) slice (currently 9/9) and `tests/stream_proxy.rs` +
`tests/tls_ocsp_stapling.rs` patterns. **Effort: M for 16 rows + L for the 2 routing
rows; plan one L wave.**

### 2.8 Protocol Residual Families - 119 Rows (Waves 7A-7D)

This is the largest surface (110 of the 119 rows are the four CGI/memcached families;
9 are gRPC). The queued **7A-7D** split referenced by the Lead is pinned here explicitly
so the counter effects are computable:

| Wave | Family | Rows | Notes |
| --- | --- | ---: | --- |
| 7A | FastCGI residual | 32 | reference family; builds the shared toolkit |
| 7B | uWSGI residual | 44 | largest; reuses toolkit **and** the upstream TLS builder (uWSGI is the only CGI family with an upstream TLS family) |
| 7C | SCGI residual | 30 | toolkit reuse only |
| 7D | memcached + gRPC residual | 13 (4 + 9) | small socket/response knobs + gRPC TLS-library tail |
| | **total** | **119** | |

#### 2.8.1 The Shared Residual Toolkit (build once in 7A)

The same semantic classes repeat across the families (the TLS-library class only exists
in uWSGI and gRPC); implementing them per family would quadruple the work and the review
surface. Build one toolkit in 7A, then map it into 7B-7D:

| Toolkit piece | Rows per family | Implementation path | Effort |
| --- | --- | --- | --- |
| Socket transport: `*_bind`, `*_socket_keepalive`, `*_send_lowat` | 2-3 | `socket2` already declared in the gateway; the model fields (`bind_address`, `socket_keepalive`) exist but are unconsumed — wire them in the shared connector (see F1) | S |
| Header/response buffer size: `*_buffer_size` | 1 | bounded header/body read buffer in the adapter; the adapters already buffer bounded responses | S |
| Cache policy: `*_cache*` (15 per big family: valid/path/key/methods/min_uses/bypass/no_cache/use_stale/revalidate/background_update/lock*/max_range_offset) | 15 | Reuse and generalize the existing shared proxy cache (`crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cache/` + `ProxyCacheConfig` in the model) for the CGI response stream; per-directive knobs are incremental once the adapter is cache-capable. `lock`/`background_update`/`stale` add a single-flight + refresh policy. | M/L |
| Store-to-root: `*_store`, `*_store_access` | 2 | Bounded async write of the upstream response into the static root path mapped from the request URI (nginx `store`), with no-symlink/no-`..` writes, atomic temp-file-then-rename, and `store_access` -> file mode mapping. Security-reviewed path; client-visible (serves future requests from the stored file). | M |
| Response/request knobs: `*_ignore_headers`, `*_ignore_client_abort`, `*_pass_request_body`, `*_force_ranges`, `*_limit_rate`, `*_catch_stderr` | 5-6 | `ignore_headers` reuses `ProxyResponsePolicyConfig`; `limit_rate` reuses the stream/limit token bucket; `pass_request_body`/`ignore_client_abort` are flags in the adapter/retry path; `force_ranges` toggles range processing on a buffered response; `catch_stderr` only matters for a process-backed FastCGI (the runtime is a protocol client, so "stderr" has no channel — close as documented difference if promoted). | S/M |
| Retry: `*_next_upstream_timeout`, `*_next_upstream_tries` | 0-2 | The adapters already use the HTTP upstream retry policy; these two map onto bounded retry budget/timeout fields. | S |
| Temp-spool buffering: `*_buffers`, `*_busy_buffers_size`, `*_max_temp_file_size`, `*_temp_file_write_size`, `*_temp_path` | 5 | **Not implementable as nginx semantics by design.** The runtime streams/buffers bounded responses and has no disk temp spool; adding one would own disk-exhaustion/cleanup semantics for a tuning knob with no byte-level client-visible effect. Recommended close as documented difference (runtime-owned), not code. | 0 (decision) |
| TLS library knobs (uWSGI and gRPC only): `*_ssl_ciphers`, `*_ssl_conf_command`, `*_ssl_crl`, `*_ssl_password_file`, `*_ssl_session_reuse`, `*_ssl_certificate_cache`, `uwsgi_ssl_protocols` | 6-7 | See §2.8.2; most are implementable (rustls `CryptoProvider` cipher list, `with_crls`, resumption, `pkcs8` decryption for password files, TLS window for protocols); `ssl_conf_command` is not. | M with one blocked row per family |

#### 2.8.2 Per-Wave Assignment And Verdict

| Wave | Executable after wave | Recommended documented-difference moves | Remaining blocked | Effort |
| --- | ---: | ---: | ---: | --- |
| 7A FastCGI (32) | 27 (cache 15, socket 3, filter 6, buffer 1, store 2) | 5 spool rows | 0 | M |
| 7B uWSGI (44) | 37 (7A toolkit + TLS identity 7 + TLS-library 5 of 7 + `uwsgi_ssl_protocols`) | 5 spool + `uwsgi_ssl_certificate_cache` (runtime-owned) | 1 (`uwsgi_ssl_conf_command`) | M/L |
| 7C SCGI (30) | 25 (same as 7A) | 5 spool rows | 0 | M |
| 7D memcached+gRPC (13) | 11 (memcached 4; gRPC bind/keepalive/ignore_headers + crl/password_file/session_reuse/ciphers 7) | `grpc_ssl_certificate_cache` (runtime-owned) | 1 (`grpc_ssl_conf_command`) | S/M |
| **totals** | **100** | **17** | **2** | |

Evidence: extend the existing [differential-protocols](../../tests/nginx/differential-protocols) slice (39/39 today;
FastCGI/uWSGI/SCGI/memcached/gRPC) with cache/store/bind/keepalive/TLS cases, add unit
tests for cache key/method/range behavior and for the unconsumed-field defect (F1), and
raise the manifest case floor.

#### 2.8.3 Why The TLS-Library Rows Are Feasible (Correcting "rustls blocks them")

The earlier shorthand "rustls cannot do cipher/CRL knobs" is only partly true; checked
against rustls 0.23.46:

| Row | rustls capability | Verdict |
| --- | --- | --- |
| `*_ssl_ciphers` | `CryptoProvider.cipher_suites` is configurable (TLS 1.2 suites selectable; TLS 1.3 suites are the three fixed ones). OpenSSL cipher-string grammar (`HIGH:!aNULL:!MD5:@SECLEVEL`) has no faithful mapping. | Executable as a **bounded named-suite subset**, fail closed on OpenSSL grammar syntax (the same "bounded subset or reject" precedent as `map`/`proxy_redirect`). Per-vhost cipher policy shares one rustls `ServerConfig` per listener, so conflicting declarations must fail closed. |
| `*_ssl_crl` | `WebPkiServerVerifier::with_crls` and the client verifier `with_crls` exist for upstream and downstream verification. | Implementable (load CRL PEM/DER from auth/proxy TLS config, rebuild verifier). |
| `*_ssl_password_file` | rustls-pemfile does not decrypt; a small dependency (e.g. `pkcs8`/`pkcs5` PEM decryption) can. | Implementable with a **new dependency** named in the wave plan, or close as documented difference. |
| `*_ssl_session_reuse` | rustls client resumption (`ClientConfig.resumption`, session cache) exists. | Implementable (per-upstream TLS config). |
| `*_ssl_certificate_cache` | nginx caches certificate files; the runtime reads at config compile/rotation. | No client-visible difference -> **runtime-owned**. |
| `uwsgi_ssl_protocols` | TLS 1.2/1.3 window mapping (the runtime's established pattern for `ssl_protocols`). | Implementable. |
| `*_ssl_conf_command` | OpenSSL `Options`/`NoAntiReplay`/`DHParameters` commands have no rustls equivalent and no portable meaning. | **Not implementable**; close as fail-closed with a diagnostic that names the rustls limitation. |

## 3. Out-Of-Profile Feasibility (64 Rows)

### 3.1 Mail Proxy - 51 Rows (class L; needs a profile amendment)

**What exists.** The protocol substrate ([sdkwork-webserver-mail-protocol](../../crates/sdkwork-webserver-mail-protocol), 45
tests) covering POP3/IMAP/SMTP state machines, SASL PLAIN/LOGIN, STARTTLS boundaries,
dot-stuffing, XCLIENT/PROXY v1, frontend transition table, backend relay helpers. It is
transport-free: no socket, no listener, no auth backend.

**What full wiring needs.**
1. **Model + materializer**: a `[mail]`/`mail {}` surface (or nginx-conf `mail {}`
   materialization) for `mail`, `server`, `listen`, `protocol`, `server_name`, `timeout`,
   `max_errors`, `resolver*`, STARTTLS/TLS per listener, `smtp_auth`/`imap_auth`/
   `pop3_auth`, capabilities/greeting, proxy timeout/buffer, `xclient`/`proxy_protocol`,
   `proxy_smtp_auth`, `proxy_pass_error_message`.
2. **Listener plane**: a mail listener type alongside the HTTP/stream listeners (gateway
   `data_plane/`), accepting, detecting protocol (`protocol smtp|imap|pop3`), STARTTLS
   upgrade with the existing rustls materializer, and per-connection timeouts/limits.
3. **Auth backends**: the `auth_http` protocol — an HTTP POST to the operator's
   auth server with nginx's mail auth fields (`Auth-User`, `Auth-Pass`, `Auth-Protocol`,
   `Auth-Login-Attempt`, `Client-IP`, `Client-Host`, `Auth-SMTP-Login` / XCLIENT data,
   etc.) and the nginx response grammar (`Auth-Status: OK|AUTHENTICATED|...`,
   `Auth-Server`, `Auth-Port`, `Auth-User`, `Auth-Pass`, `Auth-Error-Code`, ...).
   reqwest is already a workspace dependency; the protocol surface must be bounded and
   timeout-gated. Also `smtp_auth`/`imap_auth`/`pop3_auth` for backend SASL.
4. **Backend leg**: TLS to the backend (`proxy_ssl`-style trust/verification), XCLIENT/
   PROXY v1 header injection, `proxy_protocol`, `proxy_timeout`, dot-stuffing already
   handled by the substrate.
5. **Security review**: mail proxying exposes an auth oracle and SMTP relay paths; it
   needs a dedicated threat model (open-relay prevention, auth throttling, STARTTLS
   downgrade policy) and its own differential harness (Python stdlib `smtplib`/
   `imaplib`/`poplib` clients work offline; the reference nginx 1.29.6 oracle build has
   `--with-mail --with-mail_ssl_module`, so a real differential is possible).
6. **Requirement change**: PRD §4 excludes the mail proxy; the profile must be amended
   (`http-core-v1` -> a profile that includes mail, or a separate profile) before the
   rows can move from `excluded` to `executable`.

**Verdict.** Implementable; L (listener + auth_http + TLS + security review + evidence),
not XL, because the protocol substrate is done. **Dependency: profile amendment first.**

### 3.2 GeoIP - 8 Rows (class M; needs a profile amendment + new dependency)

**What it needs.** `maxminddb` (new dependency; not in `Cargo.lock` — the report names
it explicitly because a new database reader is a supply-chain decision), a database
lifecycle owner (path config, file existence/permissions, memory-mapped load, reload/
rotation, bounded lookup cost), variables (`$geoip_country_code`, `_code3`, `_name`;
`$geoip_city_*`, `$geoip_org`, `$geoip_latitude/longitude`, plus the stream variants),
and the `geoip_proxy`/`geoip_proxy_recursive` trust rules (reuse
`TrustedProxyConfig`/`real_ip` machinery). Database licensing/update cadence (MaxMind
GeoIP2/GeoLite2 EULA; test fixtures need a synthetic `.mmdb` or a licensed file) is a
product/legal dependency, not just code. http (5) + stream (3) share the lookup engine.
**Effort: M** + profile amendment + data-license decision.

### 3.3 Perl - 4 Rows (not implementable)

`perl`, `perl_modules`, `perl_require`, `perl_set` require an **embedded Perl
interpreter** executing arbitrary Perl inside the edge process. That means linking
libperl (C FFI, `unsafe`), per-request interpreter state, and a security posture
(arbitrary code execution, module loading from disk) that directly contradicts the
bounded, memory-safe, standalone-edge model (`RUST_CODE_SPEC` unsafe policy, PRD §4).
**Not implementable in this product; keep excluded.**

### 3.4 Lua / njs / Third-Party Dynamic Modules (1 excluded + PRD §4)

- `load_module` (1 row): dynamically loaded third-party nginx modules are compiled
  against nginx's internal C ABI; the Rust data plane has no such ABI. `dlopen` of nginx
  modules would not run (they expect nginx request structs/hooks) and would be an
  unbounded, memory-unsafe extension surface. The catalog already notes the reference
  OSS build is static. **Not implementable.**
- Lua (OpenResty) / njs: embedded script interpreters with nginx-specific APIs and
  event-loop integration (OpenResty cosockets/timers). Equivalents would be a new
  scripting subsystem with arbitrary-code security review; PRD §4 excludes
  "OpenResty/Lua, Perl, njs, and embedded executable configuration". **Not implementable
  in this product.**
- nginx Plus proprietary: `sticky` (1 fail-closed row) and Plus-only surfaces (keyval,
  commercial active-health details, Plus API) have no OSS reference implementation to
  differential against and a licence boundary. **Not implementable.**

### 3.5 HTTP/3 + QUIC - 9 Fail-Closed Rows (class XL; profile question first)

Rows: `http3`, `http3_hq`, `http3_max_concurrent_streams`, `http3_stream_buffer_size`,
`quic_active_connection_id_limit`, `quic_gso`, `quic_host_key`, `quic_retry`,
`events:quic_bpf`.

**Feasibility.**
- The QUIC transport substrate **exists**: `quinn` 0.11.12 is a workspace dependency and
  [quic.rs](../../crates/sdkwork-webserver-tunnel-transport/src/quic.rs) already builds a QUIC
  **server** endpoint (`quinn::Endpoint::server`, rustls TLS 1.3, transport tuning) for
  the tunnel, so QUIC server plumbing and rustls interop are proven in-tree. What is
  missing is HTTP/3: no `h3` crate is in the workspace or `Cargo.lock`.
- A full HTTP/3 wave needs: `h3` + `h3-quinn` dependencies (new supply-chain decision);
  a UDP listener plane (bind/firewall/Docker UDP port mapping, separate from TCP
  listeners — note `bin/docker-*`/`etc/` deployment surfaces would need UDP exposure);
  an h3 request/response adapter that reuses the compiled app/route selection and the
  existing handler pipeline (h3 bodies differ from hyper bodies, so a thin adapter to
  `axum::body::Body` is required); `Alt-Svc` advertisement on HTTP/1.1+HTTP/2 responses;
  QUIC/TLS material (TLS 1.3 only, same rustls certificates, ticket keys/retry tokens);
  0-RTT (rustls `max_early_data_size` exists; needs replay policy and `425 Too Early`/
  `Early-Data` semantics before any mutating request); connection migration/observability;
  and the nine directive mappings (`http3_max_concurrent_streams` -> quinn/h3
  max concurrent bidi streams, `http3_stream_buffer_size`, `quic_*` transport knobs,
  `http3_hq` HTTP/0.9-over-QUIC gating).
- The oracle side is possible: the pinned nginx 1.29.6 package is built
  `--with-http_v3_module` (verified with `nginx -V`), so a real differential is
  attainable. **But the current environment has no HTTP/3 client**: `curl` 8.18.0 here
  has HTTP2 but not HTTP3, and `aioquic` is not installed. The wave must first add an h3
  client to the differential harness (curl-ngtcp2 build or `aioquic` via pip, which is
  an offline-capability/CI dependency decision).
- **Profile boundary:** PRD §4 excludes HTTP/3, while the catalog classifies these nine
  rows `unsupported-fail-closed`, not `excluded` (finding F3). Decide the profile first.

**Verdict.** XL (new protocol family: deps + UDP listener plane + h3 adapter + 0-RTT
policy + deployment UDP + new h3 test tooling). Residual risks: UDP ingress/PMTU,
amplification/retry-token hardening, 0-RTT replay, dual-stack UDP deployment, and a
differential harness that is materially new. Recommended only after the in-profile
sequence; treat as a separate product decision, not an "RUST gap to close".

## 4. Recommended Closing Sequence

Dependency-ordered, in-profile first. Each step is one reviewed change set that includes
`cargo` tests, its differential slice, the catalog/matrix/audit regeneration
(`node tools/check-nginx-directive-matrix.mjs --write` then
`node tools/gen-nginx-directive-audit.mjs`), and a raised manifest case floor.

| Step | Wave | Rows moved | Counter delta (enforced / gap / runtime-owned) | Depends on | Evidence to add |
| ---: | --- | ---: | --- | --- | --- |
| 1 | 7A FastCGI residual | 32 | +27 / -27 / +5 | toolkit decision (F2), F1 field wiring | differential-protocols cases + cache/store unit tests |
| 2 | 7B uWSGI residual | 44 | +37 / -38 / +1 (+1 blocked) | 7A toolkit + upstream TLS builder | differential-protocols TLS/cache cases |
| 3 | 7C SCGI residual | 30 | +25 / -25 / +5 | 7A toolkit | differential-protocols cases |
| 4 | 7D memcached + gRPC residual | 13 | +11 / -12 / +1 (+1 blocked) | 7A socket helper + gRPC adapter | differential-protocols cases |
| 5 | Stream advanced | 18 | +18 / -18 | upstream TLS builder; stream `server_name`/`pass` design | differential-stream extended + stream integration tests |
| 6 | Subrequest engine | 7 | +7 / -7 | handler dispatch refactor + phase-semantics spec | new differential-subrequest slice + depth/cycle unit tests |
| 7 | Module-sized modules | 17 | +17 / -17 | none (parallelizable after 7A) | per-module differential cases (userid/browser/random_index/degradation/slice) |
| 8 | HTTP leftovers | 6 | +3 / -3 / +3 (http2_chunk_size + ssl_trusted_certificate http+stream executable; open_file_cache* -> runtime-owned) | none | h2 DATA-frame wire test; client-auth trust test; matrix note |
| 9 | Record non-implementable tail | 7 | gap -> 0 by decision | Lead catalog/audit change | blocker diagnostics + matrix rows |
| **in-profile end** | | **172** | **enforced 348 / runtime-owned 163 / behavior-gap 7 (all dependency-blocked) / out-of-profile 64 / fail-closed 145 / security-difference 17** | | |
| 10 | Mail wiring (optional profile expansion) | 51 | excluded -51, enforced +51 (bounded subset) | PRD §4 amendment, listener plane | mail differential (python stdlib clients vs nginx oracle) |
| 11 | GeoIP (optional) | 8 | excluded -8, enforced +8 | PRD amendment, `maxminddb` dependency, data licence | geo lookup differential/unit tests |
| 12 | HTTP/3 (optional) | 9 | fail-closed -9, enforced +9 | PRD/catalog boundary decision (F3), `h3` deps, UDP plane, h3 client tooling | h3 differential vs nginx 1.29.6 + UDP deployment verification |

Counter arithmetic. The current distribution is 203 enforced / 172 behavior-gap /
143 runtime-owned / 64 out-of-profile / 145 fail-closed / 17 security-difference = 744.
Steps 1-8 move **145 rows** into `enforced` (100 protocol residual + 18 stream + 7
subrequest + 17 module-sized + 3 HTTP leftovers), **20 rows** into `runtime-owned` by
approved documented-difference decision (15 no-spool rows + 3 `open_file_cache` + 2
`*_ssl_certificate_cache`), and leave **7 rows** in `behavior-gap` solely because they are
dependency-blocked (`chunked_transfer_encoding` 1, OCSP dynamic 4, `*_ssl_conf_command` 2).
That is the honest "code complete" state:

| Bucket | Current | After steps 1-8 (code complete) | Step 9 (Lead records the 7 decisions) |
| --- | ---: | ---: | ---: |
| `enforced` | 203 | **348** | 348 |
| `runtime-owned` | 143 | **163** | 163 |
| `behavior-gap` | 172 | **7** | **5** (dependency-blocked, explicit rationale) |
| `out-of-profile` | 64 | 64 | 64 |
| `fail-closed` | 145 | 145 | **147** (2 `*_ssl_conf_command`) |
| `security-difference` | 17 | 17 | 17 |
| **total** | **744** | **744** | **744** |

Step 9 does not add code; it records that the last 7 rows are decisions, not work.
The two `*_ssl_conf_command` rows move to `fail-closed` (no rustls equivalent; honoring
OpenSSL command strings would silently change TLS posture). The other five
(`chunked_transfer_encoding` 1 + dynamic OCSP 4) stay `behavior-gap` with an explicit
dependency-blocked rationale and the precise blocker diagnostic in the operator matrix —
unless the Lead prefers to move them to `fail-closed` too, which changes operator impact
(working nginx configs would fail to load) and is therefore a product decision. If the
Lead instead keeps the audit's six-decision taxonomy untouched, the seven rows remain
`behavior-gap` with the blocked rationale; either way no *implementation* backlog remains
after step 8.

Optional profile expansion (steps 10-12) would move 51 + 8 + 9 = 68 rows from
`excluded`/`fail-closed` into `enforced` (bounded subsets) after the requirement changes.
Measured from the step-9 state: enforced 348 -> 416, out-of-profile 64 -> 5
(Perl 4 + `load_module` 1), fail-closed 147 -> 138, behavior-gap 5, runtime-owned 163,
security-difference 17 - total 744.

### 4.1 What Each Step Must Not Do

- No gate lowering: differential case floors only rise; intentional differences are
  listed in the comparator allowlist with a spec justification, never by editing an
  expectation.
- No silent acceptance: every newly executable directive must either execute its bounded
  form or fail closed with a located diagnostic on the forms it does not execute (the
  Wave 1/2/2b precedent).
- No catalog claim before evidence: the catalog is regenerated from the implementation
  (`check-nginx-directive-matrix.mjs --write`), then the audit; a row moves only when
  its behavioral evidence exists (F6).
- No pile-up of decision debt: every step that lands a bounded difference updates the
  operator matrix row and the catalog `knownDifferences` so F2/F4 cannot recur.

## 5. Not Implementable In This Product

These rows are not "missing work"; they are outside what this product can safely be.
They should stay excluded/fail-closed with a diagnostic that says so.

| Item | Rows | Why not implementable |
| --- | ---: | --- |
| Perl (`perl`, `perl_modules`, `perl_require`, `perl_set`) | 4 | Requires an embedded Perl interpreter executing arbitrary Perl in the edge (C FFI/unsafe, per-request interpreter state, module loading); contradicts the memory-safe bounded-runtime model and PRD §4 "embedded executable configuration". |
| Third-party dynamic modules (`load_module`) | 1 | nginx modules are C ABI plugins against nginx's internal request/hook structs; the Rust data plane has no such ABI. Loading them cannot work and would be an unbounded memory-unsafe extension surface. |
| Lua (OpenResty) / njs | 0 rows in the inventory's OSS modules, but PRD §4 names them | Embedded script runtimes with nginx-specific event-loop/cosocket APIs; would be a new scripting subsystem plus arbitrary-code security review. |
| nginx Plus proprietary features (`sticky` and Plus-only surfaces) | 1 (`sticky`) + Plus surfaces not in the OSS inventory | No open reference implementation/oracle; licence boundary; commercial-only semantics. |
| `ssl_conf_command` (http/stream/upstream/mail variants) | 2 backlog + several fail-closed | OpenSSL `Options`/`NoAntiReplay`/`DHParameters` command strings have no rustls equivalent and no portable meaning; honoring them would silently change security posture. Close as fail-closed with a rustls-limitation diagnostic. |
| nginx disk temp-spool buffering (`*_temp_path`, `*_temp_file_write_size`, `*_max_temp_file_size`, `*_buffers`, `*_busy_buffers_size`) | 15 backlog (+ HTTP fail-closed variants) | The runtime streams bounded responses and intentionally owns no disk spool; a spool subsystem would own disk-exhaustion/cleanup semantics for a knob with no byte-level client-visible difference. Documented difference. |
| `chunked_transfer_encoding` (faithful parity) | 1 | Blocked by the pinned hyper encoder (see §2.1); only L workarounds exist. Recorded under §6 as dependency-blocked. |

## 6. Implementable But Blocked By The Pinned Dependency

| Item | Rows | Pinned blocker | Upstream fix path | Local fallback (fidelity cost) |
| --- | ---: | --- | --- | --- |
| `chunked_transfer_encoding` | 1 | `hyper` 1.12.0 h1 encoder has no close-delimited HTTP/1.1 response mode (`proto/h1/role.rs` chooses `chunked` for unknown-length 1.1 responses). | Request/implement an opt-in "close-delimited" response framing in hyper (or a response extension that selects it); then map the directive in `data_plane/` + model. | (a) bounded buffering to synthesize Content-Length (not streaming); (b) HTTP/1.0 status line (wire-visible difference, do not ship); (c) vendored/patch hyper (supply-chain/maintenance cost). |
| `ssl_stapling` / `ssl_stapling_verify` (http + stream) | 4 | `rustls` 0.23.46 exposes `CertifiedKey.ocsp` as a static byte field only — no fetch, refresh, responder selection or OCSP-response verification API (CRLs are supported; OCSP is not). | An OCSP owner outside rustls: fetch (reqwest, already a dependency), parse/verify (new crate), refresh scheduler, atomic key swap (the layered resolver supports it). Upstream rustls would need an OCSP verification hook to remove the extra crate. | Static `ssl_stapling_file` already executes (wire-proven); dynamic fetch is a documented difference, recommended given the industry OCSP sunset. |
| `ssl_ech_file` (http + stream, currently fail-closed) | 2 | `rustls` 0.23.46 has **client-side** ECH only (`client::EchConfig`, `with_ech`); no server-side ECH config API. | Upstream rustls server ECH support. | None; keep fail-closed. |
| `ssl_ciphers` full OpenSSL grammar | 2 backlog + fail-closed variants | rustls can select suites via `CryptoProvider.cipher_suites`, but OpenSSL cipher strings/`@SECLEVEL` have no faithful mapping; per-vhost cipher policy is one rustls `ServerConfig` per listener. | None needed for a bounded subset; full grammar is not a coherent target. | Execute a bounded named-suite subset, fail closed on OpenSSL grammar; document the difference. |
| `http2_chunk_size` (catalog says "blocked") | 1 | No direct hyper/h2 DATA-frame-size knob. | Optional upstream knob; not required for the observable contract. | **Implementable now** via a body adapter yielding <=N-byte frames (see §2.4); prove with a DATA-frame wire test. |

Environment caveats for the dependency-blocked decisions:

- The report inspected the **pinned** sources: `hyper` 1.12.0, `h2` 0.4.20, `rustls`
  0.23.46, `axum-server` 0.8.0, `quinn` 0.11.12, `socket2` 0.6.5 (from `Cargo.lock`). A
  dependency bump invalidates the "blocked" claims and they must be re-verified against
  the new source before the next catalog regeneration.
- The nginx reference build is 1.29.6 with `--with-http_v2_module
  --with-http_v3_module --with-mail --with-mail_ssl_module` (verified with `nginx -V`),
  so mail and HTTP/3 differentials are possible once client tooling exists; the current
  environment has no HTTP/3 client (`curl` lacks HTTP3, no `aioquic`).

## 7. Reproduction And Change Control

All commands below are read-only and were exercised for this report at the snapshot
above unless noted.

```sh
# 1. Directive inventory (744 rows, nginx 1.29.6).
node tools/gen-nginx-directive-inventory.mjs --check

# 2. Directive matrix gate: catalog <-> implementation reconciliation, typed-TOML
#    enforcement, fixture coverage, generated operator matrix (12/12).
node tools/check-nginx-directive-matrix.mjs

# 3. Generated 744-row audit ledger: freshness + evidence verification.
node tools/gen-nginx-directive-audit.mjs --check --verify-evidence
#    Regenerate after a catalog change:
#    node tools/check-nginx-directive-matrix.mjs --write
#    node tools/gen-nginx-directive-audit.mjs

# 4. Aggregate release gate (static gates + behavioural corpus + every manifest-discovered
#    differential slice, with SDKWORK_REQUIRE_NGINX_EVIDENCE=1).
node tools/run-nginx-compat-gate.mjs

# 5. Backlog counts used by this report (catalog classifications).
python3 - <<'PY'
import json, collections
c = json.load(open('specs/nginx-directive-support.catalog.json'))
print(collections.Counter(x['classification'] for x in c['directives']))
print('rows:', len(c['directives']))
PY
```

Outputs at this snapshot:

| Check | Output |
| --- | --- |
| inventory | `nginx directive inventory is canonical and valid (744 directives, nginx 1.29.6)` |
| matrix gate | `tests 12 / pass 12 / fail 0` |
| audit | `audit evidence verification passed: 744 rows, 203 executable evidence references, 124 completed-wave directives, 6 resolved evidence citations, 11 resolved migration over-claims` |
| catalog classifications | `executable 203, accepted-unenforced 170, unsupported-fail-closed 307, excluded 64` |

Manifest slice floors that the closing waves must extend (not lower):
[specs/nginx-behavioral-corpus.manifest.json](../../specs/nginx-behavioral-corpus.manifest.json) -> full-surface 76, conditional-cache 50,
differential-http 259, host-selection 39, differential-protocols 39, differential-stream 9.

**Snapshot change control.** This report is a snapshot of the catalog at sha256
`3669202b...` and HEAD `e43fa1c9...`. The worktree was concurrently receiving an
uncommitted code wave while this report was written (implementation files only; no
catalog, matrix or audit change). Those edits cannot change any count in this report
until the Lead re-syncs the catalog; the wave may also implement individual rows
discussed here (for example the 7A `fastcgi_bind`/`fastcgi_socket_keepalive` wiring), so
re-verify the specific findings against the landing commit. After the next catalog sync,
re-run the four commands above; if any count moved, regenerate this report rather than
editing it by hand.

## 8. Open Decisions For The Lead

1. **F1 fix:** wire `bindAddress`/`socketKeepalive` for the CGI resources in 7A/7D (S), or
   remove them from schema/model and fail closed. Do not leave a declared-and-dropped key.
2. **F2 alignment:** choose one decision per semantic class across protocol families
   (prefer `proxy_*` precedent) and re-sync the catalog once, instead of per-family
   decisions.
3. **F3 profile/catalog:** align HTTP/3 (`excluded` vs `unsupported-fail-closed`) and
   decide whether a `mail`-including profile is in scope; both are requirement changes.
4. **Documented-difference set:** approve moving the 15 spool rows + 2 certificate-cache
   rows + 3 `open_file_cache` rows to `runtime-owned` (no implementation), so the backlog
   counter reflects "nothing implementable".
5. **OCSP:** approve `ssl_trusted_certificate` (M) and defer dynamic
   stapling/refresh/verify as a documented difference (L, new verifier dependency,
   industry sunset).
6. **Dependency-blocked bucket:** approve recording the 7 remaining rows (chunked 1,
   OCSP dynamic 4, `ssl_conf_command` 2) as "blocked by pinned dependency/OpenSSL
   semantics" in the audit taxonomy; then the audit's implementable backlog is zero.
7. **New dependencies:** approve, if/when the optional waves start: `maxminddb` (GeoIP),
   `h3`/`h3-quinn` (HTTP/3), an OCSP verification crate (dynamic stapling), a PEM
   decryption crate (`ssl_password_file`), and the h3 differential client tooling.
8. **Evidence discipline:** every landing change regenerates catalog + matrix + audit and
   raises a manifest case floor; no behavioral gate is lowered to make a wave land.
