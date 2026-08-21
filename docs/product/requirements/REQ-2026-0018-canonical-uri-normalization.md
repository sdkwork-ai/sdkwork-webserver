# REQ-2026-0018 Canonical URI Normalization

```yaml
id: REQ-2026-0018
title: Separate raw request URI from bounded canonical routing Path
owner: SDKWork maintainers
status: in-progress
source: compatibility
problem: Route selection used the raw Path while Nginx location processing uses a decoded and normalized URI. Encoded separators, dot segments, and repeated slashes could select different routes or filesystem/upstream identities between engines.
goals:
  - Preserve raw Path and Query separately from one bounded canonical Path.
  - Match Nginx core percent decoding, slash merging, dot resolution, encoded slash, trailing slash, and above-root rejection.
  - Require authored route paths to be canonical before immutable index compilation.
  - Treat decoded reserved characters as canonical Path data without a second configuration-time decode.
  - Use canonical Path for route/static identity and stripPrefix proxy rewrites while preserving raw no-rewrite proxy URI.
  - Document intentional backslash and invalid UTF-8 security differences.
non_goals:
  - Regex/named/nested locations, rewrite directives, variables/captures, internal redirects, try_files, cache keys, or full proxy_pass directive parity.
  - Accepting Windows Nginx backslash normalization or invalid UTF-8.
acceptance_criteria:
  - Human review accepts the raw/canonical ADR and security differences.
  - Real Nginx 1.26.2 evidence records raw and normalized output.
  - Core normalization is bounded and rejects traversal above root.
  - Route indexes reject non-canonical authored paths.
  - H1 and TLS/H2 route selection use canonical Path.
  - Static mapping avoids double decoding and stripPrefix proxy rewrite uses canonical Path while preserving Query.
  - Full repository verification passes.
non_functional_requirements:
  security: One decoding phase, above-root/backslash/control/invalid-UTF8 rejection, no reflected unsafe input.
  privacy: Raw and canonical URI are request-local and never unbounded metric labels.
  performance: Allocations are bounded by maxDecodedPathBytes and maxPathSegments; no regex, task, or lock.
  compatibility: Every deliberate Nginx difference is visible in the matrix and ADR.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - ARCHITECTURE_DECISION_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - tests/nginx/uri-normalization/nginx.conf
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm verify
```

Architecture decision: [ADR-20260716-canonical-uri-dual-representation.md](../../architecture/decisions/ADR-20260716-canonical-uri-dual-representation.md), currently proposed and awaiting human review.

## Nginx 1.26.2 Comparison

The loopback fixture at `tests/nginx/uri-normalization/nginx.conf` returned `$uri|$request_uri` from the installed Windows Nginx 1.26.2 binary.

| Request target | Nginx canonical result | SDKWork draft classification |
| --- | --- | --- |
| `/a/../b?x=1` | `$uri=/b`, raw preserved | match |
| `/a/%2e%2e/b?x=1` | `$uri=/b`, raw preserved | match |
| `//a///b` | `/a/b` | match |
| `/a%2fb` | `/a/b` | match |
| `/a/%2E/b` | `/a/b` | match |
| `/a/.` | `/a/` | match |
| `/a/..` | `/` | match |
| `/../../b` | `400` | match |
| `/a%3fb` | `/a?b` | match; `?` is canonical Path data |
| `/a%23b` | `/a#b` | match; `#` is canonical Path data |
| `/a%25b` | `/a%b` | match; `%` is not decoded a second time |
| `/a/%5cb` | `/a/b` on Windows | intentional SDKWork `400` hardening difference |
| invalid UTF-8 | build/platform dependent | intentional SDKWork `400` hardening difference |

## Current Evidence

- Core normalizer, single-pass canonical route validation, decoded reserved-character matching, and finite byte/segment rejection are implemented with bounded allocations.
- Real raw-H1 route/static/rewrite-proxy tests, TLS/H2 canonical route tests, and URL re-encoding tests pass.
- Gateway evidence passes 30 unit tests, 37 data-plane integration tests, and 3 raw HTTP/1 connection tests. Core evidence passes 4 unit tests and 26 configuration integration tests.
- Full workspace tests, strict workspace Clippy, formatting, `pnpm verify`, configuration validation, pagination, response-envelope, SDK consumer-import, documentation, and diff checks pass.
- PostgreSQL lifecycle execution remains ignored without `SDKWORK_DATABASE_TEST_POSTGRES_URL`; the unrelated existing `agent.sync` operation-pattern violation also remains a repository-level commercial blocker.
- Acceptance remains pending human review of the proposed ADR and its compatibility/security differences.
