# REQ-2026-0027 Adaptive Resource Pressure Admission

```yaml
id: REQ-2026-0027
title: Shed new business work before memory, handle, or event-loop exhaustion
owner: sdkwork-webserver
status: accepted
source: reliability
problem: Fixed connection and request counts bound cardinality but cannot detect that the process or container is already near its memory or open-handle ceiling, or that the asynchronous runtime cannot make timely progress. Continuing to accept and dispatch ordinary traffic until the OS, allocator, or cgroup terminates the process is not commercially safe.
goals:
  - Add an explicit standalone deployment resource-pressure policy with process memory, Linux cgroup v2 memory, Linux FD/Windows HANDLE, and event-loop-lag thresholds.
  - Reserve concrete memory and request capacity before hard limits rather than treating a percentage alone as an emergency reserve.
  - Use consecutive pressure and recovery samples plus separate admission/recovery thresholds to prevent rapid state flapping.
  - Refuse new data-plane connections before creating connection tasks while pressure is active.
  - Reject new business requests immediately with bounded 503/Retry-After behavior while preserving a finite request reserve for fixed infrastructure probes on already established connections.
  - Supervise the sampler with explicit startup, shutdown cancellation, and join ownership.
non_goals:
  - Claiming a hard allocator heap cap, guaranteed OOM immunity, or proof for the parent 100,000-connection/24-hour-soak gate.
  - Exact process CPU percentage, per-core scheduling, kernel PSI, cgroup v1, macOS task metrics, disk quotas, network bandwidth, per-tenant fairness, or distributed admission.
  - A new public management API, Prometheus exporter, cluster autoscaler, Kubernetes eviction integration, or control-plane resource coordinator.
  - Allowing arbitrary application routes to consume the operations reserve; only exact fixed-response GET/HEAD infrastructure paths qualify.
users:
  - platform operators
  - site reliability engineers
  - container and private-appliance operators
acceptance_criteria:
  - deployment.resourcePressure exposes finite sample interval, process-memory ceiling/reserve, open-handle ceiling/reserve, admission/recovery percentages, event-loop-lag thresholds, consecutive sample counts, operations request reserve, and fail-open/fail-closed sample policy.
  - Semantic validation requires each reserve to be below its hard ceiling, recovery thresholds below admission thresholds, event-loop recovery below admission, and operationsReserveRequests below maxConcurrentRequests.
  - Windows sampling uses the current process working set and HANDLE count; Linux sampling uses bounded /proc reads for current RSS and FD count and includes cgroup v2 memory.current/memory.max when a finite controller limit exists.
  - Sampling performs no unbounded collection, never overlaps, creates no per-sample detached task, and does not hold runtime or request-path locks across filesystem or OS calls.
  - Effective memory pressure begins at the earlier of the configured admission percentage or hard ceiling minus memoryReserveBytes; finite cgroup v2 usage is evaluated independently with the same reserve.
  - Effective handle pressure begins at the earlier of the configured admission percentage or effective handle ceiling minus openHandleReserve; Linux uses the lower configured/host open-file ceiling when available.
  - One initial sample completes before listener tasks begin. A fail-closed initial sampling error prevents activation; fail-open logs one bounded error class and starts without falsely reporting a sample.
  - ConsecutivePressureSamples transitions healthy to pressured only after consecutive breaches; consecutiveRecoverySamples plus every recovery threshold transitions back to healthy.
  - While pressured, accepted sockets are closed before connection admission, TLS, HTTP parsing, or connection-task creation, and the accept loop remains responsive to shutdown and future recovery.
  - Business requests arriving on established connections are rejected after bounded URI/route classification but before Body polling, static I/O, DNS, upstream TLS, or upstream work with fixed 503 and Retry-After; HTTP/1 closes and HTTP/2 remains stream-scoped.
  - When resourcePressure is configured, maxConcurrentRequests is split into business capacity and operationsReserveRequests, so ordinary traffic cannot consume the reserve and total admitted requests never exceeds maxConcurrentRequests.
  - Reserved requests qualify only for GET/HEAD exact /healthz, /readyz, or /livez routes backed by a fixed Respond resource; a proxy, static, redirect, prefix, mutation, or other route cannot use the reserve.
  - Resource sampling and gates are process-lifetime, Restart-only policy. Watch candidates changing resourcePressure are rejected as restart-required and cannot partially replace the active governor.
  - Real tests prove pressure rejection, operations-reserve availability on an established connection, no business action execution, hysteretic recovery, pre-task connection rejection, HTTP/2 stream isolation where applicable, and deterministic shutdown.
non_functional_requirements:
  security: Pressure responses and logs expose only bounded reason classes and never reveal byte counts, host limits, paths, process ids, tenant data, or request identity.
  privacy: The governor stores aggregate process counters only and no request content, address, route value, tenant, or user identity.
  performance: Healthy request admission adds one atomic pressure read plus one non-queuing semaphore acquisition; one bounded sampler task performs one sample at a time.
  reliability: Pressure preserves a configured byte/handle margin, rejects without a waiter queue, uses hysteresis, and releases all request accounting on completion/error/cancellation/drop.
affected_surfaces:
  - config
  - deployment
  - runtime
  - reliability
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - ENVIRONMENT_SPEC.md
    - DEPLOYMENT_SPEC.md
    - PERFORMANCE_SPEC.md
    - OBSERVABILITY_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

This policy belongs to `deployment.resourcePressure`, not a route/upstream policy. It governs one standalone process and is Restart-only. Cloud host policy must ultimately constrain or replace application-supplied values, but this focused runtime consumes one already validated effective configuration and does not claim multi-tenant host-policy composition.

Memory admission compares current process RSS with `maximumProcessMemoryBytes` and, on Linux cgroup v2, independently compares `memory.current` with a finite `memory.max`. The admission boundary is the smaller of the percentage threshold and ceiling minus the configured byte reserve. Open handles use the same model; Linux additionally lowers the effective configured ceiling to the host soft/hard open-file limit when it can be read safely. Event-loop lag is measured as the delay beyond the sampler's scheduled wake deadline, which is a cross-platform progress signal rather than a claim of exact CPU utilization.

The governor uses separate healthy-to-pressure and pressure-to-healthy thresholds plus consecutive sample counts. One atomic state is read by the accept and request paths. Sampling is serialized in one supervised task; OS/proc reads are bounded and performed through one awaited blocking operation at a time. Shutdown waits for the current bounded sample and then joins the sampler, avoiding detached per-sample work.

When enabled, one total Semaphore retains the original `maxConcurrentRequests` ceiling and one business Semaphore is smaller by `operationsReserveRequests`. Every request first acquires total capacity without queuing. After bounded URI and route selection, only exact fixed infrastructure responses on `GET`/`HEAD /healthz`, `/readyz`, or `/livez` may bypass the business Semaphore; every other route must acquire business capacity and pass the pressure check. A pressure or business-capacity rejection releases its total permit synchronously before the fixed rejection response is returned. Ordinary traffic therefore cannot retain the reserve, operations may reuse otherwise idle total capacity, and the combined admitted count never exceeds the original ceiling. New sockets are closed under pressure before task creation; the reserve protects probes already using an established connection and does not claim a separate always-accepting administrative listener. A separately isolated management listener remains a parent commercial requirement.

Static validation proves that configured process-memory and open-handle reserves leave strictly separate effective admission/recovery thresholds. The initial OS sample repeats this check against a finite Linux cgroup v2 memory limit and the lower configured/host file limit. Every later successful sample revalidates those dynamic capacities; a host/cgroup contraction that removes the reserve or collapses hysteresis is treated as confirmed unsafe capacity and transitions through the fail-closed consecutive-sample path rather than the sampling-error fail-open path.

## Architecture Review

No new process, public API, persistence owner, SDK, or cross-repository protocol is introduced. The change narrows the existing standalone data-plane deployment policy and request/connection admission pipeline, so this requirement records the decision without a separate ADR. A cluster resource coordinator, public operations API, cgroup/PSI contract, or multi-tenant host-policy authority would require a separate requirement and ADR.

## Acceptance

Accepted on 2026-07-16 for the declared single-process Windows/Linux resource-pressure sampling, request partition, connection shedding, and supervised lifecycle boundary.

- The root Schema and Core model expose optional `deployment.resourcePressure` with finite sampling interval, process-memory ceiling/reserve, memory admission/recovery percentages, open-handle ceiling/reserve, handle admission/recovery percentages, event-loop wake-lag thresholds, consecutive pressure/recovery counts, operations request reserve, and explicit fail-open/fail-closed sampling policy. Unknown fields and every numeric underflow/overflow fail before activation.
- Semantic validation requires each reserve below its ceiling, percentage recovery below admission, effective post-reserve recovery strictly below admission, event-loop recovery below admission, and `operationsReserveRequests < maxConcurrentRequests`. Startup repeats effective reserve/hysteresis validation against the observed finite cgroup v2 limit and lower Linux host/configured file limit.
- Windows uses `K32GetProcessMemoryInfo` Working Set and `GetProcessHandleCount`. Linux uses bounded reads of `/proc/self/status`, `/proc/self/fd`, `/proc/self/limits`, `/proc/self/cgroup`, and finite cgroup v2 `memory.current`/`memory.max`. File reads, FD iteration, retained reason state, and sampling concurrency are bounded; exactly one awaited blocking OS sample exists at a time.
- One initial sample completes before listener tasks. Fail-closed sampling failure prevents activation, fail-open begins without fabricating a healthy sample, and every later successful sample revalidates effective host/cgroup capacity. Confirmed capacity contraction uses the consecutive fail-closed transition even when the sampling-error policy is fail-open.
- One total non-queuing request Semaphore retains `maxConcurrentRequests`; one smaller business Semaphore leaves `operationsReserveRequests`. Successfully classified business and operations requests retain their permits through response completion/error/cancellation/drop, while pressure or business-capacity rejection releases total capacity synchronously before returning the fixed response. Only bounded route classification can prove a `GET`/`HEAD` exact `/healthz`, `/readyz`, or `/livez` fixed `Respond`; prefix, proxy, mutation, missing, malformed, static, redirect, and other requests must obtain business capacity and cannot retain the reserve.
- Pressured accepted sockets close before connection admission, TLS, HTTP parsing, or task creation. Established business requests receive fixed `503 Service Unavailable` and `Retry-After: 1`; HTTP/1 requests connection close while HTTP/2 rejection remains Stream-scoped. Consecutive recovery samples below every recovery threshold restore normal admission.
- A real Windows HTTPS/HTTP2 test measures baseline process HANDLE count, opens real file handles past the configured threshold, and proves business `503`, exact fixed health `200`, proxy/prefix/POST/missing-route reserve denial, new TCP socket close, same-H2-connection recovery after handle release, restart-required pressure-policy Watch rejection, a later valid same-policy Watch publication, and deterministic sampler/listener shutdown.
- Resource pressure is Restart-only in `ReloadTopology`; candidate policy change cannot partially replace the controller, total gate, business gate, or sampler. One process-lifetime supervisor owns sampling cancellation and join, and no request-path lock is held across OS, filesystem, TLS, DNS, Body, or upstream I/O.
- Core verification passed 8 unit tests and 47 configuration contract tests. Gateway verification passed 52 unit tests, 55 data-plane integration tests, 4 raw HTTP/1 tests, 1 focused resource-pressure HTTPS/H2 test, and 4 active-health network tests.
- `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, checked-in example validation, pagination, API operation-pattern, API response-envelope, app SDK consumer-import, repository documentation, formatting, and diff checks passed.

Acceptance is intentionally narrower than commercial Web Server completion and does not claim a hard allocator cap or guaranteed OOM immunity. PostgreSQL lifecycle execution remains ignored because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is absent. Backend OpenAPI encoding corruption and the unreviewed public `agent.sync` to `agent.retrieve` operation rename still require human review. Process CPU percentage, PSI, cgroup v1, macOS resource sampling, disk/network pressure, per-tenant fairness, physical upstream connection limits, retry/hedging/replay budgets, advanced balancing, resource/health metrics and administration, cluster-global coordination, independent management ingress, 100,000-connection and 24-hour soak evidence, HA/failover/rolling upgrade, signed SBOM/provenance, and commercial operations remain release blockers.
