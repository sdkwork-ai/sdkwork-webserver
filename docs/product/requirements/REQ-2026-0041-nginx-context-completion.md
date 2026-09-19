# REQ-2026-0041 Nginx Directive Context Completion And Error-Page Mapping

```yaml
id: REQ-2026-0041
title: Support nginx server-level gzip/rewrite/return/error_page contexts
owner: sdkwork-webserver
status: accepted
source: full-nginx-surface-regression-alignment-round-2
problem: The first full-surface alignment round (REQ-2026-0040) left two cataloged context gaps and one PRD-required behavior silently ignored. Server-level `gzip` failed closed, so gzip could only be enabled app-wide; server-level `return` failed closed; server-level `rewrite` failed closed; and `error_page` — required by the http-core-v1 response-behavior group — was accepted and ignored at every level, so a 404 could never serve a mapped page.
goals:
  - Materialize server-level `gzip` / `gzip_types` / `gzip_min_length` into a per-virtual-host compression policy with nginx inheritance: the on/off switch inherits from the http level, the type list and min length replace when declared, and a host without directives inherits the app-wide policy.
  - Route the per-host policy to the compression layer through a response extension so the existing gzip predicate resolves enabled/types/minLength per matched host.
  - Materialize server-level `rewrite` by prepending the server rules to every route of that host (nginx rewrite phase runs before location selection).
  - Materialize server-level `return` as one catch-all respond/redirect route (nginx: the rewrite phase answers every request before locations, so the host's locations become unreachable).
  - Materialize `error_page <codes…> [=[response]] uri;` at http and server level (http entries inherit, server entries append, the runtime resolves the last declaration per code). On a generated static error (including access-control 403) the data plane internally redirects to the target URI by re-running location selection, strips conditionals/ranges for the redirect, replaces the response status only when `=code` is declared, and applies one hop.
non_goals:
  - Location-level `error_page` (fails closed with a precise diagnostic; document as a context gap).
  - Location-level `gzip` granularity beyond the server context (a location cannot override its host's policy yet).
  - Server-level `sub_filter` (still fails closed; cataloged).
  - `recursive_error_pages`, proxied-response interception (`proxy_intercept_errors`), and error-page targets that resolve to non-static routes.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - A host with `gzip on` compresses eligible responses while a sibling host without gzip directives serves the same content uncompressed under the same app config.
  - A server-level `rewrite … last` rewrites every matching request before location selection and the rewritten URI is forwarded upstream or served statically.
  - A server-level `return` answers every request in the host; locations in that host are never selected.
  - `error_page 404 /page.html` serves the page body with the original 404 status; `error_page 416 =200 /page.html` serves the page with status 200; ACL 403 responses resolve the mapped page.
  - Invalid `error_page` grammar (missing code, missing target, out-of-range codes, multiple targets, multiple `=code`) fails closed with a source-mapped diagnostic.
  - The single-page full-surface regression extends to cover every accepted behavior and passes end to end.
non_functional_requirements:
  security: Error-page internal redirects run the same route selection and static path sanitization as ordinary requests; conditionals/ranges are stripped from the redirect request; variable `return` forms keep their existing fail-closed validation.
  privacy: No new metric label is introduced.
  performance: The per-host compression override is resolved once per request from the already-selected virtual host; the error-page saved request identity is captured only when the host declares error pages.
  reliability: Error-page redirects are bounded to one hop and fall back to the original error response when the target cannot serve.
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

- Server-level `gzip` / `gzip_types` / `gzip_min_length` now materialize into a per-virtual-host compression policy (`VirtualHostConfig.compression`) that the compression layer resolves per matched host; hosts without directives keep inheriting the app-wide policy (`mapping.rs`, `model.rs`, `gzip_predicate.rs`, `handler.rs`).
- Server-level `rewrite` rules now prepend to every route of their host, matching the nginx rewrite-phase order (`mapping.rs`).
- Server-level `return` now collapses the host to a single catch-all respond/redirect route, matching nginx's rewrite-phase short circuit (`mapping.rs`).
- `error_page` is no longer silently ignored: http/server mappings materialize into `VirtualHostConfig.errorPages` and generated static errors (including ACL 403) internally redirect to the mapped target with optional `=code` status replacement (`mapping.rs`, `model.rs`, `handler.rs`).
