# Nginx Final-Gap Feasibility And Closing Sequence (`http-core-v1`) - 2026-10-30 (v2)

| Field | Value |
| --- | --- |
| Report date | 2026-10-30 (v2; supersedes the v1 snapshot at HEAD `e43fa1c9`) |
| Repository | `sdkwork-webserver`, branch `main`, HEAD `3f169e8b76a748f92775bb26be547d0ffb4fe3a8` |
| Scope | final-gap feasibility for the **95 behavior-gap rows** and the **13 out-of-profile directives** at this HEAD, plus the HTTP/3 profile question (9 fail-closed rows) |
| Compatibility profile | `http-core-v1` ([PRD-nginx-compatibility.md](../product/prd/PRD-nginx-compatibility.md) §2-§4) |
| Reference implementation | nginx 1.29.6, nginx.org official Linux package (the differential oracle build; `nginx -V` includes `--with-mail --with-mail_ssl_module --with-http_v3_module`) |
| Companion reports | [2026-10-30-nginx-directive-by-directive-audit.md](2026-10-30-nginx-directive-by-directive-audit.md), [NGINX-COMPATIBILITY-MATRIX.md](../guides/operator/NGINX-COMPATIBILITY-MATRIX.md), [nginx-gap.catalog.json](../../specs/nginx-gap.catalog.json), [2026-10-30-nginx-commercial-compatibility-assessment.md](2026-10-30-nginx-commercial-compatibility-assessment.md) |
| Directive inventory | [nginx-directive-inventory.json](../../specs/nginx-directive-inventory.json) - 744 directives, nginx 1.29.6; catalog `inventory.digest` `28c70ea443c01288e876d912ab7bdfa8933a8a8ffa3bbc7e5c74de6584f29d15` |
| Support catalog | [nginx-directive-support.catalog.json](../../specs/nginx-directive-support.catalog.json) - 744 rows, raw sha256 `37dc0ad686e08bca618bd0291d6453e210b5653a15ba452bf20ce83e7da010bd` |
| Audit ledger | [2026-10-30-nginx-directive-by-directive-audit.md](2026-10-30-nginx-directive-by-directive-audit.md) - 744 rows, raw sha256 `549e06450034c454cdaf60e2c763b0bdeb623896f15abb06a6f7b3310931929f`; `--check --verify-evidence` passes at this HEAD |
| Differential manifest | [nginx-behavioral-corpus.manifest.json](../../specs/nginx-behavioral-corpus.manifest.json) - 6 differential slices, raw sha256 `54514f311614fb184ef2e1c3a88dc9bbb4952c6afdefabf7487f7800c565f50c` |
| Production-config gate | [check-nginx-real-world.mjs](../../tools/check-nginx-real-world.mjs) - committed api.birdcoder.com snapshot (nginx 1.28.0, 33 files): **41 vhosts / 4 listeners / 150 resources / 88 upstreams / 0 skipped**, verified at this snapshot |
| Method | read-only: catalog/audit/manifest recomputation, source inspection, pinned-crate source inspection (`Cargo.lock`), static gates re-run at this snapshot |
| Not evidence | the binary-dependent aggregate/differential gates were **not** re-run live while this report was written: a concurrent code wave had the worktree dirty (`crates/sdkwork-webserver-core/src/config/model.rs`) and the gateway binary older than that source. Every count below is recomputed from the committed catalog/audit/manifest at HEAD `3f169e8b`; no implementation is claimed that the catalog does not claim |

## Status And Authority

- The source of truth is the committed **catalog** (classification per directive) plus the
  generated **audit ledger** (operator-facing decision per directive). This report edits
  neither artifact and reclassifies no row; where the honest close is "do not implement"
  it names the owner action (catalog + audit + matrix resync, Lead-owned).
- "Behavior gap" is the audit decision `behavior-gap` (**95 rows**), which spans two
  catalog classifications: 7 `accepted-unenforced` rows (the dependency-blocked tail) and
  88 `unsupported-fail-closed` rows. "Out of profile" is the audit decision
  `out-of-profile` (**13 rows**, all catalog `excluded`).
- Effort classes are planning estimates, not commitments: **S** = one directive or a small
  mechanical mapping over an existing model field; **M** = one coherent wave slice
  (model + materializer + data-plane path + differential evidence, several files);
  **L** = a new engine or architecture change (temp-spool subsystem, slice cache
  indexing, mail client-cert flows); **XL** = a new transport/protocol subsystem
  (HTTP/3 listener plane).
- Every planned wave keeps the release gates unchanged. No gate is lowered or skipped;
  each wave adds evidence and raises a manifest case floor.
- The mail proxy is **in profile** as of the Wave 9A/9B work (PRD §4 amended): 27 mail
  directives are executable and the mail differential slice exists. The v1 report's
  64-row out-of-profile set is therefore historical; the current set is 13.

## 0. Executive Summary

At HEAD `3f169e8b` the declared `http-core-v1` surface is:

| Bucket | Rows | Meaning |
| --- | ---: | --- |
| Executable, evidence-backed | **328** | the Rust data plane executes the bounded form; committed evidence reaches it |
| Deliberately runtime-owned | **147** | no client-visible effect; the runtime owns the subsystem by design |
| Intentional security difference | **17** | stricter/safer than nginx, spec-justified |
| Behavior gap (remaining backlog) | **95** | client-visible nginx behavior not produced yet |
| Out of profile | **13** | excluded by PRD §4 and rejected in active contexts |
| Fail-closed, not planned | **144** | rejected at materialization; outside the declared required surface |
| **total** | **744** | every inventory row appears exactly once |

The 95-row backlog decomposes into ten categories and closes as follows:

| Disposition | Rows | Composition |
| --- | ---: | --- |
| Code-implementable bounded forms | **58** | stream advanced 18, module-sized 15 (`userid` 9 + `browser` 4 + `random_index` + `slice`), HTTP/2 DATA 1, protocol residuals 14 (3 `ignore_client_abort` + 2 gRPC bind/keepalive + 9 temp-spool), mail residuals 10 |
| Runtime-owned, close by decision (no code) | **5** | protocol 4 (`fastcgi_catch_stderr`, `fastcgi_send_lowat`, 2 `*_ssl_certificate_cache`), mail 1 (`ssl_verify_depth`) |
| Blocked by the pinned rustls 0.23.46 surface | **23** | gRPC 5, uWSGI 6, mail 12 (cipher/CRL/session/password/command/depth knobs) |
| Blocked by the pinned hyper 1.12.0 encoder | **1** | `chunked_transfer_encoding` |
| Blocked by rustls' static-only OCSP field | **6** | `ssl_stapling`/`ssl_stapling_verify`/`ssl_trusted_certificate` (http + stream) |
| nginx Plus, no OSS oracle | **2** | `degradation`, `degrade` |
| **total** | **95** | **sum proof: 58 + 5 + 23 + 1 + 6 + 2 = 95** |

The 7 dependency-blocked rows (`chunked_transfer_encoding` + the six dynamic-OCSP rows)
are the only accepted-unenforced gap rows; the exact per-row allocation is in §4 and the
counter arithmetic in §6.

The largest single implementable item is the **bounded temp-spool subsystem** (9 rows,
L). The largest surface is **stream advanced gaps** (18 rows), then **mail residuals**
(23 rows, 10 implementable) and **protocol residuals** (29 rows, 14 implementable). The
only wire-visible *pinned-blocked* row remains `chunked_transfer_encoding`; the only
consumer-blocking dependency tail is dynamic OCSP stapling.

After the four in-profile waves in §6, the projection is **executable 386 / runtime-owned
152 / security-difference 17 / behavior-gap 7 (all dependency-blocked) / out-of-profile 13
/ fail-closed 169 = 744**; after the Lead records the last 7 rows as explicit
dependency-blocked decisions, **no implementable backlog remains**.

## 1. Ground Truth (Recomputed, Read-Only)

### 1.1 Inventory And Catalog Counts

`node tools/gen-nginx-directive-inventory.mjs --check` at this snapshot:
`nginx directive inventory is canonical and valid (744 directives, nginx 1.29.6)`.

Catalog classification (`specs/nginx-directive-support.catalog.json`, 744 rows):

| Classification | Rows | Share | Meaning |
| --- | ---: | ---: | --- |
| `executable` | **328** | 44.1% | executed by the Rust data plane, with a committed evidence reference |
| `accepted-unenforced` | **171** | 23.0% | parsed and accepted; the value has no (or only a documented) runtime effect |
| `unsupported-fail-closed` | **232** | 31.2% | rejected at materialization with a located diagnostic |
| `excluded` | **13** | 1.7% | deliberately outside the declared product profile |
| **total** | **744** | 100% | **sum proof: 328 + 171 + 232 + 13 = 744** |

Per nginx group:

| Group | Directives | executable | accepted-unenforced | unsupported-fail-closed | excluded |
| --- | ---: | ---: | ---: | ---: | ---: |
| `core` | 21 | 1 | 16 | 3 | 1 |
| `events` | 20 | 0 | 18 | 2 | 0 |
| `http` | 547 | 275 | 102 | 161 | 9 |
| `mail` | 51 | 27 | 1 | 23 | 0 |
| `stream` | 105 | 25 | 34 | 43 | 3 |
| **total** | **744** | **328** | **171** | **232** | **13** |

The mail group is fully in profile (27 executable = the Wave 9A/9B set); the remaining
out-of-profile rows are HTTP/stream GeoIP (8), HTTP Perl (4) and `load_module` (1).

### 1.2 Audit Decision Counts (744 Rows)

`node tools/gen-nginx-directive-audit.mjs --check --verify-evidence` at this snapshot:
`audit evidence verification passed: 744 rows, 328 executable evidence references, 249 completed-wave directives, 6 resolved evidence citations, 11 resolved migration over-claims`.

| Decision | Count | Share | Meaning |
| --- | ---: | ---: | --- |
| `enforced` | **328** | 44.1% | executed with committed evidence |
| `runtime-owned` | **147** | 19.8% | no client-visible effect; runtime owns the subsystem |
| `security-difference` | **17** | 2.3% | intentional stricter/safer behavior, spec-justified |
| `behavior-gap` | **95** | 12.8% | client-visible behavior not produced yet (planned backlog) |
| `out-of-profile` | **13** | 1.7% | PRD §4 exclusion, rejected in active contexts |
| `fail-closed` | **144** | 19.4% | rejected at materialization, not planned |
| **total** | **744** | 100% | **sum proof: 328 + 147 + 17 + 95 + 13 + 144 = 744** |

Catalog classification × audit decision:

| Catalog classification | Rows | `enforced` | `runtime-owned` | `security-difference` | `behavior-gap` | `out-of-profile` | `fail-closed` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `executable` | 328 | 328 | 0 | 0 | 0 | 0 | 0 |
| `accepted-unenforced` | 171 | 0 | 147 | 17 | 7 | 0 | 0 |
| `unsupported-fail-closed` | 232 | 0 | 0 | 0 | 88 | 0 | 144 |
| `excluded` | 13 | 0 | 0 | 0 | 0 | 13 | 0 |
| **total** | **744** | 328 | 147 | 17 | 95 | 13 | 144 |

The 7 `accepted-unenforced` behavior-gap rows are exactly the dependency-blocked tail:
`chunked_transfer_encoding`, `ssl_stapling`/`ssl_stapling_verify`/`ssl_trusted_certificate`
(http) and the three `stream:` equivalents. Every other gap row is still rejected at load.

### 1.3 The 95-Row Backlog, By Category (Exact Directives)

Categories are the audit generator's `REMAINING_GAP_CATEGORIES` set
([tools/gen-nginx-directive-audit.mjs](../../tools/gen-nginx-directive-audit.mjs)), emitted
in [audit ledger](2026-10-30-nginx-directive-by-directive-audit.md) §3.2 and cross-checked
against the committed catalog:

| # | Category | Rows | Exact directives |
| --- | --- | ---: | --- |
| 1 | HTTP/1 framing policy | 1 | `chunked_transfer_encoding` |
| 2 | module-sized functional modules | 17 | `userid`, `userid_domain`, `userid_expires`, `userid_flags`, `userid_mark`, `userid_name`, `userid_p3p`, `userid_path`, `userid_service`, `ancient_browser`, `ancient_browser_value`, `modern_browser`, `modern_browser_value`, `random_index`, `degradation`, `degrade`, `slice` |
| 3 | HTTP/2 DATA chunking | 1 | `http2_chunk_size` |
| 4 | OCSP dynamic fetch / refresh / verification | 6 | `ssl_stapling`, `ssl_stapling_verify`, `ssl_trusted_certificate`, `stream:ssl_stapling`, `stream:ssl_stapling_verify`, `stream:ssl_trusted_certificate` |
| 5 | stream advanced gaps | 18 | `proxy_protocol_timeout`, `pass`, `server_name`, `proxy_bind`, `proxy_half_close`, `proxy_next_upstream_timeout`, `proxy_next_upstream_tries`, `proxy_requests`, `proxy_responses`, `proxy_download_rate`, `proxy_upload_rate`, `proxy_ssl`, `proxy_ssl_certificate`, `proxy_ssl_certificate_key`, `proxy_ssl_name`, `proxy_ssl_server_name`, `proxy_ssl_verify`, `proxy_ssl_trusted_certificate` (all `stream:`) |
| 6 | gRPC residual forms | 8 | `grpc_bind`, `grpc_socket_keepalive`, `grpc_ssl_certificate_cache`, `grpc_ssl_ciphers`, `grpc_ssl_conf_command`, `grpc_ssl_crl`, `grpc_ssl_password_file`, `grpc_ssl_session_reuse` |
| 7 | FastCGI residual forms | 6 | `fastcgi_catch_stderr`, `fastcgi_ignore_client_abort`, `fastcgi_max_temp_file_size`, `fastcgi_send_lowat`, `fastcgi_temp_file_write_size`, `fastcgi_temp_path` |
| 8 | uWSGI residual forms | 11 | `uwsgi_ignore_client_abort`, `uwsgi_max_temp_file_size`, `uwsgi_ssl_certificate_cache`, `uwsgi_ssl_ciphers`, `uwsgi_ssl_conf_command`, `uwsgi_ssl_crl`, `uwsgi_ssl_password_file`, `uwsgi_ssl_session_reuse`, `uwsgi_ssl_verify_depth`, `uwsgi_temp_file_write_size`, `uwsgi_temp_path` |
| 9 | SCGI residual forms | 4 | `scgi_ignore_client_abort`, `scgi_max_temp_file_size`, `scgi_temp_file_write_size`, `scgi_temp_path` |
| 10 | mail module residuals | 23 | `auth_http_pass_client_cert`, `imap_capabilities`, `pop3_capabilities`, `smtp_capabilities`, `resolver`, `resolver_timeout`, `set_real_ip_from`, `ssl_certificate_compression`, `ssl_ciphers`, `ssl_client_certificate`, `ssl_conf_command`, `ssl_crl`, `ssl_dhparam`, `ssl_ecdh_curve`, `ssl_password_file`, `ssl_prefer_server_ciphers`, `ssl_session_cache`, `ssl_session_ticket_key`, `ssl_session_tickets`, `ssl_session_timeout`, `ssl_trusted_certificate`, `ssl_verify_client`, `ssl_verify_depth` (all `mail:`) |
| | **total** | **95** | **sum proof: 1 + 17 + 1 + 6 + 18 + 8 + 6 + 11 + 4 + 23 = 95** |

`open_file_cache`, `open_file_cache_min_uses` and `open_file_cache_valid` are **not** in
the 95: the audit records them as `runtime-owned` (three of the 147). They are a
performance cache with no client-visible response contract; the runtime owns static-file
open/caching policy. §4.6 explains why no code is planned.

### 1.4 The 13 Out-Of-Profile Directives (Exact)

All carry catalog classification `excluded` and PRD §4 as authority; the audit decision is
`out-of-profile` for each.

| Group | Module | Rows | Directives |
| --- | --- | ---: | --- |
| http | `ngx_http_geoip_module` | 5 | `geoip_city`, `geoip_country`, `geoip_org`, `geoip_proxy`, `geoip_proxy_recursive` |
| stream | `ngx_stream_geoip_module` | 3 | `geoip_city`, `geoip_country`, `geoip_org` |
| http | `ngx_http_perl_module` | 4 | `perl`, `perl_modules`, `perl_require`, `perl_set` |
| core | `ngx_core_module` | 1 | `load_module` |
| | | **13** | **sum proof: 8 GeoIP + 4 Perl + 1 load_module = 13** |

HTTP/3 and QUIC are a separate profile question: the `ngx_http_v3_module` rows
(`http3`, `http3_hq`, `http3_max_concurrent_streams`, `http3_stream_buffer_size`,
`quic_active_connection_id_limit`, `quic_gso`, `quic_host_key`, `quic_retry`) plus
`events:quic_bpf` are **9 `unsupported-fail-closed`** rows, not part of the 13 and not part
of the 95. See §5.3.

### 1.5 Wave Completion (249 Directives)

Seventeen waves landed 249 directives, all `executable` with verified evidence:

| Wave | Directives | State |
| --- | ---: | --- |
| wave 1 | 5 | done - map, MIME `types`/`default_type`, `gzip_static`, `gzip_vary` |
| wave 2 | 6 | done - redirect construction, limit status codes, `max_ranges`, chain files, typed-TOML maps |
| wave 2b | 9 | done - proxy response-header/cookie policy, `internal`, `merge_slashes`, header acceptance |
| wave 3a | 6 | done - charset family header behavior, gzip refinements |
| wave 3a-2 | 6 | done - autoindex family, `empty_gif`, `stub_status` |
| wave 3b | 5 | done - HTTP/2 stream ceiling, `proxy_method`, stream allow/deny/return |
| wave 4a | 3 | done - `geo`, `split_clients`, `valid_referers` |
| wave 5 (upstream protocol) | 56 | done - FastCGI/uWSGI/SCGI/memcached + bounded memcached `set` |
| wave 6 (gRPC) | 20 | done - h2 upstream client, headers/timeouts/buffer/retry/intercept, verified `grpc_ssl_*` |
| stream/OCSP wave | 8 | done - stream limit/zone/dry-run/map/set/preread, static `ssl_stapling_file` |
| wave 7A (CGI transport) | 8 | done - `*_bind`, `*_socket_keepalive` for all CGI/memcached transports |
| wave 10 (rewrite/variable) | 1 | done - server/location `if` conditions over the bounded variable engine |
| mail waves (9A/9B) | 27 | done - POP3/IMAP/SMTP proxy end-to-end (auth_http, login/relay, STARTTLS/implicit TLS) |
| wave 7B (uWSGI upstream TLS) | 7 | done - `uwsgi_ssl_*` identity/verification |
| wave 7C (CGI cache family) | 45 | done - FastCGI/uWSGI/SCGI `*_cache*` on the shared cache store |
| wave 7D (protocol response policy) | 30 | done - CGI buffer/store/limit_rate/force_ranges/ignore_headers/pass_request_body, memcached buffer/gzip-flag, gRPC ignore_headers |
| wave 8 (subrequest engine) | 7 | done - `addition`, `auth_request`/`auth_request_set`, `mirror` on a one-level bounded engine |
| **total** | **249** | **sum proof: 5+6+9+6+6+5+3+56+20+8+8+1+27+7+45+30+7 = 249** |

## 2. Implemented Surface And Evidence

### 2.1 Differential Slices (6, 458 Cases) And The Production Gate

Each slice renders one fixture twice (real nginx 1.29.6 oracle and the gateway) and compares
the observable contract; every slice owns its committed oracle, pins `nginx/1.29.6`, and
proves which implementation answered through an identity witness (`Server: nginx/1.29.6`
vs no banner on the gateway).

| Slice | Cases | Orchestrator / floor | What it proves |
| --- | ---: | --- | --- |
| conditional-cache | 50 | [run-nginx-differential.mjs](../../tools/run-nginx-differential.mjs) / 50 | conditional requests (`If-None-Match`/`If-Modified-Since`), byte ranges, `expires`/`etag`/cache policy field by field |
| differential-http | 274 | [run-nginx-differential-http.mjs](../../tools/run-nginx-differential-http.mjs) / 274 | broad HTTP surface: static/MIME/HEAD/ranges incl. multipart, routing/rewrite/`return`, response headers (`add_header` full semantics), `error_page`, gzip, allow/deny + `auth_basic`, `limit_req`/`limit_conn`, proxy URI splice/forwarded headers/hop-by-hop/upstream status, referer/geo/split_clients variables, autoindex, charset, addition/auth_request/mirror subrequests |
| host-selection | 39 | [run-nginx-differential-host.mjs](../../tools/run-nginx-differential-host.mjs) / 39 | virtual-host selection precedence (exact/leading+trailing wildcard/dot-prefix/regex authored order/default), bounded named locations for `try_files`/`error_page` |
| differential-protocols | 77 | [run-nginx-differential-protocols.mjs](../../tools/run-nginx-differential-protocols.mjs) / 77 | FastCGI/uWSGI/SCGI parameter+body+header+intercept+retry forms, memcached hit/miss, gRPC unary/headers/retry/TLS; cache family, response policy (store/rate/ranges), uWSGI upstream TLS (trusted/mTLS/untrusted) |
| differential-stream | 9 | [run-nginx-differential-stream.mjs](../../tools/run-nginx-differential-stream.mjs) / 9 | stream variable engine (`set`/`map`/`$ssl_preread_server_name`), ordered access phase, literal `return`, `limit_conn` both directions, proxied byte flow |
| differential-mail | 9 | [run-nginx-differential-mail.mjs](../../tools/run-nginx-differential-mail.mjs) / 9 | POP3/IMAP/SMTP proxy: client byte transcript, every backend line and every `auth_http` request header for login/relay, AUTH LOGIN dialogs, auth failures, SMTP `AUTH`+XCLIENT, STARTTLS/implicit TLS |
| **total** | **458** | **sum proof: 50 + 274 + 39 + 77 + 9 + 9 = 458** | |

Plus the in-process behavioural corpus (full-surface, 76 cases) and the production-config
gate: the committed api.birdcoder.com nginx snapshot (33 files, nginx 1.28.0) materializes
through `validate-nginx` with an operator document-root allowlist and reports
**`virtualHosts=41 listeners=4 resources=150 upstreams=88 skipped=0`** — i.e. the whole
real-world tree loads with no skipped site and no fail-closed marker.

### 2.2 What Landed After v1

- **Wave 8 subrequest engine** (7 rows): one-level bounded re-entrant dispatch now backs
  `add_before_body`/`add_after_body`/`addition_types`, `auth_request`/`auth_request_set`
  and `mirror`/`mirror_request_body`; differential-http grew 259 -> 274 cases.
- **Mail 9A/9B** (27 rows): config plane (`MailConfig`/`MailServerConfig`, schema,
  validation, typed TOML), nginx `mail {}` materialization, `FrontendSession` plaintext
  dialog listeners with timeout/`so_keepalive`/inbound PROXY v1, `auth_http` exchange,
  STARTTLS/implicit TLS, XCLIENT and backend relay; mail differential 9/9; mail is now in
  profile.
- **Protocol response policy (7D)** and **CGI cache family (7C)**: the shared cache store
  and response-policy seam now also serve the CGI family, which is the foundation the
  remaining temp-spool and `ignore_client_abort` rows build on.
- **uWSGI upstream TLS (7B)**: the upstream TLS policy builder is now proven on a second
  protocol family, which is directly reusable for the stream upstream-TLS rows in §4.5.

## 3. Feasibility Rules Used Below

- **Implementable** = a bounded form can execute and the unsupported forms fail closed with
  a located diagnostic; requires model + materializer + data-plane path + behavioral
  evidence (a differential case or a wire/integration test). "Materializes into a model
  field" alone is not evidence.
- **Runtime-owned** = the value has no client-visible contract the nginx documentation
  promises; the runtime owns the subsystem by design. Close by decision, not by code.
- **Blocked by the pinned dependency** = the contract is real but the pinned crate cannot
  express it; the row stays fail-closed/accepted-unenforced with the blocker diagnostic and
  a named upstream fix path.
- **Not implementable in this product** = the feature requires an embedded interpreter, a
  foreign C ABI, or a proprietary implementation with no OSS oracle, and would contradict
  the standalone memory-safe edge model or PRD §4.

## 4. The 95 Behavior-Gap Rows, Category By Category

### 4.1 HTTP/1 Framing Policy - `chunked_transfer_encoding` (1) - **blocked by pinned hyper**

**nginx contract (probe).** With `chunked_transfer_encoding on` (default) an unknown-length
HTTP/1.1 response is sent with `Transfer-Encoding: chunked`. With `off` the same response is
close-delimited: status line stays `HTTP/1.1`, no `Transfer-Encoding`, and the server adds
`Connection: close` (the connection cannot be reused).

**Pinned stack.** `hyper` **1.12.0** `proto/h1/role.rs` selects the server encoder:
an unknown-length response uses `Encoder::close_delimited()` only for HTTP/1.0 or for
methods/statuses that cannot be chunked (HEAD/1xx/204/304); otherwise it unconditionally
writes `transfer-encoding: chunked`. There is no public knob to request close-delimited
HTTP/1.1 framing. The three local workarounds all change the contract: (a) buffer to
synthesize `Content-Length` (breaks unbounded/streaming responses), (b) answer HTTP/1.0
(wire-visible version change — do not ship), (c) vendor/patch hyper (supply-chain +
maintenance cost for response framing/keep-alive ownership).

**Verdict: blocked by the pinned dependency.** Keep the truthful accepted-unenforced
diagnostic; record the upstream feature request (opt-in close-delimited HTTP/1.1 response)
and re-verify on any hyper bump. Effort: L only if a vendored encoder is chosen.

### 4.2 Module-Sized Functional Modules (17) - **15 implementable, 2 nginx Plus**

| Module | Rows | Design sketch | Effort | Verdict |
| --- | ---: | --- | --- | --- |
| `userid` | 9 | Process-identity cookie (`uid`) generation with nginx's wire format + `$uid_got`/`$uid_set` variables; `name`/`domain`/`path`/`expires`/`p3p`/`flags`/`mark`/`service` knobs consumed by the response filter; reuses the existing response-header/variable seam | M | implementable |
| `browser` | 4 | `$modern_browser`/`$ancient_browser` classification from authored `modern_browser`/`ancient_browser` version lists (family/version grammar) feeding the shared variable engine; bounded UA parser, no new crate required | M | implementable |
| `random_index` | 1 | Location handler: for a directory request with no index, pick a random regular file from a bounded directory read (sorted set + random index) | S | implementable |
| `slice` | 1 | Split a proxied response into cached range subrequests (`slice <size>`, `$slice_range` variable, cache key includes the range, range coalescing); builds on the landed shared cache store + range engine | M/L | implementable (cache-indexing work) |
| `degradation`/`degrade` | 2 | nginx Plus module (`ngx_http_degradation_module`, no `--with-http_degradation_module` in the OSS reference build) keyed on nginx's own pool-allocation threshold: no OSS oracle exists to differential against, and the runtime's `resource_pressure` controller is a different (documented) signal | S by size, 0 by fidelity | **not implementable — nginx Plus only; close fail-closed with a Plus/Plus-oracle diagnostic** |

`degradation`/`degrade` appear in the 744-row inventory because the inventory is derived
from nginx source tables, but the OSS reference binary does not build the module: there is
no behavior to reproduce and no oracle to compare. The catalog/audit should move these two
rows from `behavior-gap` to a fail-closed "not implementable (nginx Plus, no OSS oracle)"
decision — Lead catalog action.

### 4.3 HTTP/2 DATA Chunking - `http2_chunk_size` (1) - **implementable M**

nginx caps the body bytes sliced into one HTTP/2 DATA frame (default 8k). The pinned
`hyper` 1.12.0/h2 0.4.20 server path does not merge distinct `send_data` calls; it may only
split further for peer `SETTINGS_MAX_FRAME_SIZE`/flow control. A bounded response-body
wrapper that yields at most N bytes per frame therefore realizes the observable contract.
Needs a raw-h2 frame-capturing wire test (the repo already proves SETTINGS publication in
`tests/http2_tuning.rs`), a mapping knob, and a catalog wording fix from "blocked" to
"body-sizing adapter". **Verdict: implementable M.**

### 4.4 OCSP Dynamic Fetch / Refresh / Verification (6) - **blocked by pinned rustls**

Rows: `ssl_stapling`, `ssl_stapling_verify`, `ssl_trusted_certificate` (http) and the three
`stream:` equivalents.

**What already works.** Static stapling via `ssl_stapling_file` is executable: the DER
response is loaded into rustls `CertifiedKey.ocsp` and is wire-proven by
`tests/tls_ocsp_stapling.rs` (the static form is *not* a gap).

**Why dynamic is blocked.** rustls **0.23.46** exposes `CertifiedKey { cert, key, ocsp }`
as static bytes; there is no fetch, refresh, responder-selection or OCSP-response
verification API (CRLs are supported for verification, OCSP is not). A dynamic
implementation needs an OCSP owner outside rustls: HTTP fetch (`reqwest` 0.13.5 is already
a dependency), DER parse/verify (a new crate — none is in `Cargo.lock`), a refresh
scheduler, and an atomic key-swap path. `ssl_trusted_certificate` is the trust store those
verification paths would consume; wiring it alone is M but has no client-visible effect
until verification exists.

**Verdict: blocked by the pinned dependency M/L.** Keep the accepted-unenforced blocker
diagnostic; recommended close as a documented intentional difference (Let's Encrypt ended
OCSP service in 2025; the fleet uses short-lived ACME certificates; nginx defaults
`ssl_stapling off`). If a future requirement needs it: add the verifier crate + refresh
owner as one L wave.

### 4.5 Stream Advanced Gaps (18) - **implementable M/L**

| Sub-family | Rows | Path | Effort |
| --- | ---: | --- | --- |
| Upstream TLS client | 7 (`proxy_ssl`, `_certificate`, `_certificate_key`, `_name`, `_server_name`, `_verify`, `_trusted_certificate`) | Reuse the now-proven upstream TLS policy builder (HTTP proxy + uWSGI 7B) and add a `tokio-rustls` client handshake in `stream_proxy.rs`; verify by default unless the operator explicitly disables it (documented stricter-than-nginx default) | M |
| Transport controls | 5 (`proxy_bind`, `proxy_half_close`, `proxy_next_upstream_timeout`, `proxy_next_upstream_tries`, `proxy_protocol_timeout`) | `socket2` bind (already a gateway dependency); half-close via per-direction `shutdown`; retry budget reuses the HTTP `UpstreamRetryConfig`; `proxy_protocol_timeout` bounds an inbound PROXY v1 read using the mail substrate's `ProxyProtocolV1` parser (already committed) | S/M |
| Rate / counters | 4 (`proxy_download_rate`, `proxy_upload_rate`, `proxy_requests`, `proxy_responses`) | Token-bucket throttling in the stream copy loop (reuse limit_req math) and per-connection counters in the stream metrics; `proxy_requests`/`proxy_responses` gate connection lifetime | S/M |
| Routing | 2 (`pass`, `server_name`) | `server_name` needs a per-listener stream-server table selected from the preread SNI; `pass <addr>` needs bounded in-process handoff to another stream listener (no loop). Only structural rows | L |

Evidence: extend `tests/nginx/differential-stream` (9/9) with TLS-upstream, PROXY-header,
half-close/rate/counter and SNI-routing cases; integration tests in `tests/stream_proxy.rs`.
**Verdict: implementable; one L wave (2 structural rows) or M + L split.**

### 4.6 Static Open-File Cache (3) - **not a gap (runtime-owned)**

`open_file_cache`, `open_file_cache_min_uses`, `open_file_cache_valid` are audit
`runtime-owned`, not behavior-gap. nginx documents the cache as a performance cache with
deliberately staleness-tolerant metadata; response bytes are unchanged and the runtime
already owns the static-file open/walk policy (`data_plane/static_path.rs`). Implementing an
fd cache would add OS-state management for a tuning-only knob. **Verdict: no work; keep the
runtime-owned decision and the operator-matrix note.** (`open_file_cache_errors` and
`open_file_cache_events` are already fail-closed with the same rationale.)

### 4.7 Protocol Residual Forms (29: FastCGI 6, uWSGI 11, SCGI 4, gRPC 8)

The landed protocol adapters are complete for their implemented forms; the residual rows
split into five semantic classes, which is why one shared toolkit closes them:

| Class | Rows | Verdict | Path |
| --- | ---: | --- | --- |
| `*_ignore_client_abort` (FastCGI/uWSGI/SCGI) | 3 | implementable S | adapter flag: keep reading the upstream response when the client disconnects (bounded by response limits) rather than cancelling |
| `grpc_bind`, `grpc_socket_keepalive` | 2 | implementable S | the shared transport connector already implements IP-literal bind + `SO_KEEPALIVE` for the CGI family; wire the same helper into the gRPC adapter |
| Bounded temp-spool family (`*_max_temp_file_size`, `*_temp_file_write_size`, `*_temp_path` for FastCGI/uWSGI/SCGI) | 9 | implementable L | New bounded temp-spool subsystem: per-worker spool root confinement (operator-configured root, no `..`/symlink writes), write/rename/cleanup lifecycle, disk-pressure fail-closed policy, and response streaming from the spool. This is the one new subsystem in the in-profile backlog |
| Logging/platform no-op (`fastcgi_catch_stderr`, `fastcgi_send_lowat`) | 2 | runtime-owned (close by decision) | the runtime is a protocol client with no child-process stderr channel (`catch_stderr`); `SO_SNDLOWAT` is not a portable, meaningful TCP tuning knob on supported platforms. Keep fail-closed/located diagnostic |
| pinned-rustls TLS knobs: gRPC `ssl_ciphers`, `ssl_conf_command`, `ssl_crl`, `ssl_password_file`, `ssl_session_reuse`; uWSGI the same five + `ssl_verify_depth` | 11 | blocked by pinned rustls (close fail-closed) | OpenSSL cipher strings / CRL / session-reuse / encrypted-key / chain-depth policy have no faithful rustls 0.23.46 mapping at the pinned API surface; `ssl_conf_command` has no rustls equivalent at all |
| config-time material cache: `uwsgi_ssl_certificate_cache`, `grpc_ssl_certificate_cache` | 2 | runtime-owned | nginx caches loaded certificate files; the runtime loads at config compile/rotation, so the value has no client-visible effect |

**Row check: 3 + 2 + 9 + 2 + 11 + 2 = 29.** The implementable delta is 14 rows
(3 S + 2 S + 9 L); the other 15 close by documented decision (11 fail-closed on the pinned
TLS stack + 4 runtime-owned).

### 4.8 Mail Module Residuals (23) - **10 implementable, 1 bounded, 12 pinned-blocked**

The mail proxy is in profile with 27 executable directives and a 9-case real-nginx
differential. The residuals:

| Class | Rows | Verdict | Path |
| --- | ---: | --- | --- |
| Capability lists: `pop3_capabilities`, `imap_capabilities`, `smtp_capabilities` | 3 | implementable S | the listener already advertises nginx default capability sets; make the authored extra tokens (and the resulting UTF-8 cap on the advertised list) a config field |
| `resolver`, `resolver_timeout` | 2 | implementable S | mail `auth_http` backend hostname resolution; reuse the bounded runtime resolver already used by dynamic `proxy_pass` |
| `set_real_ip_from` (mail) | 1 | implementable S/M | inbound PROXY v1 already gives the real client IP; the directive adds the trusted-source CIDR set and rewrites the client identity used for `auth_http` |
| Client-certificate flow: `ssl_client_certificate`, `ssl_verify_client`, `ssl_trusted_certificate`, `auth_http_pass_client_cert` | 4 | implementable M | rustls supports client-certificate verification with a CA store; surface the verified peer identity in the generated `auth_http` request header when configured. Needs mail TLS material in the model (present) + differential mail cases |
| `ssl_verify_depth` | 1 | runtime-owned / bounded difference | rustls verifies the full chain and exposes no depth knob; nginx's depth limit is a verification-policy difference. Close by decision with a located diagnostic |
| pinned-rustls TLS knobs: `ssl_certificate_compression`, `ssl_ciphers`, `ssl_conf_command`, `ssl_crl`, `ssl_dhparam`, `ssl_ecdh_curve`, `ssl_password_file`, `ssl_prefer_server_ciphers`, `ssl_session_cache`, `ssl_session_ticket_key`, `ssl_session_tickets`, `ssl_session_timeout` | 12 | blocked by pinned rustls (close fail-closed) | OpenSSL certificate compression/cipher-string/CRL/DH/ECDH/password/session policy has no faithful rustls 0.23.46 mapping at the pinned API surface |

**Row check: 3 + 2 + 1 + 4 + 1 + 12 = 23.** Implementable delta: 10 rows; 12 fail-closed on
the pinned stack; 1 runtime-owned/bounded.

## 5. Out-Of-Profile Feasibility (13 + HTTP/3)

### 5.1 GeoIP - 8 Rows (http 5 + stream 3) - **implementable M after a PRD amendment**

Plan when/if the profile admits a GeoIP module:
- Dependency: a MaxMind DB reader (`maxminddb` crate — **not** in `Cargo.lock`; a named
  supply-chain decision), memory-mapped lookup with a bounded per-lookup cost.
- Database lifecycle: operator-configured path, existence/permission validation at config
  compile, atomic reload/rotation on file change (same pattern as certificate material),
  fail-closed startup when the configured DB is missing/invalid.
- Variables: `$geoip_country_code`, `_code3`, `_name`, `$geoip_city_*`,
  `$geoip_latitude`/`longitude`, `$geoip_org`, and the stream variants, fed into the
  existing bounded variable engine; `geoip_proxy`/`geoip_proxy_recursive` reuse the
  `TrustedProxyConfig`/real-IP machinery.
- Product: MaxMind GeoIP2/GeoLite2 licence and update cadence (test fixtures need a
  synthetic `.mmdb` or a licensed file), plus the PRD §4 amendment.
- Evidence: unit tests for the lookup engine + variable expansion, a config-compile test
  for DB lifecycle, and a synthetic-database differential (nginx `geoip_*` with the same
  synthetic DB) proving the variable values.

**Effort: M** (shared engine http+stream) + legal/data decision + profile amendment.

### 5.2 Perl - 4 Rows - **not implementable in this product**

`perl`, `perl_modules`, `perl_require`, `perl_set` require an embedded Perl interpreter
executing arbitrary Perl inside the edge: libperl FFI (`unsafe`), per-request interpreter
state, module loading from disk, and arbitrary-code security posture. That contradicts the
memory-safe standalone-edge model (`RUST_CODE_SPEC` unsafe policy) and PRD §4 ("embedded
executable configuration"). **Keep excluded; no OSS-runtime equivalent is defensible.**

### 5.3 HTTP/3 + QUIC - 9 Fail-Closed Rows - **XL; profile decision first**

Rows: `http3`, `http3_hq`, `http3_max_concurrent_streams`, `http3_stream_buffer_size`,
`quic_active_connection_id_limit`, `quic_gso`, `quic_host_key`, `quic_retry`,
`events:quic_bpf`.

- QUIC server plumbing is proven in-tree: `quinn` 0.11.12 builds a rustls TLS 1.3 QUIC
  server endpoint for the tunnel transport. Missing is HTTP/3: no `h3`/`h3-quinn` crate is
  in the workspace or `Cargo.lock`.
- A real wave needs: `h3`/`h3-quinn` (supply-chain decision); a UDP listener plane separate
  from TCP listeners (bind/firewall/Docker UDP port mapping, `Alt-Svc` advertisement);
  an h3 request/response adapter feeding the existing compiled routing/handler pipeline;
  QUIC/TLS material (TLS 1.3 only, retry tokens, ticket keys); 0-RTT replay policy and
  `425 Too Early`/`Early-Data` semantics before any mutating request; connection
  migration/observability; and the 9 directive mappings.
- The oracle is possible (`nginx -V` includes `--with-http_v3_module`), but this environment
  has no HTTP/3 client (`curl` lacks HTTP3; no `aioquic`), so the differential harness must
  gain an h3 client first (curl-ngtcp2 or a Python client — a CI/offline dependency
  decision).
- Profile: PRD §4 excludes HTTP/3 while the catalog classifies these rows
  `unsupported-fail-closed`; align the profile/catalog decision before any work.

**Verdict: XL, not part of the in-profile closing sequence.** Residual risks: UDP
ingress/PMTU, amplification/retry hardening, 0-RTT replay, dual-stack UDP deployment, and
new h3 test tooling.

## 6. Closing Sequence To "Nothing Implementable Left"

Dependency-ordered, in-profile only. Each step is one reviewed change set with cargo tests,
its differential/integration evidence, and the Lead's catalog/audit/matrix resync
(`node tools/check-nginx-directive-matrix.mjs --write` then
`node tools/gen-nginx-directive-audit.mjs`), raising the relevant manifest case floor.

| Step | Wave | Rows moved | Executable delta | Other decision delta | Evidence to add |
| ---: | --- | ---: | ---: | --- | --- |
| 1 | Stream advanced (upstream TLS, half-close, rates/counters/retry, bind, inbound PROXY, SNI routing, `pass`) | 18 | +18 | - | differential-stream extended (TLS upstream, PROXY client, half-close/rate/counter/SNI cases) + `stream_proxy` integration tests |
| 2 | Module-sized modules (`userid` 9, `browser` 4, `random_index`, `slice`) | 15 | +15 | `degradation`/`degrade` 2 -> fail-closed (Plus, no OSS oracle) | per-module differential/integration cases (userid cookie wire, browser variables, random index, slice range cache) |
| 3 | HTTP/2 DATA chunking (`http2_chunk_size`) | 1 | +1 | - | raw h2 DATA-frame capture test + mapping |
| 4 | Protocol residual toolkit (ignore_client_abort 3, gRPC bind/keepalive 2, temp-spool 9) | 14 | +14 | 4 -> runtime-owned (`fastcgi_catch_stderr`, `fastcgi_send_lowat`, 2 `*_ssl_certificate_cache`); 11 -> fail-closed (pinned rustls knobs) | differential-protocols cases + temp-spool integration tests + socket/bind tests |
| 5 | Mail residuals (capabilities 3, resolver 2, realip 1, client-cert flow 4) | 10 | +10 | 1 -> runtime-owned (`ssl_verify_depth` bounded); 12 -> fail-closed (pinned rustls knobs) | differential-mail cases (capabilities, client-cert flow, real IP, resolver) |
| **in-profile end** | | **58** | **+58 executable** | 7 blocked rows remain behavior-gap | |
| 6 | Consolidation (Lead-owned) | 0 | 0 | Record the final 7 dependency-blocked rows as explicit decisions; regenerate catalog + audit + matrix + gap notes; raise slice floors | blocker diagnostics in operator matrix; audit rationale |

**Counter projection.**

Current: `enforced 328 / runtime-owned 147 / security-difference 17 / behavior-gap 95 /
out-of-profile 13 / fail-closed 144 = 744`.

| Bucket | Current | After steps 1-5 | After step 6 (decision record) |
| --- | ---: | ---: | ---: |
| `enforced` | 328 | **386** | 386 |
| `runtime-owned` | 147 | **152** | 152 |
| `security-difference` | 17 | 17 | 17 |
| `behavior-gap` | 95 | **7** | **7** (all dependency-blocked, explicit rationale) |
| `out-of-profile` | 13 | 13 | 13 |
| `fail-closed` | 144 | **169** | 169 |
| **total** | **744** | **744** | **744** |

Arithmetic: executable +58 (18 + 15 + 1 + 14 + 10); runtime-owned +5 (4 protocol + 1 mail);
fail-closed +25 (2 Plus + 11 protocol-rustls + 12 mail-rustls); behavior-gap -88
(58 executed + 5 runtime-owned + 25 fail-closed = 88); the seven remaining gap rows are the
blocked tail.

Row-level proof of the 95 -> 7 movement:

- 58 rows move to executable (steps 1-5).
- 5 rows move to runtime-owned: 4 protocol (`fastcgi_catch_stderr`, `fastcgi_send_lowat`,
  `uwsgi_ssl_certificate_cache`, `grpc_ssl_certificate_cache`) + 1 mail (`ssl_verify_depth`).
- 25 rows move to fail-closed: 2 Plus (`degradation`, `degrade`) + 11 protocol pinned-rustls
  + 12 mail pinned-rustls.
- 58 + 5 + 25 = 88; **95 - 88 = 7** remaining: `chunked_transfer_encoding` (1) and the six
  dynamic-OCSP rows.

**Catalog target after step 5** (same total): `executable 386`, `accepted-unenforced 176`
(171 + the 5 runtime-owned closures), `unsupported-fail-closed 169`
(232 - 58 executable moves - 5 accepted moves), `excluded 13`.
Sum proof: 386 + 176 + 169 + 13 = 744.

**After step 6** no code remains. The seven blocked rows stay behavior-gap (or move to
fail-closed if the Lead prefers to make the operator impact explicit); either way the
*implementable* backlog is zero and the closing claim becomes: every remaining row is
either implemented, deliberately runtime-owned, an intentional security difference,
out of profile, fail-closed, or blocked by a named pinned dependency.

**Optional profile-expansion program** (after the in-profile close, only with a PRD
amendment and new-dependency approval): GeoIP 8 (excluded -> executable, M) and HTTP/3 9
(fail-closed -> executable, XL). If both land: executable 403, out-of-profile 5 (Perl 4 +
`load_module` 1), fail-closed 160, behavior-gap 7, runtime-owned 152, security-difference
17 = 744. Perl/`load_module` stay the permanent "not implementable" tail.

### 6.1 What Each Step Must Not Do

- No gate lowering: differential case floors only rise; intentional differences are listed
  in the comparator allowlist with a spec reason, never by editing an expectation.
- No silent acceptance: every newly executable directive must execute its bounded form or
  fail closed with a located diagnostic for the forms it does not execute.
- No catalog claim before evidence: the catalog is regenerated from the implementation, then
  the audit; a row moves only when behavioral evidence exists.
- No decision debt: every bounded difference lands in the operator matrix and the catalog
  notes in the same change set.

## 7. Not Implementable In This Product

| Item | Rows | Why not implementable |
| --- | ---: | --- |
| Perl (`perl`, `perl_modules`, `perl_require`, `perl_set`) | 4 | Embedded Perl interpreter (libperl FFI/unsafe, per-request interpreter state, module loading) contradicts the memory-safe standalone edge and PRD §4. |
| Third-party dynamic modules (`load_module`) | 1 | nginx modules are C-ABI plugins against nginx request/hook structs; the Rust data plane has no such ABI. Loading them cannot work and is an unbounded memory-unsafe extension surface. |
| nginx Plus `degradation`/`degrade` | 2 (backlog) | Plus-only module; the OSS reference build does not compile it, so there is no OSS oracle and no reproducible behavior to match. Close fail-closed with a Plus rationale. |
| `ssl_conf_command` (backlog forms: gRPC, uWSGI, mail; plus http/stream fail-closed variants) | 3 backlog rows | OpenSSL command strings (`Options`, `NoAntiReplay`, DH parameters, …) have no rustls equivalent and no portable meaning; honoring them would silently change TLS posture. |
| nginx disk temp-spool buffering | 9 backlog rows (`*_temp_path`, `*_temp_file_write_size`, `*_max_temp_file_size` for FastCGI/uWSGI/SCGI) | **Implementable but L**: the runtime streams bounded responses and owns no disk spool; a spool subsystem must own disk-exhaustion/cleanup/confinement semantics. Listed under §4.7 as the one L subsystem, not as "impossible". |

## 8. Implementable But Blocked By The Pinned Dependency

Pinned versions verified in `Cargo.lock` at this snapshot: `hyper` 1.12.0, `h2` 0.4.20,
`rustls` 0.23.46, `tokio-rustls` 0.26.6, `quinn` 0.11.12, `axum-server` 0.8.0, `socket2`
0.6.5, `reqwest` 0.13.5, `x509-parser` 0.18.1, `tower-http` 0.6.11. Absent:
`maxminddb`, `h3`, `h3-quinn`, and any OCSP-verification crate.

| Item | Rows | Pinned blocker | Upstream fix path | Local fallback (fidelity cost) |
| --- | ---: | --- | --- | --- |
| `chunked_transfer_encoding` | 1 | `hyper` 1.12.0 h1 server encoder has no close-delimited HTTP/1.1 response mode (`proto/h1/role.rs` chooses `chunked` for unknown-length 1.1 responses) | opt-in close-delimited framing in hyper (or a response extension selecting it); then map the directive | (a) bounded buffering to synthesize `Content-Length` (not streaming); (b) HTTP/1.0 status line (wire-visible, do not ship); (c) vendored/patch hyper (supply-chain cost) |
| `ssl_stapling` / `ssl_stapling_verify` / `ssl_trusted_certificate` (http + stream) | 6 | `rustls` 0.23.46 `CertifiedKey.ocsp` is static bytes only: no fetch/refresh/responder-selection/OCSP verification API (CRLs are supported; OCSP is not) | OCSP owner outside rustls: fetch (`reqwest`), DER parse/verify (new crate), refresh scheduler, atomic key swap; or an upstream rustls OCSP verification hook | static `ssl_stapling_file` already executes (wire-proven); dynamic fetch is a documented difference, defensible given the OCSP sunset |
| gRPC pinned-TLS knobs (`ssl_ciphers`, `ssl_conf_command`, `ssl_crl`, `ssl_password_file`, `ssl_session_reuse`) | 5 | OpenSSL cipher strings / encrypted key files / CRL / resumption / command strings have no faithful rustls 0.23.46 mapping at the pinned API surface (`ssl_conf_command` has no equivalent at all) | none for full grammar; bounded named-suite subset and CRL wiring are possible but are a separate product decision | fail closed with a located rustls diagnostic; bounded subset only if the Lead requests it |
| uWSGI pinned-TLS knobs (`ssl_ciphers`, `ssl_conf_command`, `ssl_crl`, `ssl_password_file`, `ssl_session_reuse`, `ssl_verify_depth`) | 6 | same as gRPC; `verify_depth` has no rustls chain-depth knob | same as gRPC; `verify_depth` could only be approximated by a custom verifier | fail closed; keep `ssl_verify` (already executable) as the supported policy |
| mail pinned-TLS knobs (12 `ssl_*` forms listed in §4.8) | 12 | same OpenSSL surface: compression/cipher strings/commands/CRL/DH/ECDH/password/cipher preference/session cache/tickets/timeouts | none for full parity; session tickets are rustls-managed internally, not operator-configured | fail closed with a located rustls diagnostic; the executable mail TLS subset covers certificate/key/protocols/STARTTLS |

Dependency-blocked decisions are only valid for these pinned versions. Any bump of
`hyper`, `rustls`, `tokio-rustls`, or `h2` invalidates the table: re-verify the source
claims before the next catalog regeneration, and move a row to executable if the upstream
surface appears.

## 9. Reproduction And Change Control

All commands are read-only; the first four were exercised at this snapshot (outputs below).
Commands 5-12 need the gateway binary; the last full aggregate run in this session passed
every step when the manifest carried five differential slices, and the mail slice landed
later the same day (see §9.1).

```sh
# 1. Directive inventory (744 rows, nginx 1.29.6).
node tools/gen-nginx-directive-inventory.mjs --check

# 2. Directive matrix gate: catalog <-> mapping.rs, typed-TOML enforcement, fixture
#    coverage, generated operator matrix (12 checks).
node tools/check-nginx-directive-matrix.mjs

# 3. Generated 744-row audit ledger: freshness + evidence verification.
node tools/gen-nginx-directive-audit.mjs --check --verify-evidence

# 4. Static compatibility gates.
node tools/check-nginx-gap.mjs
node --test tools/check-nginx-behavioral-corpus.mjs

# 5. Aggregate release gate: static gates + behavioural corpus + real-world gate +
#    every manifest-discovered differential slice (currently 6).
node tools/run-nginx-compat-gate.mjs

# 6. Real-world production config gate (api.birdcoder.com snapshot).
node tools/check-nginx-real-world.mjs

# 7-12. Differential slices (each also enforces its case floor and pins nginx/1.29.6).
node tools/run-nginx-differential.mjs             # conditional-cache 50
node tools/run-nginx-differential-http.mjs        # broad HTTP 274
node tools/run-nginx-differential-host.mjs        # host selection 39
node tools/run-nginx-differential-protocols.mjs   # CGI/memcached/gRPC 77
node tools/run-nginx-differential-stream.mjs      # stream 9
node tools/run-nginx-differential-mail.mjs        # mail 9

# 13. Counts used by this report.
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
| audit | `audit evidence verification passed: 744 rows, 328 executable evidence references, 249 completed-wave directives, 6 resolved evidence citations, 11 resolved migration over-claims` |
| catalog classifications | `executable 328, accepted-unenforced 171, unsupported-fail-closed 232, excluded 13` |
| real-world gate | `ok: virtualHosts=41 listeners=4 resources=150 upstreams=88 skipped=0` (run at snapshot start) |
| aggregate gate | last full run in this session passed every step against a fresh binary (static gates, behavioural corpus, real-world config and the then-indexed 5 differential slices); the mail slice is indexed now, so the next run executes 6 |

Change control for the closing waves: after each wave, regenerate the matrix and audit
(`node tools/check-nginx-directive-matrix.mjs --write` then
`node tools/gen-nginx-directive-audit.mjs`) and raise the affected slice floor in
`specs/nginx-behavioral-corpus.manifest.json`. The catalog is never hand-edited to claim a
row before its behavioral evidence exists; this report does not edit the catalog, audit,
matrix, manifest, code or tools.

### 9.1 Snapshot Change-Control Note

The worktree at this snapshot contains one uncommitted code change
(`crates/sdkwork-webserver-core/src/config/model.rs`) from a concurrent in-flight wave; it
cannot change any count above until the Lead re-syncs the catalog/audit, and it makes the
gateway binary older than the source tree, which is why the binary-dependent gates were not
re-run live for this report. The catalog/audit content itself is verified current by the
static gates in §9. Before treating this report as a release statement, re-run the
aggregate gate on a fresh binary after the wave lands.

## 10. Open Decisions For The Lead

1. **Pinned-blocked tail:** approve recording the 7 remaining behavior-gap rows
   (`chunked_transfer_encoding` + the six dynamic-OCSP rows) as explicit
   dependency-blocked decisions in the audit taxonomy; then the implementable backlog is
   zero. If a fail-closed carve-out is preferred for `chunked_transfer_encoding`, that is a
   separate operator-impact choice (working nginx configs would be rejected).
2. **nginx Plus rows:** approve moving `degradation`/`degrade` from behavior-gap to
   fail-closed "not implementable (Plus, no OSS oracle)"; catalog + audit sync.
3. **Temp-spool subsystem:** approve the one new in-profile L subsystem (bounded disk
   spool for the CGI family) or record the 9 rows as a documented difference; the
   projection in §6 assumes it is implemented.
4. **New dependencies:** if the optional waves ever start, approve `maxminddb` (GeoIP),
   `h3`/`h3-quinn` (HTTP/3), an OCSP-verification crate (dynamic stapling), and an h3
   differential client. None is needed for the in-profile close.
5. **Manifest metadata:** `specs/nginx-behavioral-corpus.manifest.json` still declares
   `pinnedNginx: "1.26.2"` while every differential runner pins `nginx/1.29.6`
   (`REFERENCE_NGINX`). Update the manifest field (or add a per-slice oracle pin) so the
   manifest and the gate cannot disagree; this report does not edit the manifest.
6. **HTTP/3 profile boundary:** PRD §4 excludes HTTP/3 while the catalog classifies the 9
   rows fail-closed; align the decision (keep excluded / move to explicit fail-closed) even
   if no h3 wave is planned.
7. **Evidence discipline:** every landing change regenerates catalog + matrix + audit and
   raises a manifest case floor; no behavioral gate is lowered to make a wave land.

## Appendix A - Category-To-Verdict Summary

| # | Category | Rows | Verdict | Effort | Owner action |
| --- | --- | ---: | --- | --- | --- |
| 1 | HTTP/1 framing | 1 | blocked by pinned hyper 1.12.0 | L (only if vendored) | record dependency-blocked decision; upstream request |
| 2 | module-sized modules | 17 | 15 implementable; 2 nginx Plus (not implementable) | M (userid/browser), S (random_index), M/L (slice) | implement 15; move 2 to fail-closed |
| 3 | HTTP/2 DATA chunking | 1 | implementable | M | implement + DATA-frame wire test |
| 4 | dynamic OCSP | 6 | blocked by pinned rustls 0.23.46 | M/L | record decision; static `ssl_stapling_file` remains the supported form |
| 5 | stream advanced | 18 | implementable | M (16) + L (routing 2) | implement; extend differential-stream |
| 6 | gRPC residuals | 8 | 2 implementable + 1 runtime-owned + 5 pinned-rustls | S + decision | implement bind/keepalive; close 5 fail-closed |
| 7 | FastCGI residuals | 6 | 1 implementable (`ignore_client_abort`) + 2 runtime-owned (`catch_stderr`, `send_lowat`) + 3 temp-spool L | S/L | temp-spool wave; close 2 by decision |
| 8 | uWSGI residuals | 11 | 4 implementable (1 `ignore_client_abort` S + 3 temp-spool L) + 1 cert-cache runtime-owned + 6 pinned-rustls | S/L | implement 4; close 7 by decision |
| 9 | SCGI residuals | 4 | 1 implementable + 3 temp (L) | S/L | temp-spool wave |
| 10 | mail residuals | 23 | 10 implementable + 1 runtime-owned/bounded + 12 pinned-rustls | S/M | implement 10; extend differential-mail; close 13 by decision |

(Appendix A is a reading aid; §4.7/§4.8 contain the exact per-row allocation and the
95-row arithmetic.)
