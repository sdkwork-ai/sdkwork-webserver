# Service Level Objectives and Operational Response

This document defines the **targets** the standalone edge is operated against and the evidence that
is (and is not) available today. It does not invent production telemetry: the measurements in
[PRODUCTION-READINESS.md](PRODUCTION-READINESS.md) are loopback baselines, and a target becomes an
SLO only once the target host reports it.

## 1. Service and interfaces

| Interface | Surface |
| --- | --- |
| Data plane | Public HTTP/HTTPS listeners compiled from `sdkwork.webserver.config.json` |
| Management | Loopback-only `/healthz`, `/readyz`, `/livez`, `/metrics` (management listener) |
| Control plane | Same-origin backend API under `/backend/v3/api` |

## 2. SLIs and targets

| SLI | Definition | Target | Error budget (30 days) |
| --- | --- | --- | --- |
| Availability | Non-5xx responses / all data-plane requests, excluding client 4xx | **99.9%** | 43 min 12 s |
| Edge latency | p99 of requests answered by the local cache/static path | **≤ 100 ms** at the edge | — |
| Upstream latency | p99 of proxied requests, measured at the edge | **≤ 1 s** | — |
| Config reload | Successful reloads / attempted reloads | **99.99%** | 4 min 19 s |
| Certificate renewal | Certificates renewed before `not_after - renew_before` / due renewals | **100%** (hard gate) | none |

The availability/error-budget policy is burn-rate based: page at 2% budget consumed in 1 hour, ticket
at 5% in 6 hours, and review at 10% in 3 days. A certificate that misses its renewal window is an
incident regardless of budget.

## 3. Current evidence

- Baseline throughput/latency: `pnpm test:capacity`, including soak mode
  (`SDKWORK_CAPACITY_DURATION_SECONDS`). Latest loopback run: 32,000 requests, 0 failures,
  34,284 req/s, p99 3.56 ms; a 10-minute sustained run is recorded in the audit report.
- Idle connections: `pnpm test:connections` held 15,000 idle sockets while a fresh request returned
  200; the workstation ceiling (16,101) was kernel buffer/port exhaustion, not data-plane refusal.
- Compile budget: `pnpm test:route-compile` compiled 10,000 routes in 349–438 ms.
- Health/readiness/metrics: the management listener serves `/healthz`, `/readyz`, `/livez`, and
  `/metrics`; a non-loopback bind is refused unless `SDKWORK_WEBSERVER_MANAGEMENT_EXPOSE_ALLOWED=true`.

These are **regression baselines**, not production SLO attainment. The first production month of
`/metrics` data is what establishes the SLO.

## 4. Alert rules already shipped

`deployments/observability/prometheus-rules.yml` is the executable alert layer for this service, and
`deployments/observability/README.md` explains how to scrape the loopback operations listener. The
rules cover scrape-down, sustained resource pressure, upstream transport failures/timeouts, request
admission rejection, protocol-error bursts, certificate expiry, audit-persistence gaps, and WebSocket
drain timeouts. Every expression is grounded in a series the exporters actually publish; node
convergence and rollout-state rules are deliberately absent until those series exist.

The availability/latency SLOs in §2 are not yet a Prometheus rule: they need the request-outcome
series exposed by the framework's HTTP metrics registry, which is a separate scrape target. Until
that target is wired into the same rule file, treat the shipped rules as the production alerting
surface and the SLO table as the operating target.

## 5. Support bundle and incident inputs

The module's standard entrypoints already provide the diagnostic surface; no ad-hoc collector is
required:

| Need | Command |
| --- | --- |
| Environment diagnosis (human) | `bin/doctor.sh` |
| Environment diagnosis (machine) | `bin/doctor.sh --json` / `--export` |
| Configuration dump (secrets redacted) | `bin/config.sh list/show/get/diff` |
| Logs (bounded, exportable) | `bin/docker-deploy.sh logs --tail/--since/--export` |
| Backup / restore verification | `bin/backup.sh list/verify/restore` |
| Release/package verification | `bin/apps-package.sh` + sidecar `.sha256` |

Attach the `doctor --export` archive, the bounded log export, the affected configuration diff, and
the release version to every incident. The archive must be produced with the same entrypoints; do
not hand-copy environment files.

## 6. Review

Review targets monthly against `/metrics` and adjust only with a recorded decision. Capacity
targets (100k idle connections, 24 h soak) remain launch gates tracked in the audit report, not SLO
claims.
