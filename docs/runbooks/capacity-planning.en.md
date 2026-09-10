# Capacity Planning Runbook (sdkwork-webserver)

Standards: PRD §8.1 (performance and memory targets), REQ-2026-0027/0033/0034
(resource pressure admission, runtime metrics). Symptoms: memory/CPU near
limits, rising rejection counters, upstream capacity saturation, connections
approaching the configured ceiling.

## 1. Observation entry points

```bash
# Data-plane runtime metrics (loopback, fixed cardinality; RED + capacity)
curl -s http://127.0.0.1:3901/metrics | grep -E "connection|request|upstream|pressure|reject"

# Management metrics
curl -s http://127.0.0.1:3800/metrics

# Read-only environment diagnostics (9 checks, exit 70 = FAIL)
bin/doctor.sh            # --json / --export for machines
```

Key series: `connections_active`, `requests_active` (admission permits),
`requests_rejected_total` (by class), `upstream_request_capacity` /
`upstream_physical_connections`, `resource_pressure` (Windows/Linux sampled,
hysteresis-gated admission), request/upstream latency histograms.

## 2. Capacity baselines (configuration -> budget)

| Dimension | Configuration key | Sizing rule |
| --- | --- | --- |
| Connections | `limits.maxConnections` | per-connection budget × count ≤ ~60% of process memory budget |
| Concurrent requests | `limits.maxConcurrentRequests` | non-queuing admission; keep `operationsReserveRequests` for management |
| Upstream physical connections | upstream `maxConnections` / target ceilings | upstream budget ≤ egress fd/memory budget |
| Database pool | `SDKWORK_DATABASE_MAX_CONNECTIONS` | size within the memory budget; classic start = 2×vCPU + storage throughput factor |
| Container limits | compose `deploy.resources` / systemd `MemoryMax` | backstop against allocator exhaustion; keep 20-30% headroom |
| Usage metering | `usageMetering.maxBuckets` | default 65536 buckets; lower for high-entropy hostname traffic |

With `deployment.resourcePressure` enabled the process begins bounded
rejection/shedding before OS exhaustion — the PRD §8.1 "emergency margin".
Recommended in production.

## 3. Scaling paths

- **Vertical**: raise connection/request budgets and scale container/systemd
  limits proportionally; confirm fd ceilings (`LimitNOFILE=65536` /
  compose `ulimits.nofile`).
- **Horizontal (cloud data plane)**: render additional Nodes under the
  node-identity model (deployments/kubernetes/README.md §Scaling Contract);
  scale the ingress layer, never clone a registered Node.
- **Horizontal (standalone management plane)**: the shipped channel is
  single-host; upgrade/scale via the maintenance-window contract in
  failed-rollout.md.

## 4. Load and soak testing (PRD §8.1 evidence gap)

No load/soak benchmark ships yet (PRD Phase 3 acceptance item). Until the
official baseline exists:

- after capacity-related configuration changes, run
  `scripts/webserver-release-smoke.mjs` as a functional smoke;
- record 24h `connections_active` / RSS curves to confirm no monotonic growth
  across reloads, certificate rotations, disconnected clients, and failed
  upstreams.
