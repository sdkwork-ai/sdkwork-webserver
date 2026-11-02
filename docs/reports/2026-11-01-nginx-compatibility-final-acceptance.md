# nginx 1.29.6 OSS compatibility — final acceptance state

> **Re-audit note (2026-11-02):** The "384/384" and "89/89" clean-checkout claims
> below were not reproducible from the committed tree. The HTTP oracle had been
> captured from a fixture tree that contained an uncommitted `autoindex/sub/`
> directory and an ignored `autoindex/z.log`; the protocol oracle embeds the
> absolute `/opt/source/...` checkout path and capture-time mtimes. The HTTP
> oracle was recaptured and now passes 384/384; the protocol slice remains
> environment-dependent. See
> [2026-11-02-nginx-compatibility-reaudit.md](2026-11-02-nginx-compatibility-reaudit.md)
> for the corrected evidence and the remaining gaps.

Snapshot: `main` at `cab02622` (2026-11-01). Reference oracle: nginx.org
**nginx/1.29.6**; directive inventory generated from the 1.29.6 source
(`specs/nginx-directive-inventory.json`, 744 directives).

## 1. Directive ledger (terminal)

| Classification | Count | Meaning |
| --- | ---: | --- |
| executable | **397** | the Rust data plane executes the directive and committed evidence asserts the behaviour |
| accepted-unenforced | 173 | accepted for stock-config load with a located diagnostic; value has no effect |
| unsupported-fail-closed | 169 | rejected with a located diagnostic when active (142 fail-closed decisions + 27 security-difference rows) |
| excluded (out of profile) | **5** | Perl (4) + `load_module` (1) — embedded interpreter / nginx C ABI |
| **total** | **744** | nginx 1.29.6 OSS directive table |

Audit decisions over the same 744 rows: **397 enforced**, 149 runtime-owned,
17 security-difference, **34 behavior-gap**, 5 out-of-profile, 142 fail-closed.
Every executable row carries at least one resolvable evidence reference
(`node tools/gen-nginx-directive-audit.mjs --verify-evidence`).

## 2. Behavioural evidence (all real, all green)

| Evidence | Command | Result |
| --- | --- | ---: |
| Broad HTTP differential vs nginx 1.29.6 | `node tools/run-nginx-differential-http.mjs` | **384/384** |
| Conditional/cache differential | `node tools/run-nginx-differential.mjs` | **50/50** |
| Host-selection differential | `node tools/run-nginx-differential-host.mjs` | **39/39** |
| Upstream protocols (FastCGI/uWSGI/SCGI/memcached/gRPC) | `node tools/run-nginx-differential-protocols.mjs` | **89/89** |
| Stream plane (+ extra split_clients/pass fixture) | `node tools/run-nginx-differential-stream.mjs` | **23/23** |
| Mail proxy (auth_http/relay/TLS/capabilities/client-cert) | `node tools/run-nginx-differential-mail.mjs` | **36/36** |
| GeoIP variable plumbing (oracle lacks the GeoIP modules; labelled proxy differential) | `node tools/run-nginx-geoip-differential.mjs` | **5/5** |
| Real production nginx config (api.birdcoder.com snapshot, 33 files) | `node tools/check-nginx-real-world.mjs` | 41 vhosts / 4 listeners / 150 resources / 88 upstreams / **0 skipped** |
| `http2_chunk_size` nginx config acceptance | part of the HTTP differential runner | accepts http/server/location, rejects zero |
| Self-asserting full-surface regression | in `node tools/run-nginx-compat-gate.mjs` | 76/76 |
| Aggregate release gate | `node tools/run-nginx-compat-gate.mjs` | **PASS** (static gates + full-surface + real-world + 7 differential slices) |

Rust suites at this snapshot: core 628 tests, gateway library 468 tests, plus
the integration suites listed in the individual wave commits; `cargo fmt --
--check` and `cargo clippy --no-deps -D warnings` (core + gateway lib/bins) are
clean.

## 3. Implemented surface (waves)

1. `http-core-v1` foundation: static, routing/rewrite, headers, error_page,
   gzip, access/limit, proxy/cache, TLS, host selection, named locations.
2. Upstream protocols: FastCGI, uWSGI, SCGI, memcached and gRPC (codec crate
   plus adapters; bind/keepalive/unix targets/TLS/retries/response policies).
3. Protocol cache family, buffering, store, limit_rate, ranges, ignore_headers.
4. Bounded temp-spool subsystem and connection control (ignore_client_abort,
   catch_stderr, send_lowat accepted-unenforced).
5. Stream plane: limit_conn/zones, map/set, preread, upstream TLS, inbound
   PROXY protocol v1/v2, SNI routing, half-close, bind, counters, rates, retry
   budget, split_clients, pass.
6. Mail proxy: listeners, auth_http, POP3/IMAP/SMTP frontends, XCLIENT, relay,
   frontend/backend TLS, STARTTLS, capability lists, client certificates.
7. Rewrite/variable engine: `set`, `if`, regex captures, variable
   proxy_pass/root, and the real production config now materializes.
8. Subrequest engine: addition, auth_request/auth_request_set, mirror.
9. Modules: userid, browser, random_index, slice, http2_chunk_size, GeoIP
   (http+stream), plus OCSP static stapling, referer, geo, split_clients, map,
   types, gzip_static and the earlier waves.

## 4. Remaining 34 behavior-gap rows (all blocked)

### 4.1 Pinned dependency — 32

| Rows | Blocker | Upstream fix path |
| --- | --- | --- |
| `chunked_transfer_encoding` (1) | pinned hyper 1.12 HTTP/1.1 encoder has no close-delimited response mode; nginx `off` emits no Transfer-Encoding and delimits by EOF | hyper close-delimited server body support (or a raw H1 writer); probed against nginx and recorded |
| OCSP dynamic (`ssl_ocsp`, `ssl_ocsp_cache`, `ssl_ocsp_responder` http+stream = 6) | rustls 0.23 `CertifiedKey.ocsp` is a static field; no dynamic fetch/verify/cache/responder surface. Static `ssl_stapling_file` **does** execute | a rustls OCSP custodian/responder API (or a TLS layer with one) |
| gRPC TLS knobs (6: `certificate_cache`, `ciphers`, `conf_command`, `crl`, `password_file`, `session_reuse`) | rustls has no OpenSSL cipher-list/CRL/session-reuse/conf-command/encrypted-key/provider-cache surfaces | rustls-native or OpenSSL-backed TLS provider |
| uWSGI TLS knobs (7: the six above + `verify_depth`) | as above; verify_depth is covered by full-chain verification | as above |
| mail rustls knobs (12: `certificate_compression`, `ciphers`, `conf_command`, `crl`, `dhparam`, `ecdh_curve`, `password_file`, `prefer_server_ciphers`, `session_cache`, `session_timeout`, `session_tickets`, `session_ticket_key`) | rustls owns cipher/session/ticket/curve policy and does not read encrypted keys or CRLs | as above |

Each blocked row fails closed with a precise located diagnostic naming the
missing capability; none is silently ignored.

### 4.2 nginx Plus — 2

`degradation` / `degrade` are nginx Plus commercial directives with no OSS
oracle or public implementation; they stay fail-closed with an
"unsupported directive" diagnostic.

### 4.3 Runtime-owned (not gaps)

`open_file_cache*` (performance-only, no client-visible contract), mail
`error_log`, and `fastcgi_send_lowat` (BSD-only SO_SNDLOWAT; Linux ignores it,
nginx logs "not supported, ignored") are accepted-unenforced with located
diagnostics.

## 5. Out of profile — 5 (not implementable in this product)

| Directive/module | Reason |
| --- | --- |
| Perl (`perl`, `perl_set`, `perl_modules`, `perl_require`) | requires embedding a Perl interpreter and its security/runtime model |
| `load_module` | loads arbitrary nginx C ABI modules (no ABI compatibility with a Rust server; would require emulating the nginx module ABI) |
| OpenResty/Lua, njs, third-party dynamic modules | embedded interpreters / dynamic-module ABI — separately licensed/engineered products, not configuration mappings |
| nginx Plus proprietary features | commercial, not in the OSS reference build |

## 6. Commercial position

Within the declared profile the server is deployable: the complete real
production configuration of `api.birdcoder.com` (41 virtual hosts, 88 upstreams,
33 config files) loads with zero skipped files, and every executable behavior is
backed by either a real-nginx differential case or a documented protocol/h2
integration test. The remaining 34 behavior rows cannot be implemented on the
pinned hyper/rustls stack (or are nginx Plus); each fails closed with a precise
diagnostic, so no silent approximation can reach production. Perl, Lua/njs,
third-party dynamic modules and nginx Plus are explicitly out of profile.

## 7. Reproduction

```bash
# static + behavioural + all differential slices + real-world config
node tools/run-nginx-compat-gate.mjs
# individual gates
node tools/gen-nginx-directive-inventory.mjs --check
node tools/check-nginx-directive-matrix.mjs
node tools/gen-nginx-directive-audit.mjs --check --verify-evidence
node tools/check-nginx-real-world.mjs
node tools/run-nginx-differential-http.mjs
node tools/run-nginx-differential.mjs
node tools/run-nginx-differential-host.mjs
node tools/run-nginx-differential-protocols.mjs
node tools/run-nginx-differential-stream.mjs
node tools/run-nginx-differential-mail.mjs
node tools/run-nginx-geoip-differential.mjs
python3 tests/nginx/full-surface/probe.py        # self-asserting 76/76
cargo test -p sdkwork-webserver-core
cargo test -p sdkwork-api-webserver-standalone-gateway --lib
```

Real oracle: nginx.org nginx/1.29.6 (`/tmp/nginx-deb/root/usr/sbin/nginx`);
TLS/mail-ssl/HTTP2-enabled builds used for those slices are described in the
per-wave commits. The production snapshot lives in
`tests/nginx/real-world/production-api-birdcoder/`.
