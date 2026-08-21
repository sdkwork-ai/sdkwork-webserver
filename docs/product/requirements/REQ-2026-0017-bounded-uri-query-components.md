# REQ-2026-0017 Bounded URI And Query Components

```yaml
id: REQ-2026-0017
title: Bound and validate request URI components consistently across HTTP versions
owner: SDKWork maintainers
status: accepted
source: security
problem: HTTP/1 had an original-wire request-target ceiling and H2 had a decoded Header List ceiling, but every resource action lacked one cross-protocol Path/Query policy. H2 could therefore admit larger Path or parameter sets than H1, malformed percent escapes reached later adapters, and proxy Query composition had no explicit parameter/component budget.
goals:
  - Add finite raw Path, once-decoded Path, Path segment, Query string, Query parameter, and Query name/value component budgets.
  - Apply one allocation-free O(n) URI precheck before route selection and every resource action for HTTP/1 and HTTP/2.
  - Reject malformed percent escapes, decoded NUL/control bytes, and decoded backslash before routing, static mapping, or proxy composition.
  - Return bounded 414 for budget overflow and 400 for invalid representation; close HTTP/1 and isolate HTTP/2 to the affected Stream.
  - Preserve valid Query bytes through reverse proxying.
  - Support atomic Watch updates because policy is evaluated from the active immutable generation per request.
non_goals:
  - Canonical Nginx URI normalization, dot-segment resolution, merge_slashes, rewrite/capture processing, Unicode normalization, or proxy_pass original-versus-normalized URI selection.
  - Query semantic parsing, application parameter typing, form decoding, cookies, route Query predicates, or WAF rules.
users:
  - Platform operators
  - Site reliability engineers
  - HTTP application clients
acceptance_criteria:
  - Schema, Serde defaults, semantic validation, example configuration, and documentation expose all six budgets.
  - Query budgets are all disabled together or all positive; decoded Path cannot exceed the raw Path budget.
  - H1 and H2 enforce raw/decoded Path bytes, segment count, Query bytes, parameter count, and component bytes before action execution.
  - Malformed escapes and decoded NUL/control/backslash return 400.
  - Budget overflow returns 414; HTTP/1 closes and H2 remains reusable.
  - A valid bounded Query crosses the proxy unchanged.
  - Watch publishes valid budget changes atomically and later requests observe the new generation.
non_functional_requirements:
  security: Validation performs exactly one bounded percent-decoding scan and emits fixed text without reflecting URI or Query data.
  privacy: No URI, Query, parameter, decoded buffer, or client data is retained.
  performance: O(n) byte scanning with constant state, no allocation, collection, regex, task spawn, or lock.
  reliability: H1 original-wire total target limits remain an earlier defense; Handler policy supplies cross-version behavior.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
    - NGINX_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - pnpm verify
```

Product authority: [PRD-runtime-core.md](../prd/PRD-runtime-core.md) sections 5 and 7. Nginx compatibility authority: [PRD-nginx-compatibility.md](../prd/PRD-nginx-compatibility.md) section 8.

## Phase Contract

The Handler reads limits from the active generation after framing validation and before authority/route selection. The scanner never materializes decoded input. It validates each `%HH` once, counts decoded Path bytes and separators, checks Query pairs and their name/value slices, and rejects decoded control/NUL/backslash values.

Raw Path matching and proxy forwarding behavior remain unchanged in this requirement. Canonical Nginx URI normalization requires a separate ADR because it affects route indexes, static filesystem identity, rewrite ordering, cache keys, and upstream URI selection.

## Acceptance Evidence

The root Schema, Serde defaults, semantic validator, example configuration, and Handler expose finite raw Path, once-decoded Path, Path segment, Query string, Query parameter, and Query name/value component budgets. Query controls must be all zero or all positive; decoded Path cannot exceed the raw Path ceiling.

The scanner performs one allocation-free O(n) pass with constant state. Unit tests cover every budget class, malformed escapes, decoded NUL/control bytes, and decoded backslash. Real H1 tests prove bounded `400`/`414` with connection close. TLS/H2 tests prove Stream-scoped rejection and same-connection recovery. Proxy evidence proves a valid Query remains byte-preserved. A Watch test narrows Path budgets and proves later requests observe the atomically published generation.

Executed acceptance evidence:

- `cargo test -p sdkwork-webserver-core --test webserver_config`: 24 passed.
- `cargo test -p sdkwork-api-webserver-standalone-gateway`: 28 unit, 36 data-plane integration, and 3 raw HTTP/1 tests passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt -- --check`: passed.
- `cargo run -p sdkwork-api-webserver-standalone-gateway -- validate etc/examples/sdkwork.webserver.config.json`: passed.
- `pnpm verify`: passed, including full-workspace tests, database lifecycle, contract materialization, repository/docs/topology/database checks, and cloud gateway validation.

This acceptance does not claim canonical Nginx URI normalization, rewrite ordering, filesystem canonical identity, cache-key normalization, or `proxy_pass` original-versus-normalized URI parity. Those semantics require a separate ADR and differential corpus. PostgreSQL execution remained ignored because no disposable URL was configured. The pre-existing `GET /backend/v3/api/agent/sync` operation-pattern violation remains subject to human review.
