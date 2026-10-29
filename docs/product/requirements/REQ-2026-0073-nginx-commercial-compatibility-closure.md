# REQ-2026-0073 Nginx Commercial Compatibility Closure

```yaml
id: REQ-2026-0073
title: Close the release-blocking nginx compatibility gaps and publish executed compatibility evidence
owner: sdkwork-webserver
status: accepted
source: nginx-commercial-compatibility-closure (differential oracles: nginx 1.29.6, Linux x86_64)
problem: |
  The product claimed an nginx-compatible public edge (`http-core-v1`), but the claims were
  only partly executable and partly unimplemented:

  1. The real-nginx differential gate was red on a clean Linux checkout. The committed oracle
     had been captured from a CRLF working tree of `tests/nginx/differential/public/big.txt`,
     while the committed fixture is LF: every strong ETag, `Content-Length`, body length and
     range total then differed (103 unexpected differences), so `check:nginx-differential`
     could never pass at release time. Worse, no aggregate verification entrypoint executed
     the nginx gates at all — `_sdkwork:check`/`_sdkwork:verify` never invoked the gap catalog,
     corpus, differential or behavioural runners (audit finding T8), so a release could ship
     while every compatibility claim was unexecuted.
  2. PRD-nginx-compatibility §5 requires nginx host selection over exact, leading-wildcard,
     trailing-wildcard and regex `server_name` forms in authored order. The implementation
     only matched exact and leading-wildcard names and failed closed on the rest, so a common
     real configuration could not migrate. §6 requires named locations and bounded internal
     redirects; only the Adaptive Web `@pc`/`@h5` special case existed.
  3. PRD §3/§13/§14 require a machine-readable, per-directive support catalog and a published
     operator support matrix. Only a capability-level gap catalog existed and no gate kept a
     published matrix in sync with the implementation.
  4. The migration CLI carried a second, drifting classifier (`MAPPED`/`BLOCKED`): it reported
     `expires` as unsupported (the runtime executes it), reported `types` block bodies as
     unknown directives, and rendered nginx directives into typed TOML keys that the runtime
     accepts but does not enforce (`ocspStapling`, `ssl_ciphers`→`ciphers`, `map`, …), turning
     one silent no-op into another.
  5. The typed TOML plane silently dropped behaviour-affecting accepted keys (TLS cipher/session
     policy, OCSP stapling, proxy retry/header/buffering policy, `map`, autoindex, charset,
     MIME defaults). The nginx-conf import plane already produced a diagnostic per accepted
     directive; the TOML plane produced nothing, contradicting PRD §11.
goals:
  - Make the real-nginx differential oracle deterministic and self-checking across platforms, and make every nginx compatibility gate part of release verification with fail-closed "evidence required" semantics.
  - Implement nginx host-selection parity for exact, leading-wildcard, trailing-wildcard and regex `server_name` forms with PRD §5 precedence and case-insensitive matching, proven against a real nginx binary.
  - Implement bounded named-location serving targets (`try_files … @name`, `error_page … = @name`) with one-hop internal redirect, compile-time cycle rejection and phase-order access control.
  - Publish an implementation-derived directive-level support catalog for nginx 1.29.6 (744 OSS directives), a generated operator matrix, and a drift gate binding catalog, inventory, CLI classifier and implementation code together.
  - Report accepted-but-unenforced behaviour keys on the typed TOML plane instead of dropping them silently.
  - Add a broad real-nginx differential slice for static delivery, routing/rewrite, response headers, error pages, gzip negotiation, access control and proxy semantics, fixing real divergences it exposes.
  - Close the response-semantics gaps the differential slice escalated: full nginx `add_header` semantics (http/server/location inheritance, status gating, `always`, duplicates and variables), directory-without-index 403, and `error_page` mapping for statuses produced by `return`/redirect routes.
  - Implement directive-by-directive completion waves from the audit ledger: Wave 1 `map`/`types`/`default_type`/`gzip_static`/`gzip_vary`; Wave 2 redirect policies (`absolute_redirect`/`port_in_redirect`/`server_name_in_redirect`), `limit_req_status`/`limit_conn_status`, `max_ranges`, certificate `chainFile` consumption, typed `[[http.maps]]`; Wave 2b proxy response-header policies (`proxy_hide_header`/`proxy_pass_header`/`proxy_redirect`/`proxy_cookie_path`/`proxy_cookie_domain`), `internal`, `merge_slashes`, `underscores_in_headers`/`ignore_invalid_headers`; Wave 3a/3a-2 charset family (`charset`/`charset_types`/`source_charset`/`override_charset`), gzip refinements (`gzip_disable`/`gzip_proxied`), autoindex family, `empty_gif`, `stub_status`; Wave 3b `http2_max_concurrent_streams`, `proxy_method`, stream `allow`/`deny`/`return`, truthful reclassification of the HTTP/2 directives nginx made obsolete and precise OCSP diagnostics; Wave 4a referer/geo/split_clients; upstream-protocol wave FastCGI/uWSGI/SCGI/memcached end-to-end with 26/26 real-nginx differential (gRPC next). Every one carries real-nginx differential evidence, and the audit ledger records the decision for all 744 directives.
non_goals:
  - Full nginx OSS behavioural parity (third-party modules, OpenResty/Lua/njs, FastCGI/uWSGI/SCGI/memcached, mail proxy, HTTP/3), which PRD §4 keeps out of profile.
  - `tls.ocsp-stapling`, `http.websocket.rfc8441` and `ops.reload.listener-handoff`, which remain cataloged missing capabilities with documented commercial impact and workarounds.
  - Incremental migration of the fleet's `ocspStapling = true` declarations into an OCSP implementation; this requirement only stops those declarations from being silently ignored.
  - Per-directive inheritance modelling in the catalog (contexts/grammar/runtime field/evidence are recorded; inheritance semantics remain a residual PRD §3 gap).
users:
  - web server application authors migrating real nginx configurations
  - node operators and release verification
  - commercial support and compliance review
acceptance_criteria:
  - The differential oracle is deterministic and self-checking: the fixture is byte-pinned (`-text`), the baseline records and asserts the reference binary (`nginx/1.29.6`), the gate fails when the fixture byte size no longer matches the recorded representation size, and the battery reports 50/50 matched with only allowlisted known differences (0 unexpected).
  - `node tools/run-nginx-compat-gate.mjs` is the single release entrypoint and is wired into `_sdkwork:verify`; it executes the gap catalog, corpus manifest, directive matrix, in-process behavioural corpus and every manifest-indexed differential orchestrator with `SDKWORK_REQUIRE_NGINX_EVIDENCE=1`, so a skipped probe fails the release.
  - nginx `server_name` parity: exact beats wildcard, longest leading wildcard wins, trailing-wildcard and dot-prefix forms match nginx, regex names are evaluated in authored order and case-insensitively, and the listener default server answers otherwise — each rule covered by a core test and by a committed `host-selection` differential slice with a case floor.
  - Named locations: `try_files … @name` and `error_page … = @name` dispatch to a named location with a serving behaviour, one bounded internal redirect, no cycle, and preserved access-control/auth/limit filters; unsupported forms fail closed with a source diagnostic.
  - `node tools/check-nginx-directive-matrix.mjs` passes: 744 inventory directives are all classified (79 executable / 183 accepted-unenforced / 253 fail-closed / 229 excluded), every executable entry has a real evidence reference, fixtures only use catalogued directives, the generated matrix is current, the CLI consumes the catalog and never renders an unenforced typed key, and the catalog's typed-TOML unenforced set equals `server_toml::UNENFORCED_BEHAVIOR_TOML_KEYS`.
  - `node tools/run-nginx-differential-http.mjs` passes with 0 unexpected differences against real nginx at a 237-case floor; real divergences it exposed are fixed with targeted regression tests.
  - The 39 directives listed in the goals move to `executable` in the directive catalog with runtime field + committed evidence (plus the HTTP/2 obsolete directives are truthfully reclassified as accepted no-ops), so the catalog counts become 116 executable / 171 accepted-unenforced / 228 fail-closed / 229 excluded (744 total), and the generated operator matrix plus the 744-row audit ledger are regenerated and gate-current.
  - `node tools/gen-nginx-directive-audit.mjs --check --verify-evidence` passes and the audit ledger shows one decision row per nginx 1.29.6 directive, including the remaining behavior-gap list with wave/priority.
  - `add_header` is nginx-exact for http/server/location: a level that declares any `add_header` replaces the inherited array, declarations keep authored order (duplicate names allowed), the nginx default status list is honoured, `always` applies to every status including errors, the bounded variable subset expands, and the headers are applied on every response kind (static, proxy, cache, return/redirect, error pages, named-location hops, generated and access-control rejections). Unknown variables, non-ASCII values and oversize declarations fail closed at config load.
  - A directory request with no index file answers 403 (nginx autoindex-off) without an ETag, and `error_page` also maps the final status produced by `return`/respond routes and rewrite-generated redirects (Location preserved, representation validators dropped for non-2xx).
  - Typed TOML loads surface `compatibility:` diagnostics for behaviour-affecting accepted keys (`validate` prints the count), while enforced keys and pure process tuning produce none.
  - The published matrix documents every non-executable directive with what the operator loses and a derived migration action that never names an unenforced typed key as an equivalent.
  - The pre-existing evidence corpus does not regress: `python3 tests/nginx/full-surface/probe.py` stays green at its declared 76-case floor, and `cargo test -p sdkwork-webserver-core`, `cargo test -p sdkwork-api-webserver-standalone-gateway --lib`, `cargo fmt -- --check` and the documented clippy gate pass.
non_functional_requirements:
  security: |
    Host selection must not allow a request to reach a virtual host whose `server_name` does not
    cover it; regex names are bounded and compiled once at config load; named-location internal
    redirects must not skip access control, auth_basic, limit_req/limit_conn or header filters.
    Accepted-but-unenforced TLS policy keys (ciphers, session policy, OCSP) must be reported so an
    operator never believes a security control is active when it is not.
  privacy: |
    No new metric label or tenant-visible field is introduced; differential batteries record only
    response metadata and never a credential or request body.
  performance: |
    Host-selection indexes are compiled once per revision (exact hash lookups, bounded wildcard
    lists, regex list evaluated only after exact/wildcard misses); no per-request allocation is
    added to the common exact/bare-host path. The aggregate gate adds no runtime cost.
  reliability: |
    A differential battery must prove which implementation answered on each port (identity
    witnesses) and refuse a busy port, so a comparison can never measure nginx against itself.
    Oracle/fixture drift fails the gate instead of silently redefining expected behaviour.
affected_surfaces:
  - nginx-compat-configuration
  - request-data-plane
  - release-verification
trace:
  specs:
    - NGINX_SPEC.md
    - SDKWORK_WEBSERVER_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
    - CODE_STYLE_SPEC.md
    - PNPM_SCRIPT_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - tests/nginx
    - tools/
verification:
  - node tools/run-nginx-compat-gate.mjs
  - node tools/check-nginx-directive-matrix.mjs
  - node tools/gen-nginx-directive-inventory.mjs --check
  - node tools/run-nginx-differential.mjs
  - node tools/run-nginx-differential-http.mjs
  - node tools/run-nginx-differential-host.mjs
  - node tools/run-nginx-behavioral-corpus.mjs
  - node --test tools/import-nginx-conf.test.mjs
  - python3 tests/nginx/full-surface/probe.py
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway --lib
```

## Evidence And Alignment Findings

- Differential oracle: the committed `tests/nginx/differential/nginx-oracle.json` was recaptured
  from nginx 1.29.6 against the LF-pinned fixture, `.gitattributes` marks every differential
  fixture directory `-text`, `run-nginx.sh` runs an unprivileged package binary by injecting its
  http temp paths, and `tools/run-nginx-differential.mjs` now pins `REFERENCE_NGINX`, asserts the
  baseline came from it and that the fixture byte size still matches the recorded representation.
  The gate went from 4/50 matched with 103 unexpected differences to 50/50 with 0 unexpected.
- Gate exposure: `tools/run-nginx-compat-gate.mjs` discovers differential orchestrators from
  `specs/nginx-behavioral-corpus.manifest.json` (no hard-coded slice list) and is wired into
  `_sdkwork:verify` via `check:nginx-compat`, closing the T8 finding that the nginx gates had no
  execution entry.
- Host selection: see `crates/sdkwork-webserver-core/src/config/server_name.rs`,
  `tests/server_name_selection.rs` and `tests/nginx/differential-host/`.
- Named locations: bounded internal-redirect dispatch is materialized from the same route model;
  cycle/missing-target cases are rejected at compile time and covered by core tests.
- Directive catalog: `specs/nginx-directive-inventory.json` (744 directives parsed from nginx
  1.29.6 `ngx_command_t` tables), `specs/nginx-directive-support.catalog.json`,
  `tools/gen-nginx-directive-inventory.mjs`, `tools/check-nginx-directive-matrix.mjs` (11 checks,
  mutation-tested) and the generated `docs/guides/operator/NGINX-COMPATIBILITY-MATRIX.md`.
- Migration CLI: `tools/import-nginx-conf.mjs` no longer owns a classifier; it consumes the
  catalog, treats `types`/`charset_map`/`map` bodies as data, reports `expires`/`default_type`
  correctly, and refuses to render accepted-but-unenforced typed keys.
- Typed TOML diagnostics: `server_toml::UNENFORCED_BEHAVIOR_TOML_KEYS` and
  `accepted_unenforced_toml_diagnostics`, surfaced through `LoadedWebServerConfig.compatibility`
  and printed by `validate` (`acceptedButUnenforced=` in the summary line).
- Broad differential slice: `tests/nginx/differential-http/` and
  `tools/run-nginx-differential-http.mjs` compare the HTTP surface against real nginx (87 cases,
  0 unexpected differences); real divergences exposed by it are fixed in the data plane with
  targeted regression tests.
- Response-semantics closure: `add_header` now implements nginx's replace-on-declare inheritance,
  default status list, `always`, duplicate ordering and bounded variable expansion on every
  response kind (`CustomHeaderConfig.always`, `RouteConfig.response_headers`); a directory without
  an index answers 403 (`static_path.rs`); `error_page` applies to Respond/Redirect/rewrite
  statuses. Two nginx-generated-416 header-filter quirks are documented as intentional
  differences rather than reproduced. The remaining bounded gap is gzip policy on generated
  error pages (compression layer sits outside the handler); it is documented, not claimed.
- Known capability gaps stay visible: `tls.ocsp-stapling`, `http.websocket.rfc8441` and
  `ops.reload.listener-handoff` remain `missing` in `specs/nginx-gap.catalog.json` and are
  reported in the compatibility matrix; they are not claimed as supported.
- Final directive audit distribution: 116 enforced / 143 runtime-owned / 17 security-difference /
  29 behavior-gap / 229 out-of-profile / 210 fail-closed. The 29 behavior-gap rows each carry a
  concrete architecture blocker (hyper/h2 DATA chunking, rustls OCSP surface, stream zone state /
  variable engine / upstream TLS client, bounded subrequest engine for `addition`, `$invalid_referer`
  threading) and remain fail-closed or accepted-with-diagnostic; none is silently approximated.
- Directive waves: Wave 1 (`map`, `types`, `default_type`, `gzip_static`, `gzip_vary`), Wave 2
  (redirect policies, limit status codes, `max_ranges`, `chainFile`, typed `[[http.maps]]`) and
  Wave 2b (proxy response-header policies, `internal`, `merge_slashes`, header acceptance policy)
  are implemented with bounded fail-closed subsets and differential coverage; the remaining
  behavior-gap list (chunked_transfer_encoding, charset family, autoindex family, gzip
  refinements, HTTP/2 tuning, stream module gaps) is tracked in the audit ledger as Wave 3.
- Directive audit: `tools/gen-nginx-directive-audit.mjs` generates the 744-row decision ledger
  from the catalog (deterministic `--check` plus offline `--verify-evidence`) and is executed by
  the aggregate `check:nginx-compat` release gate.
