# REQ-2026-0071 Conditional-Request And Cache-Policy Differential Alignment

```yaml
id: REQ-2026-0071
title: Align conditional requests, ranges and cache policy with real nginx
owner: sdkwork-webserver
status: accepted
source: nginx-conditional-cache-alignment-round (differential oracle: nginx 1.29.6)
problem: |
  The full-surface corpus asserted conditional-request and cache behaviour only against
  this repository's own expectations: no fixture ever compared the data plane with a real
  nginx binary, so every assertion inherited whatever the implementation believed. Reading
  the upstream modules (`ngx_http_not_modified_filter_module.c`,
  `ngx_http_range_header_filter_module.c`, `ngx_http_headers_filter_module.c`) showed the
  belief was wrong in four places — a missing `If-Match` precondition, an `If-Modified-Since`
  vote that had to precede `If-None-Match`, a 304 that must keep its validators, and an
  `If-Range` mismatch that must fall back to a full 200 instead of a 206 — plus two
  header-placement defects (`Accept-Ranges` on a 206, representation validators on a 416) and
  one location-resolution defect where an inherited `root` shadowed an explicit `alias`,
  which both 404s working configurations and can expose documents an alias deliberately
  points outside the document root. Separately, none of the new unit tests had ever been
  compiled: `cargo check` does not compile `#[cfg(test)]` code, so the accompanying compile
  errors were invisible and the "unit tests pass" claim was unfounded.
goals:
  - Resolve a static location through `alias` before `root`, so a location declaring `alias` never falls back to an inherited server/http `root` (nginx `ngx_http_static_handler` order).
  - Answer `If-Match` as a precondition with 412 on a miss (`*` matches any existing representation, concrete candidates compare by whole value).
  - Evaluate `If-Modified-Since` before `If-None-Match` in one vote, so an IMS that proves staleness serves the body even when INM would have voted 304.
  - Keep `ETag`, `Last-Modified` and `Cache-Control` on a 304 while clearing `Content-Type`, `Content-Length` and `Accept-Ranges`, matching the entity-header reset in `ngx_http_not_modified_header_filter`.
  - Gate `Range` on `If-Range`: a mismatching validator ignores the range and serves the whole representation with 200 (never 206); the entity-tag form compares the whole string byte for byte, the date form requires exact equality.
  - Emit a strong `ETag` (`"<mtime-hex>-<size-hex>"`) for static files, because `If-Range` entity-tag matching requires byte equality and a weak tag can never satisfy it.
  - Attach `Accept-Ranges: bytes` only to a full representation, never to a 206.
  - Answer 416 through the special-response shape: keep `Content-Range: bytes */<size>` and drop the representation validators.
  - Publish `Expires` for `expires modified <time>` even when the target time is already in the past, overriding only `Cache-Control` with `no-cache`, and normalise `@24h` to the next midnight rather than a further day ahead.
  - Prove the whole surface against a real nginx binary with a reusable differential battery, and make the resulting slice executable by a gate rather than living in scratch scripts.
non_goals:
  - Reproducing nginx's built-in error-page body (it embeds the nginx version banner); the documented allowance covers the body only, not the status or any cache header.
  - http-level `root` / `index`: still fails closed with a diagnostic; cataloged as a context gap.
  - `tls.ocsp-stapling`, `http.websocket.rfc8441` and `ops.reload.listener-handoff`, which remain the three cataloged missing capabilities.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - A 50-case differential battery over conditionals, ranges and cache policy matches real nginx on every compared field except the two documented allowances (the deployment freshness default and the error-page body).
  - The allowance list is auditable on its own (`compare.py --strict`) and cannot absorb a real regression, proven by mutation.
  - `alias` wins over an inherited `root` in both the location that declares it and its siblings, with regression tests.
  - `cargo test -p sdkwork-webserver-core --lib` and `cargo test -p sdkwork-api-webserver-standalone-gateway --lib` both compile and pass.
  - The full-surface behavioural corpus stays green at its declared case floor.
non_functional_requirements:
  security: An `alias` location must never serve from an inherited `root`, which would expose documents outside the directory the alias points at; the precondition chain must reject an unsatisfiable `If-Match` instead of serving the representation.
  privacy: No new metric label is introduced; the battery records only response metadata.
  performance: The conditional chain and the `If-Range` gate resolve from headers already parsed for the request, adding no extra file metadata call; a 304 still avoids opening the file body.
  reliability: The differential battery must prove which implementation answered on each port, because Windows localhost forwarding can silently route a connection into a WSL listener and make the comparison measure nginx against itself.
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
    - tests/nginx/differential
    - tools/run-nginx-differential.mjs
verification:
  - node tools/run-nginx-differential.mjs
  - node tools/run-nginx-behavioral-corpus.mjs
  - node --test tools/check-nginx-behavioral-corpus.mjs
  - pnpm check:nginx-gap
  - cargo test -p sdkwork-webserver-core --lib
  - cargo test -p sdkwork-api-webserver-standalone-gateway --lib
```

## Alignment findings closed by this requirement

- **`alias` versus an inherited `root`** (`nginx/mapping.rs`): the resource branch chain tested `root.or_else(inherited_root)` before `alias`, so a location declaring `alias` under a server that declared `root` was silently rewritten to `root` + the full path. The branch now computes an `effective_root` that is `None` whenever the location declares `alias`. Three regression tests cover the inherited-server-root case, a sibling location that keeps the full path, and a `try_files`-only location that must still inherit the server root.
- **`If-Match` precondition** (`data_plane/static_file_response.rs`): a miss now answers 412 before any representation work; `*` matches an existing representation and concrete candidates compare by whole value. Verified against real nginx (`cond.if-match-miss` 412, `cond.if-match-star` 200).
- **`If-Modified-Since` before `If-None-Match`**: the two headers used to be evaluated as independent clauses. They now form one `if (ims || inm)` chain in which an IMS that proves staleness serves the body before INM is consulted, matching `ngx_http_not_modified_filter`. Covered by `if_modified_since_votes_before_if_none_match`.
- **304 validators**: the 304 path used to drop `ETag`/`Last-Modified`. It now retains them and clears `Content-Type`, `Content-Length` and `Accept-Ranges`, exactly as the filter does. Covered by `not_modified_keeps_the_cache_validators`.
- **`If-Range` gate**: `parse_range` was called unconditionally, so a stale validator still produced a 206. The range is now only honoured when the validator matches; otherwise the whole representation is served with 200. Covered by `if_range_ignores_the_range_when_the_validator_differs`.
- **Strong `ETag`** (`weak_etag` renamed to `entity_tag`): static responses now emit `"<mtime-hex>-<size-hex>"`. This was not cosmetic — a weak tag can never satisfy nginx's `If-Range` entity-tag comparison, so the range was previously ungated for every client that echoed the tag.
- **`Accept-Ranges` placement**: the header was added to the shared builder, so a 206 advertised byte-range support it must not advertise. It is now attached only to a full representation.
- **416 shape**: an unsatisfiable range answered with the representation's validators attached. It now goes through `range_not_satisfiable`, keeping `Content-Range: bytes */<size>` and dropping the validators, matching the special-response path.
- **`expires modified <time>`** (`data_plane/cache_policy.rs`): the stale branch must still publish `Expires` (the `ngx_http_time()` call precedes the negative test, which only rewrites `Cache-Control`), and `@24h` normalises through `mktime` to the next midnight rather than a further day ahead. Both are asserted by the probe and the pinned-clock unit test.
- **A test defect the compiler had never seen**: four `E0618` errors from a test helper shadowed by its own call sites and one `E0063` from a new struct field. `cargo check` does not compile `#[cfg(test)]` code, so the crate looked healthy while the tests could not build at all. Fixed alongside a rename of the helper.
- **The differential slice is now gated**: `tools/run-nginx-differential.mjs` pins the fixture mtime, refuses to run against a stale listener, starts the gateway on a port nginx does not own, captures the battery, compares against the recorded real-nginx baseline, and re-proves the oracle when a real nginx is available.
