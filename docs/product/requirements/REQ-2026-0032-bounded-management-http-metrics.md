# REQ-2026-0032 Bounded Management HTTP Metrics

```yaml
id: REQ-2026-0032
title: Make management HTTP metrics real and bound metric series memory
owner: sdkwork-webserver
status: accepted
source: production-observability-correctness
problem: The management process mounted /metrics with a new default registry while app-api and backend-api WebFrameworkLayer instances had no reference to it, so business requests were not counted. The shared registry also retained labeled request and pipeline-stage HashMap entries without a series ceiling, allowing unknown textual paths or extension-supplied labels to grow process memory.
goals:
  - Share exactly one HTTP metrics registry across app-api, backend-api, and the management /metrics handler.
  - Enforce hard request-series, stage-series, label-key byte, and stage-label byte ceilings in sdkwork-web-framework.
  - Collapse every unresolved route to one fixed unmatched metric label rather than a redacted but still variable raw path.
  - Expose dropped-series counters instead of allocating after a ceiling is reached.
  - Emit validated SDKWork environment, deployment_profile, runtime_target, and runtime_profile dimensions.
non_goals:
  - Claiming data-plane connection, request, upstream, health, resource-pressure, reload, or tunnel metrics are implemented.
  - Adding HTTP latency histograms, OpenTelemetry export, dashboards, alert rules, or long-term metric storage.
  - Changing existing public metric names or the legacy operationId label without human compatibility review.
  - Making a public production metrics listener decision; deployment exposure remains host policy.
users:
  - platform operators
  - site reliability engineers
acceptance_criteria:
  - A request handled by an injected WebFrameworkLayer is visible through the /metrics registry used by service_router.
  - App-api and backend-api wrappers preserve existing entrypoints and add explicit shared-registry entrypoints.
  - Unknown paths cannot create one series per path value.
  - The registry stores no more than 4096 labeled request series and 128 pipeline-stage series.
  - A request series key above 2048 bytes and a stage label above 128 bytes are dropped without insertion.
  - New series beyond a ceiling increment sdkwork_http_metric_series_dropped_total by kind; observations for existing series remain countable.
  - Counter accumulation uses saturating arithmetic where a locked series value is updated.
  - Metrics dimensions reject unknown lifecycle, deployment, runtime-target, and database profile aliases before management startup.
non_functional_requirements:
  security: Metrics labels contain route templates or the fixed unmatched token, never raw paths, credentials, request ids, trace ids, tenant ids, or payloads.
  privacy: No new user, tenant, organization, IP, hostname, or free-form business value is exported.
  performance: Request recording performs one bounded key allocation and one bounded-map lock; scrape allocation and iteration are bounded by the fixed series ceilings.
  reliability: Metrics saturation never rejects or delays the business request and remains observable through dropped-series counters.
affected_surfaces:
  - management-runtime
  - observability
  - web-framework
trace:
  specs:
    - OBSERVABILITY_SPEC.md
    - WEB_FRAMEWORK_SPEC.md
    - WEB_BACKEND_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - ../sdkwork-web-framework/crates/sdkwork-web-core
    - crates/sdkwork-routes-webserver-app-api
    - crates/sdkwork-routes-webserver-backend-api
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-web-core metrics::tests
  - cargo test -p sdkwork-routes-webserver-app-api --test app_web_framework_routes
  - cargo test --workspace
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

The bounded series registry remains owned by `sdkwork-web-core`; the application does not fork or wrap framework metric internals. The default hard ceilings are compile-time framework safety limits. The public limit constructor may lower but cannot raise them, which keeps tests and stricter consumers configurable without allowing runtime configuration to remove the memory bound.

Known operations use route templates resolved from the finite route manifest. An unresolved route uses the exact label `unmatched`; path redaction remains appropriate for logs but is not a metric-cardinality boundary. Request totals continue increasing even when a labeled series is dropped.

The Web Server management bootstrap constructs one registry with normalized dimensions and injects the same `Arc` into app-api, backend-api, and `ServiceRouterConfig`. Existing wrapper functions remain source-compatible and retain their previous no-injected-registry behavior for other compositions.

## Compatibility Boundary

Existing metric names and existing labels are preserved in this slice. The current shared framework still emits the historical `operationId` label rather than the canonical `operation_id`; changing or dual-emitting that public Prometheus contract requires explicit compatibility review. Data-plane telemetry requires a separate runtime-owned registry and operations-listener requirement.

## Acceptance Evidence

Accepted on 2026-07-16 with the following evidence:

- Seven focused `sdkwork-web-core` metric tests pass. They prove the infrastructure-path exclusions, exact `unmatched` collapse, request/stage series ceilings, oversized-label rejection, dropped-series counters, existing-series continuity after saturation, and Prometheus label escaping.
- The app-api integration test injects one registry into `WebFrameworkLayer`, sends a protected request that returns `401`, and renders that same registry through the management `/metrics` service. The output contains exactly one request with the route template, operation id, and `401` status, proving that the endpoint no longer scrapes an unrelated empty registry.
- Management bootstrap tests prove canonical `development|test|staging|production`, `standalone|cloud`, `server|container`, and `postgresql` dimensions. Unsupported aliases such as `runtime_target=docker` fail startup instead of becoming arbitrary metric labels.
- `sdkwork-web-framework` focused metric tests and `cargo clippy -p sdkwork-web-core --all-targets -- -D warnings` pass. The framework's full workspace remains independently blocked by pre-existing generated-assembly/test baseline failures outside this requirement; no generated output was hand-edited to hide them.
- The Web Server's environment-independent `pnpm.cmd verify` checks passed: workspace tests, OpenAPI materialization, documentation, topology, database-framework, and cloud-gateway validation succeeded. PostgreSQL lifecycle was not executed in this requirement's acceptance run; this requirement changes no persistence behavior, and REQ-2026-0004/0049 own the database evidence.
- Full Web Server `cargo clippy --workspace --all-targets -- -D warnings` passes. Both changed repositories pass `cargo fmt --all -- --check` and `git diff --check`.
- SDKWork pagination, API operation pattern, API response envelope, app SDK consumer import, application layering, and Rust backend composition checks pass.
