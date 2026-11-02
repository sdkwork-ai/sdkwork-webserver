# nginx compatibility re-audit — 2026-11-02

> 结论（中文）：当前实现**不是**“与 nginx 所有功能完美兼容”，这个目标既不可实现也不应对外承诺。
> 在声明的 `http-core-v1` 档位内，它已经具备可商用的主体能力；本次复核发现并修复了 3 个
> 会让发布证据“假绿/假红”的验证缺陷（Windows 行尾、发布门禁未真正 fail-closed、HTTP
> 差分 oracle 被脏工作区污染）。但仍有 2 个 P0/P1 语义缺口（缓存键降级、代理超时被忽略）
> 和 1 个 P0 证据可移植性缺口（protocol 差分只能在原始 `/opt/source` 环境复现）需要在
> 商业签收前处理。

- Audit target: `main` at `4fcac580` (working tree also contains this audit's fixes)
- Reference implementation: nginx **1.29.6** OSS (nginx.org package, extracted under
  `/tmp/nginx-deb/root/usr/sbin/nginx` in WSL for this audit)
- Declared product profile: `http-core-v1` (`docs/product/prd/PRD-nginx-compatibility.md`)
- Previous terminal report: [`2026-11-01-nginx-compatibility-final-acceptance.md`](2026-11-01-nginx-compatibility-final-acceptance.md)
- Machine-readable ledgers: `specs/nginx-directive-support.catalog.json`,
  `specs/nginx-gap.catalog.json`

## 1. Verdict

| Question | Answer |
| --- | --- |
| Perfect compatibility with every nginx feature? | **No.** 23% of the 744 OSS directives are accepted-but-inert, 23% fail closed, and 5 are out of profile. nginx Plus, third-party/dynamic modules, Lua/njs/Perl, and HTTP/3 remain outside the product. |
| Is “run any nginx.conf unchanged” a valid claim? | **No.** 169 directives reject an active configuration; 173 load but have no effect (each emits a located diagnostic). |
| Is the declared `http-core-v1` profile commercial-ready? | **Conditionally.** The static contract is coherent and most runtime evidence is green, but two semantic downgrades and one evidence-portability defect require an explicit decision (see §4/§5). |
| Were release-blocking verification defects found? | **Yes, three.** All three are fixed and re-verified (§3). |

### Directive ledger (nginx 1.29.6, 744 OSS directives)

| Classification | Count | Share | Meaning |
| --- | ---: | ---: | --- |
| `executable` | 397 | 53.4% | Runtime executes the directive; committed evidence asserts the behaviour |
| `accepted-unenforced` | 173 | 23.3% | Configuration loads, a located diagnostic is emitted, the value has no effect |
| `unsupported-fail-closed` | 169 | 22.7% | An active occurrence is a source-mapped error |
| `excluded` | 5 | 0.7% | Perl (4) + `load_module` (1) |
| **total** | **744** | 100% | |

Accepted-but-unenforced by declared risk: **20 high / 83 medium / 70 low**.

## 2. What was verified

### 2.1 Static contract (Windows, Node 24.11.1)

| Gate | Command | Result |
| --- | --- | --- |
| Directive matrix (12 checks) | `node tools/check-nginx-directive-matrix.mjs` | **PASS 12/12** |
| Gap catalog | `node --test tools/check-nginx-gap.mjs` | **PASS** |
| Corpus manifest + orchestrator guard | `node --test tools/check-nginx-behavioral-corpus.mjs` | **PASS** |
| Inventory canonical form | `node tools/gen-nginx-directive-inventory.mjs --check` | **PASS** (744 directives) |
| Directive audit / evidence | `node tools/gen-nginx-directive-audit.mjs --check --verify-evidence` | **PASS** (744 rows, 397 evidence refs) |
| Migration CLI | `node --test tools/import-nginx-conf.test.mjs` | **PASS 14/14** |
| Aggregate release gate | `node tools/run-nginx-compat-gate.mjs` | **FAIL (correctly): 9 runtime-evidence steps** |

The Windows aggregate gate fails because the PATH has `cargo.exe` but no `rustc`, so no
behavioural evidence can execute. It now reports nine concrete failures instead of printing
`SKIP` and exiting green.

### 2.2 Runtime evidence (WSL Ubuntu 22.04, gateway built with rustc 1.99.0, nginx 1.29.6)

The gateway binary was built in WSL (`target/debug/sdkwork-api-webserver-standalone-gateway`,
10m58s) and the slices were executed against the committed oracles and a real nginx 1.29.6
package extracted from nginx.org.

| Evidence | Result |
| --- | --- |
| In-process full-surface corpus | **76/76 PASS** |
| Conditional/cache differential | **50/50 PASS**, 0 unexpected |
| Broad HTTP differential | **384/384 PASS**, 0 unexpected (after §3.4 fix) |
| Stream differential | **23/23 PASS** (14 main + 9 split_clients/pass) |
| Mail differential | **36/36 PASS** (9 main + 8 capabilities/realip + 19 client-cert) |
| GeoIP variable-plumbing differential | **5/5 PASS** |
| Real production config (api.birdcoder.com snapshot) | **41 vhosts / 4 listeners / 150 resources / 88 upstreams / 0 skipped** |
| Upstream-protocol differential | **75/89 on this WSL/DrvFs checkout; 14 environment artifacts** (§4.1) |
| Aggregate gate | **FAIL only on the protocol slice**; every other step passed |

The HTTP differential initially failed **377/384** with 14 unexpected differences, all
autoindex. That was a real committed-oracle defect, not a runtime regression; see §3.4.

## 3. Verification defects found and fixed

### 3.1 Windows EOL conversion made 3 of 12 directive-matrix checks fail

The committed nginx contracts are canonical LF, but this checkout uses
`core.autocrlf=true` and had no attribute pinning them, so the working copy was CRLF. The
matrix gate compared raw `readFileSync` text with its own `\n` serialization and failed for:

- `inventory is canonical, digest-valid, and derived from nginx 1.29.6`
- `support catalog covers the inventory exactly (checks a and b)`
- `generated operator matrix is current (check f)`

Fixes:

- `.gitattributes` pins the five generated contracts (`nginx-directive-inventory.json`,
  `nginx-directive-support.catalog.json`, `nginx-gap.catalog.json`,
  `nginx-behavioral-corpus.manifest.json`, `NGINX-COMPATIBILITY-MATRIX.md`) to `text eol=lf`.
- `tools/check-nginx-directive-matrix.mjs` reads them through `readCanonicalText()`
  (normalizes `\r\n`), so the gate is reproducible on a tree checked out before the attribute.
- `tools/gen-nginx-directive-inventory.mjs` compares canonical content, not checkout bytes.

Result: **12/12 matrix checks pass on Windows.**

### 3.2 Four differential runners ignored `SDKWORK_REQUIRE_NGINX_EVIDENCE`

`tools/run-nginx-compat-gate.mjs` exports `SDKWORK_REQUIRE_NGINX_EVIDENCE=1` and did **not**
forward `--require-oracle`. Four orchestrators parsed only the CLI flag, so a missing
python/nginx/gateway binary printed `SKIP` and returned `0` even under the release gate:

- `tools/run-nginx-differential-http.mjs`
- `tools/run-nginx-differential-mail.mjs`
- `tools/run-nginx-differential-protocols.mjs`
- `tools/run-nginx-differential-stream.mjs`

Fixes:

- Every orchestrator treats `--require-oracle` **or**
  `SDKWORK_REQUIRE_NGINX_EVIDENCE=1` as a hard requirement and logs `FAIL` instead of `SKIP`.
- `tools/run-nginx-compat-gate.mjs` also passes `--require-oracle` explicitly.
- `tools/check-nginx-behavioral-corpus.mjs` now asserts that every indexed orchestrator
  references `SDKWORK_REQUIRE_NGINX_EVIDENCE`, so the contract cannot silently regress.

Result: on a host without the runtime evidence, the aggregate gate now fails with 9 concrete
steps instead of passing.

### 3.3 Migration CLI was not part of the release gate

`node --test tools/import-nginx-conf.test.mjs` (14 tests) existed but was not invoked by
`pnpm verify` or the aggregate nginx gate. A regression there could render an
accepted-but-unenforced directive as an enforced TOML key and silently change production
behaviour. The aggregate gate now runs the 14-test suite as a static step.

### 3.4 The broad HTTP oracle was captured from a polluted fixture tree (P0, fixed)

On a clean checkout the HTTP differential failed **377/384** with 14 unexpected differences,
all in the seven autoindex cases (`html`, `html-human-size`, `html-localtime`, `json`,
`jsonp`, `jsonp-no-callback`, `xml`). The committed oracle expected five directory entries:

```
sub (directory), a.txt, two words.txt, x&y.txt, z.log (13 bytes)
```

but the committed fixture contains only `.hidden.txt`, `a.txt`, `two words.txt`, `x&y.txt`.
`sub/` and `z.log` were never committed (`z.log` is ignored by `tests/nginx/.gitignore`), so
the oracle could only ever match the dirty working tree it was captured from. This invalidated
the previous "384/384" claim on any clean checkout.

Fixes:

- Recaptured `tests/nginx/differential-http/nginx-oracle.json` from nginx 1.29.6 against the
  cleaned fixture (`--refresh-oracle`). The recapture changed only the seven autoindex bodies
  semantically; the other 383 cases changed only their volatile `Date` header.
- Added a `--refresh-oracle` guard in `tools/run-nginx-differential-http.mjs` that refuses to
  capture when `git status --porcelain --untracked-files=all --ignored=matching` reports any
  entry under `tests/nginx/differential-http/public`. A future polluted capture fails loudly
  instead of redefining the expected behaviour.
- Re-ran the slice: **384/384 PASS, 0 unexpected**.

### 3.5 A protocol fixture was CRLF-corrupted

`tests/nginx/differential-protocols/public/fallback.txt` was 34 bytes in the working tree
(CRLF) while the committed blob and the oracle expect 33 bytes (LF), so `intercept-404`
compared `body_len 33 != 34` and an ETag mismatch. The file was restored from its index bytes;
`.gitattributes` now pins `tests/nginx/differential-protocols/public/**` and the GeoIP binary
fixtures with `-text`, so future checkouts cannot silently convert them.

## 4. Remaining gaps (not fully fixed in this audit)

### 4.1 P0 — the protocol differential is not reproducible outside its capture environment

On this WSL/DrvFs checkout the upstream-protocol slice passes **75/89**; the 14 failures are
all environment artifacts, not runtime divergences:

| Cause | Cases | Explanation |
| --- | --- | --- |
| Absolute checkout path embedded in `extra_hex` / `Content-Length` | 10 (`cache-uwsgi-hit`, `fastcgi-keepconn-reuse`, `head`, `uwsgi-*`) | The mock echoes `DOCUMENT_ROOT`/`SCRIPT_FILENAME`; the oracle embeds `/opt/source/sdkwork-space/sdkwork-webserver/.../public`. Hex-encoded uWSGI params also carry 2-byte length prefixes, so raw-body normalization cannot fix the echoed `Content-Length`. |
| Fixture mtime / ETag | 1 (`intercept-404`) | The protocol battery does not pin fixture mtimes; the oracle ETag encodes the capture-time mtime. A fresh checkout has a different mtime. |
| File mode on DrvFs | 1 (`d7-store`) | The store contract asserts `644`; `/mnt/d` (DrvFs) reports `777` because it does not implement Unix permissions. |
| Unix domain sockets on DrvFs | 2 (`unix-fastcgi`, `unix-uwsgi`) | `/mnt/d` does not support `AF_UNIX`; the typed-plane mock cannot bind, so the gateway correctly answers 502. |

The partial compare-time path normalization added in this audit
(`tools/run-nginx-differential-protocols.mjs`) fixes the plain-text path cases but cannot
repair the hex length prefixes, mtimes, modes, or sockets. A genuinely portable gate needs:

1. The battery/mocks to canonicalize the fixture root to `@PUBLIC@` **at capture time**, and to
   recompute protocol length prefixes after canonicalization.
2. The protocol battery to pin every fixture mtime (as the HTTP battery already does).
3. The store-mode assertion to be skipped or made filesystem-aware on non-Unix filesystems.
4. The unix-socket cases to run on a native filesystem (CI does; a WSL `/mnt/*` checkout
   cannot), or to be skipped with a loud environment diagnostic rather than a false 502.

Until then, "89/89" is a claim about the original `/opt/source` native Linux checkout, not a
property of the committed repository.

### 4.2 P0 — `proxy_cache_key` is ignored while caching remains active

- Evidence: `crates/sdkwork-webserver-core/src/nginx/mapping.rs:4539` accepts
  `proxy_cache_key` by flipping the global cache flag but never parses or applies the template.
  The runtime key (`crates/sdkwork-api-webserver-standalone-gateway/src/data_plane/cache/key.rs:7`)
  is method + host + path + query + `Vary` inputs.
- Impact: a location with `proxy_cache zone; proxy_cache_key "$cookie_user";` is cached under
  the default key. If the upstream response does not carry a matching `Vary`, a response can
  cross user/tenant boundaries. The directive is classified `accepted-unenforced` and emits a
  diagnostic, so this is a **diagnosed downgrade**, not a silent one — but it is unsafe to ship
  without an explicit decision.
- Recommendation: fail closed when `proxy_cache` is active and the key is not equivalent to
  the nginx default, or implement a bounded template
  (`$scheme/$request_method/$host/$request_uri/$uri/$args/$cookie_*/$http_*`). Add a two-user
  differential proving partition isolation.

### 4.3 P1 — proxy connect/read/send timeouts and retry policy are ignored

- Evidence: `mapping.rs:4530-4535` explicitly accepts `proxy_http_version`, `proxy_buffering`,
  `proxy_read_timeout`, `proxy_send_timeout`, `proxy_connect_timeout`,
  `proxy_request_buffering` as no-ops. The typed TOML importer maps `proxyReadTimeout`, but the
  catalog records `tomlEnforced: false` (`http:proxy_read_timeout`). Runtime defaults are 5 s
  connect / 30 s request-to-headers (`crates/sdkwork-webserver-core/src/config/model.rs:56`,
  `:2241-2244`).
- Contradiction: the product's own production sidecar declares
  `proxy_read_timeout 120s; proxy_send_timeout 120s;`
  (`deployments/webserver/snippets/gateway-locations.production.conf:46-47`), and
  nonproduction declares `proxy_read_timeout 300s;`
  (`gateway-locations.nonproduction.conf:33`). Those values are not enforced.
  `NGINX_SPEC.md` §2 requires long read/send timeouts for streaming/generation routes.
- Impact: a slow first byte or long-poll beyond 30 s can be answered with 504 even though the
  operator configured 120/300 s. nginx's `proxy_read_timeout` is an idle-between-reads timer,
  while the runtime's 30 s is a request-to-headers deadline, so the semantics also differ.
- Recommendation: implement per-location connect/read timeout mapping to the upstream
  execution policy (at minimum a first-byte deadline and an idle-body deadline), or stop
  emitting long timeouts in the platform sidecars and document the fixed 30 s ceiling.

### 4.4 P1 — 20 high-risk TLS policy directives are accepted but inert

The high-risk accepted-unenforced set is dominated by TLS policy:

- `ssl_ciphers`, `ssl_prefer_server_ciphers`, `ssl_session_cache`, `ssl_session_tickets`,
  `ssl_stapling`, `ssl_stapling_verify`, `ssl_trusted_certificate` (http)
- the same seven on the stream plane
- `proxy_ssl_ciphers`, `proxy_ssl_protocols` (http and stream)
- `open_file_cache`, `proxy_next_upstream`

Impact: rustls owns the TLS policy, so `ssl_session_tickets off;` does not disable tickets and
`ssl_stapling on;` does not produce a staple. A compliance review that reads the nginx
configuration will believe a control is active when it is not. The diagnostics and matrix do
state this, and rustls defaults are modern, but the **operator's stated policy is not
enforced**.

Recommendation: for directives whose only purpose is a security/compliance control
(`ssl_session_tickets off`, `ssl_stapling on`, `ssl_stapling_verify on`,
`ssl_trusted_certificate`), fail closed unless the operator explicitly acknowledges the
rustls policy; otherwise document them as “runtime-owned TLS policy”, not “accepted”.

### 4.5 P1/P2 — 169 fail-closed directives include implementable common features

The terminal report's “remaining 34 behaviour-gap rows are all blocked by pinned dependencies
or nginx Plus” is true only for the *behaviour-gap* ledger. The gap catalog's note “No
implementable directive remains” is broader and is not supported by the inventory: the
fail-closed set still contains features a commercial nginx edge commonly needs and that are
not dependency-blocked:

- rate shaping: `limit_rate`, `limit_rate_after`, `proxy_limit_rate`
- access composition: `satisfy`, `limit_except`, `auth_delay`
- request-shape control: `proxy_set_body`, `proxy_pass_request_body`,
  `proxy_socket_keepalive`, `proxy_bind`
- response filters: `gunzip`, `add_trailer`, `early_hints`
- proxy-cache policy: `proxy_cache_bypass`, `proxy_no_cache`, `proxy_cache_methods`,
  `proxy_cache_min_uses`, `proxy_cache_lock`, `proxy_cache_revalidate`,
  `proxy_cache_convert_head`, `proxy_cache_background_update`,
  `proxy_cache_max_range_offset`
- module features: DAV (4), SSI (7), image filter (7), MP4 (4), FLV (1), HTTP/3 (8)
- low-value tuning/diagnostics: `rewrite_log`, `uninitialized_variable_warn`,
  `referer_hash_*`, `open_log_file_cache`, `limit_*_dry_run`

Recommendation: correct the wording to “no implementable row remains in the 34-row
behaviour-gap ledger”; publish a value/effort backlog for the rest. Prioritise proxy-cache
policy, timeouts, `limit_rate`, `satisfy`/`limit_except`, and `proxy_set_body`/`gunzip`.

### 4.6 P2 — three capability areas remain missing

`specs/nginx-gap.catalog.json` still marks:

- `http.websocket.rfc8441` (HTTP/2 extended CONNECT WebSockets) — missing
- `tls.ocsp-stapling` / `tls.ocsp-assessment` — missing (static `ssl_stapling_file` is
  `partial`)
- `ops.reload.listener-handoff` — missing (same-topology reload only)

These are already documented; they are not regressions. They are commercial gaps only if the
target market requires HTTP/2 WebSockets, dynamic OCSP, or zero-downtime listener topology
changes.

### 4.7 P3 — committed `Cargo.lock` was stale

`cargo build` updated `Cargo.lock` to add `tracing` to `sdkwork-routes-iam-open-api`, whose
sibling manifest declares it. A release build that uses `--locked` (or a reproducibility
audit) would fail on the previous lock. The lock was refreshed by the build; release tooling
should add `--locked` and CI should verify the lock is current.

## 5. Commercial go / no-go

### Go — for the declared `http-core-v1` profile, with conditions

1. Run the release gate on a **native Linux checkout at the canonical CI path** with a built
   gateway, python3, and the pinned nginx 1.29.6 oracle; archive the output. The WSL `/mnt/*`
   path cannot execute the unix-socket/mode-dependent protocol cases.
2. No production configuration uses a non-default `proxy_cache_key` while caching is active
   (§4.2), or §4.2 is fixed before rollout.
3. Streaming/generation routes set their runtime timeout through the typed upstream
   configuration, or accept the 30 s first-byte ceiling, and the platform sidecars stop
   advertising 120/300 s (§4.3).
4. TLS policy differences are accepted in writing by the operator/compliance owner (§4.4).
5. The 14 protocol environment artifacts are either fixed in the harness (§4.1) or explicitly
   excluded from the release claim with the native-Linux result attached.

### No-go

- “Perfect compatibility with all nginx features.”
- “Any nginx configuration can run unchanged.”
- “The nginx timeouts/TLS policy in the sidecar are enforced by the runtime.”
- “384/384 / 89/89 reproduce on any clean checkout” — the protocol slice still does not, and
  the HTTP slice only reproduces after the oracle recapture in this audit.

The operator-facing authority remains
`docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`; marketing, UI, and release notes must
name the `http-core-v1` profile and the evidence revision.

## 6. Reproduction

```sh
# Static contract (runs on Windows and Linux)
node tools/check-nginx-directive-matrix.mjs
node --test tools/check-nginx-gap.mjs
node --test tools/check-nginx-behavioral-corpus.mjs
node tools/gen-nginx-directive-inventory.mjs --check
node tools/gen-nginx-directive-audit.mjs --check --verify-evidence
node --test tools/import-nginx-conf.test.mjs

# Release gate (requires rustc/cargo, python3, and a built gateway)
node tools/run-nginx-compat-gate.mjs

# Refresh the HTTP oracle only from a clean fixture tree
node tools/run-nginx-differential-http.mjs --refresh-oracle
```

Environment note: this session built the gateway in WSL (10m58s) and ran the slices there.
Windows itself has no `rustc`, so only the static gates run there. The HTTP differential now
passes 384/384; the protocol differential remains path/filesystem-dependent as described in
§4.1.

## 7. Recommended next actions (ordered)

1. **§3.4 regression guard** — keep the dirty-fixture refusal and add it to the other
   differential runners that capture oracles.
2. **§4.1** — canonicalize fixture roots and pin mtimes at protocol capture time; move the
   unix-socket fixtures to a native temp directory; make the store-mode assertion
   filesystem-aware. Then recapture the protocol oracle and require 89/89 on a clean tree.
3. **§4.2** — fail closed on non-default `proxy_cache_key` when caching is active; add loader
   and differential tests.
4. **§4.3** — honour `proxy_connect_timeout` / `proxy_read_timeout` per location, or align the
   sidecars and documentation with the fixed 30 s ceiling.
5. **§4.4** — decide the TLS-policy contract (fail closed for security controls or require
   explicit acknowledgement) and update the matrix wording.
6. **§4.5** — correct the “no implementable directive remains” claim and publish the remaining
   backlog by value/effort.
7. **§4.7** — add `--locked` to release builds and a lock freshness gate.
8. Add a Windows job that runs the six static compatibility gates, so the EOL regression fixed
   here cannot return unnoticed.
