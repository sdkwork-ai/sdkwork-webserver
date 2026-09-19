# REQ-2026-0042 Location-Level Overrides, Proxy Error Interception, And Recursive Error Pages

```yaml
id: REQ-2026-0042
title: Complete nginx error-page and compression context granularity
owner: sdkwork-webserver
status: accepted
source: full-nginx-surface-regression-alignment-round-3
problem: Round two (REQ-2026-0041) left the last cataloged context gaps. Location-level `gzip` and `error_page` failed closed, `proxy_intercept_errors` was accepted-and-ignored so proxied upstream errors could never render a mapped page, `recursive_error_pages` was ignored so an error inside an error page surfaced as the raw engine error, and server-level `sub_filter` failed closed.
goals:
  - Materialize location-level `gzip` / `gzip_types` / `gzip_min_length` into a per-route compression policy with nginx inheritance (the on/off switch inherits, types/min length replace when declared, routes without directives inherit the host policy), and let the compression layer resolve route-first, then host, then app.
  - Materialize location-level `error_page` mappings that replace the host-level set for that route, and server-level `sub_filter` entries inherited by locations that declare none (nginx replace-on-declare).
  - Materialize `proxy_intercept_errors` across http/server/location with nginx inheritance; when enabled, a proxied response whose status has an `error_page` mapping is replaced by the mapped page (the original status stands unless `=code` overrides it).
  - Materialize `recursive_error_pages` at http/server level; when enabled, an error produced while serving an error page re-enters the mapping with the target route's own error_page set, bounded to three hops; when disabled the target's own error stands (nginx default).
  - Fix a sub_filter middleware defect the new coverage exposed: a response whose body was buffered but not modified was returned with an emptied body.
non_goals:
  - `recursive_error_pages` unbounded chains (three-hop bound is an SDKWork hardening difference, cataloged).
  - Error-page targets that resolve to proxied routes (v1 keeps static-only targets; a proxy target aborts mapping and the original error stands).
  - Location-level `recursive_error_pages` (nginx declares it in http/server only).
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - A location with `gzip off` serves uncompressed while sibling routes on the same gzip-enabled host compress.
  - A location `error_page 404 /alt.html` serves alt.html for that route's 404s while the host-level 404 page still serves other routes.
  - A server-level `sub_filter` applies to locations without their own sub_filter and does not leak into locations that declare one.
  - `proxy_intercept_errors on` replaces a mapped upstream 503 with the error page (status preserved); without the flag the upstream error passes through verbatim.
  - A 404 whose error-page target is itself missing resolves through the host-level 404 mapping when `recursive_error_pages on`; with the flag off the raw target error stands.
  - Invalid combinations fail closed with source-mapped diagnostics; the single-page regression covers every behavior and passes.
non_functional_requirements:
  security: Recursive chains are bounded (three hops); each hop re-runs full route selection and static path sanitization; interception cannot serve a target outside the configured error-page URIs.
  privacy: No new metric label is introduced.
  performance: Route-level policies ride the existing per-request extension path; no extra locking or allocation on the hot path beyond the captured request identity.
  reliability: The sub_filter no-op path restores the buffered body instead of emptying it (regression covered by the gzip scoping case serving large static bodies).
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
  - node --test tools/check-nginx-behavioral-corpus.mjs
  - pnpm check:nginx-gap
  - cargo fmt --all -- --check
```

## Alignment findings closed by this requirement

- Location-level `gzip` and `error_page` materialize into `RouteConfig.compression` / `RouteConfig.errorPages` with nginx inheritance; the compression layer resolves route → host → app.
- `proxy_intercept_errors` materializes across http/server/location (`ResourceConfig::Proxy::proxy_intercept_errors`) and the proxy dispatch replaces mapped upstream errors with the error page.
- `recursive_error_pages` materializes into `VirtualHostConfig.recursiveErrorPages`; the resolver chains through per-route mapping sets with a three-hop bound.
- Server-level `sub_filter` inherits into locations that declare none (shared helper with the location parser).
- Fixed: the sub_filter middleware returned an emptied body when buffering produced no substitution (body restore on the no-op path).
