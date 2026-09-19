# REQ-2026-0040 Full-Surface Nginx Behavioral Regression Corpus

```yaml
id: REQ-2026-0040
title: Add a single-page full-surface nginx behavioral regression slice
owner: sdkwork-webserver
status: accepted
source: full-nginx-surface-regression-alignment
problem: The indexed corpus compared only narrow protocol slices (URI normalization, pipeline depth, keep-alive, early response, connection age). No single regression page exercised the whole declared http-core-v1 runtime surface end to end against the serving data plane, so cross-family regressions (routing, proxying, limits, filters, TLS, stream) surfaced only through operator reports.
goals:
  - Provide one fixture (tests/nginx/full-surface/nginx.conf) and one self-contained probe (tests/nginx/full-surface/probe.py) that together cover every runtime-executable directive family declared in specs/nginx-gap.catalog.json.
  - Cover routing (exact/prefix/^~/regex/~* locations, rewrite last/break/redirect/permanent with captures, return, try_files, alias, root, index, wildcard server_name, default server), URI canonicalization through route selection, static file semantics (MIME, ETag/Last-Modified conditionals, ranges, HEAD, 404), proxying (URI replacement, variable proxy_pass, proxy_set_header inheritance/replacement, proxy_pass_request_headers off, hop-by-hop stripping, body limits, chunked bodies, 100-continue, cache, chunked relay, WebSocket), load balancing (smooth weighted round robin, least_conn, ip_hash, hash consistent, passive health + backup failover via nginx-default retries), response filters (gzip negotiation, sub_filter, limit_req, limit_conn, allow/deny, auth_basic, secure_link), HTTP/1 wire semantics (keep-alive, close, pipelining, HTTP/1.0), TLS SNI certificate selection with ALPN h2 SETTINGS exchange, and the stream TCP proxy.
  - Keep the probe self-contained: it owns the mock upstreams, renders absolute-path staging material, generates the TLS fixtures and htpasswd, spawns and stops the data plane, and reports one single-page result table with a non-zero exit code on any failure.
  - Close the nginx compatibility gaps the first alignment runs exposed.
non_goals:
  - Full nginx OSS behavioral parity beyond the declared http-core-v1 profile.
  - Differential comparison against a pinned stock nginx binary in CI (the existing REQ-linked slices keep that role).
  - Server/location-level `gzip` context granularity and server-level `return` (cataloged context gaps; the fixture uses http-level gzip and location-level return).
users:
  - web server application authors
  - node operators
  - release verification
acceptance_criteria:
  - `python tests/nginx/full-surface/probe.py` runs every case against the locally built data plane and exits non-zero when any case fails; the summary table is the single review page.
  - The fixture materializes with zero skipped files (`validate-nginx`).
  - nginx-default upstream retry semantics hold: `proxy_next_upstream error timeout` equivalent failover reaches the backup target when the primary transport-fails.
  - `proxy_set_header` follows nginx inheritance: a child level that declares any directive replaces the whole inherited array.
  - A rewrite that changes the URI is forwarded upstream in rewritten form; unrewritten requests keep the raw request path (REQ-2026-0018).
  - `If-None-Match` uses RFC 9110 weak comparison with correct polarity: an echoed (weak or bare) tag answers 304 and a different tag serves 200.
  - `try_files` SPA fallbacks accept nested internal-redirect targets (`/app/index.html`).
  - The slice is indexed in specs/nginx-behavioral-corpus.manifest.json and validated by tools/check-nginx-behavioral-corpus.mjs.
non_functional_requirements:
  security: The probe uses throwaway self-signed fixtures and a generated htpasswd credential committed nowhere; runtime artifacts stay untracked via tests/nginx/.gitignore; mock upstreams bind loopback only.
  privacy: No tenant, request, or trace metric label is introduced.
  performance: The full page completes in well under a minute on a developer workstation; mock upstreams and the data plane are started and stopped per run.
  reliability: All cases are deterministic; rate and concurrency limits use loose count bounds; streaming assertions check relay fidelity instead of arrival timing.
affected_surfaces:
  - request-data-plane
  - nginx-compat-configuration
trace:
  specs:
    - NGINX_SPEC.md
    - SDKWORK_WEBSERVER_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - tests/nginx/full-surface
verification:
  - python tests/nginx/full-surface/probe.py
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway --lib if_none_match
  - node --test tools/check-nginx-behavioral-corpus.mjs
  - pnpm check:nginx-gap
```

## Alignment findings closed by this requirement

- nginx-default upstream retries: nginx-materialized upstreams carried no retry policy, so transport failures answered 502 instead of failing over to the backup target. The materializer now emits the nginx-default equivalent of `proxy_next_upstream error timeout` with one attempt per declared server (`crates/sdkwork-webserver-core/src/nginx/mapping.rs`).
- `proxy_set_header` inheritance: the materializer merged parent and child entries per header name; nginx replaces the whole inherited array whenever the child level declares any directive (`mapping.rs`).
- Rewrite forwarding: `proxy_pass` without a URI part forwarded the raw request path even after a `rewrite last`/`break` changed the URI. The data plane now forwards the rewritten canonical path once a rewrite changed it, while unrewritten requests keep the raw request path per REQ-2026-0018 (`handler.rs`, `proxy.rs`).
- `If-None-Match`: the static responder answered 200 for an echoed tag and 304 for a different tag (inverted polarity), and weak comparison failed the asymmetric `W/` prefix case (`static_file_response.rs`).
- `try_files` SPA fallback validation rejected nested internal-redirect targets although the data plane resolves them safely under the chrooted root (`validate.rs`).
