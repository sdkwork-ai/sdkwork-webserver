# Tech: Distributed Cluster Management (Cluster Plane)

- Status: implemented (self-cluster plane v1; join modes + API-only sync plane v2)
- Owner: web-platform
- Related: `SDKWORK_WEBSERVER_SPEC.md` §17.4 (standalone-only), PRD §5/§8.2 (node-scoped
  distribution and reliability), `TECH-runtime-data-plane.md`

## 1. Scope and terminology

The **cluster plane** models the sdkwork-webserver deployment itself:

| Concept | Table | Meaning |
| --- | --- | --- |
| Cluster | `webserver_cluster` | A deployment grouping; per-cluster heartbeat interval and offline threshold. A `default` cluster is auto-provisioned on first registration. |
| Host | `webserver_cluster_host` | One machine (one row per machine code): system basics (OS, kernel, arch, CPU, memory), **remote IP** (control-plane observed / reported), **local IPs**, **machine code**, **MAC addresses**. |
| Instance | `webserver_cluster_instance` | One webserver **process** (one row per `(host, pid)`): role, environment, PID, process start, bind address/port, advertised public endpoint, build version, status, health state, uptime, latest metrics. |
| Event | `webserver_cluster_event` | Append-only lifecycle evidence (registered / online / offline / degraded / maintenance / removed). |
| Heartbeat | `webserver_cluster_heartbeat` | Bounded-retention liveness + metrics samples. |
| Peer message | `webserver_cluster_peer_message` | Instance-to-instance mailbox delivered through the control plane (direct or broadcast, at-most-once handoff). |

Cluster data is **platform infrastructure**; the admin surface answers only the
platform operator tenant (PRD-FR-030). The operator tenant is resolved through
`web_platform_operator_tenant_id()` in `sdkwork-webserver-core`, which defaults to the
IAM bootstrap tenant (`100001`) and can be overridden with
`SDKWORK_WEBSERVER_PLATFORM_OPERATOR_TENANT_ID`. One host runs many instances; hosts join
clusters.

## 2. Communication model

- **In-process self-report** (primary): every standalone gateway process registers its own
  host + process and heartbeats on a timer (`cluster_self_report.rs` in the assembly; owner
  = the gateway process). Hosts that share the central PostgreSQL therefore form one cluster
  view across machines with no extra agent — multi-instance on one host is just more gateway
  processes with unique `SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID`s.
- **Machine-only internal surface** (remote / no-shared-DB path):
  - `POST /internal/v3/api/web/cluster/instances/register` — authenticated with the shared
    registration credential `wreg_…` (env `SDKWORK_WEBSERVER_CLUSTER_REGISTRATION_TOKEN`,
    constant-time compare). Returns the instance's own heartbeat token `winst_…` + peer
    directory. Idempotent per `(machine code, pid)`; latest registration wins.
  - `POST /internal/v3/api/web/cluster/instances/heartbeat` — liveness + health + metrics;
    response carries the refreshed peer directory (≤ 200 peers) and pending mailbox messages
    (at-most-once).
  - `GET  /internal/v3/api/web/cluster/peers` — peer directory.
  Both routes live on the machine-only framework layer; IAM user keys can never reach them.
- **Peer messaging**: instances communicate through the mailbox table; broadcast is
  `to_instance_id IS NULL`. Delivery happens on heartbeat with `FOR UPDATE SKIP LOCKED`.

## 3. Liveness

Heartbeats refresh the instance row and its host. The gateway-owned sweep (same timer as the
self-report loop) marks instances offline after the **cluster's own** offline threshold
(`last_heartbeat_at < now - offline_threshold_seconds` per cluster, joined in SQL), expires
hosts without reachable instances, expires peer messages past `expires_at`, and purges
heartbeat samples beyond `SDKWORK_WEBSERVER_CLUSTER_HEARTBEAT_RETENTION_HOURS` (default 72h).
All sweep operations are bounded batches (PAGINATION_SPEC §2.5).

## 4. Admin surface (backend-admin, permission `web.cluster.read|write`)

- `GET|POST /backend/v3/api/clusters`, `GET|PATCH|DELETE /backend/v3/api/clusters/{clusterId}`
  (delete refuses non-empty clusters).
- `GET /backend/v3/api/clusters/hosts` (cursor), `GET|PATCH|DELETE /clusters/hosts/{hostId}`
  (delete refuses hosts with instances; PATCH renames / reassigns cluster).
- `GET /backend/v3/api/clusters/instances` (cursor; filters cluster/host/status/health),
  `GET|PATCH|DELETE /clusters/instances/{instanceId}` (PATCH: name, status incl. maintenance,
  public endpoint).
- `GET /backend/v3/api/clusters/events` (cursor; filters cluster/severity).
- `GET /backend/v3/api/clusters/overview` — aggregate counts for the polling dashboard.

The PC console (`@sdkwork/webserver-pc-admin-cluster`) adds the **集群管理 / Cluster** tab
group, whose entries all live under `/admin/cluster/<child>` — `overview` (集群总览 / Cluster
Overview), `clusters`, `hosts`, `instances`, `events`. The tab's own landing path is
`/admin/cluster/overview`; no entry may claim the bare `/admin/cluster` prefix, which is only a
navigation boundary (an entry owning it would give the operator two URL spellings for the same
page). The sidebar labels of `overview` and `clusters` are deliberately distinct (集群总览 vs
集群) — a shared word renders as two identically named menu items. The overview auto-refreshes
(10s polling); clusters / hosts / instances / events pages run over the shared registry engine.
Status wire enums: instance `0=offline,1=online,2=starting,3=stopping,4=error,5=maintenance`;
host `0=offline,1=online,2=deploying,3=error,4=maintenance`.

## 5. Configuration (env)

`SDKWORK_WEBSERVER_CLUSTER_SELF_REPORT_ENABLED` (default on), `SDKWORK_WEBSERVER_CLUSTER_REGISTRATION_TOKEN`,
`SDKWORK_WEBSERVER_CLUSTER_CODE`, `SDKWORK_WEBSERVER_CLUSTER_ROLE`, `SDKWORK_WEBSERVER_CLUSTER_HEARTBEAT_INTERVAL_SECS`,
`SDKWORK_WEBSERVER_CLUSTER_HEARTBEAT_RETENTION_HOURS`, host identity overrides
`SDKWORK_WEBSERVER_HOST_{NAME,MACHINE_CODE,LOCAL_IPS,MAC_ADDRESSES,OS_VERSION,KERNEL_VERSION}`,
`SDKWORK_WEBSERVER_INSTANCE_{NAME,PUBLIC_ENDPOINT}`. See `etc/topology/standalone.*.env`.

## 6. Non-goals (v1)

- No push channel (WebSocket/SSE) for the admin plane; the overview polls.
- No leader election; the registry is the shared database itself.
- No peer-to-peer direct data channel; mailbox delivery is through the control plane.
- Full interface enumeration (all IPs/MACs) on Windows degrades to env overrides; Linux
  reads `/sys/class/net`.

## 7. Join modes and the API-only sync plane (v2)

Every host and instance carries a **join mode** (`LAN` = 同网段, direct API/shared database;
`TUNNEL` = FRP-style reverse tunnel through the public gateway) persisted in
`webserver_cluster_host.join_mode` / `webserver_cluster_instance.join_mode`
(migration `0014_webserver_cluster_join_modes_and_sync`). Registration validates the
pairing: `TUNNEL` hosts must advertise a tunnel route domain, `LAN` hosts must not carry
one. The admin surface filters and manages instances by mode and by sync state.

**TUNNEL-mode nodes never touch the database.** They join, heartbeat, and synchronize
exclusively through the machine-only internal API, transported as HTTP through the
reverse tunnel's relay (route domain → the admin API listener). The node-side
membership client (`cluster_member.rs` in the agent daemon) speaks both transports:
direct HTTP for `LAN`, tunnel-relayed HTTP for `TUNNEL`.

### Data synchronization plane

- `webserver_cluster_sync_revision` stores desired-state revisions per
  `(cluster, kind)` with `kind ∈ {0=config, 1=applications}`, a snowflake revision
  label, the canonical payload SHA-256, and the JSON payload.
- Admin action `POST /backend/v3/api/clusters/{clusterId}/sync` publishes a revision
  and flips every instance of the cluster to `PENDING`.
- Heartbeat responses carry the per-track desired-vs-applied
  `sync` states; a drifting node fetches
  `GET /internal/v3/api/web/cluster/sync/manifest?kind=…`, applies it through its
  `SyncApplier`, and acknowledges via
  `POST /internal/v3/api/web/cluster/sync/ack` (`IN_SYNC`/`FAILED`).
- The instance row tracks `applied_*_revision` per kind plus an aggregate
  `sync_status` (`unknown / in_sync / pending / failed`) surfaced in the admin API
  with `joinMode` / `syncStatus` list filters.

### Node service quality

Heartbeats may embed a `quality` sample (`cpuPercent`, `memoryPercent`,
`openConnections`, `rttMillis`, `errorRatePercent`). The registry derives a 0..=100
`quality_score` per instance (saturation and latency penalties, `cluster_quality_score`)
and exposes it on the admin surface for availability monitoring; the liveness sweep
marks silent members offline and instances re-joining after recovery are re-registered
idempotently by `(machine code, pid)`.

## 8. Auto-discovery and auto-routing (cluster routing runtime)

New crate `sdkwork-webserver-cluster`: the discovery/routing runtime, kept
dependency-light (arc-swap + tokio) and pure-sync on the pick path.

- **Auto discovery**: a `TopologyRefresher` pulls the live inventory from a
  `DiscoverySource` (database-backed in the management assembly; trait-injected
  in the data plane) on a bounded interval and publishes an immutable
  `ClusterTopology` snapshot through `ArcSwap`. Readers never block and a
  failed refresh keeps the previous snapshot (stale-but-usable beats empty).
- **Auto routing**: the data-plane request path consults the snapshot before
  virtual-host routing; a request `Host` matching a `served_domains` entry is
  routed to a picked healthy instance over the internal network
  (`relay_cluster_http`, HTTP/1.1 with WebSocket upgrade pumping).
- **Load balancing strategies** (`LoadBalancingStrategy`, per cluster in
  `webserver_cluster.lb_strategy`, overridable per request with the
  `x-cluster-lb-strategy` header):
  - `round_robin` — DEFAULT (industry's most common default; weight-aware)
  - `weighted_round_robin` — nginx smooth-WRR semantics (no burst clustering)
  - `least_connections` — fewest in-flight routed requests, quality tiebreak
  - `random` — lock-free xorshift
  - `random_two_choices` — power-of-two-choices (near-least-conn without
    global state; best at high concurrency)
  - `ip_hash` — client-IP sticky (nginx `ip_hash`)
  - `consistent_hash` — ketama-style ring (160 vnodes/instance, FNV-1a +
    splitmix64 finalizer), minimal remapping on membership changes
- **Health-awareness**: picking considers only online + not-unhealthy
  instances; `InstanceLease` tracks in-flight routed requests (released on
  drop) and sticky strategies fall back to any healthy instance when their
  owner is unavailable (availability over affinity).
- **High concurrency**: the pick path is atomics-only (`ArcSwap` snapshot
  read, `AtomicU64` cursors/rng, `AtomicI32` smooth-WRR currents,
  `AtomicU32` in-flight) — no mutexes, no allocation; verified by
  multi-threaded regression tests.

## 9. Per-instance operations (admin, commercial-grade instance management)

Instance rows carry a full operations surface (`webserver_cluster_instance`
migration `0016_webserver_cluster_instance_ops`):

| Capability | Wire / SQL | Semantics |
| --- | --- | --- |
| Cordon / uncordon | `routing_enabled`, `POST .../instances/{id}/cordon\|uncordon` | Remove from routing while serving (K8s cordon) |
| Graceful drain | `draining` + `drain_started_at`, `POST .../instances/{id}/drain\|undrain` | Exclude from routing; node finishes in-flight work then stops; events `INSTANCE_DRAINING`/`INSTANCE_UNDRAINED` |
| Labels | `labels` JSONB, `PATCH .../instances/{id}` | Free-form operator metadata |
| Restart tracking | `restart_count` / `last_restarted_at` | Auto-incremented when registration observes a `process_started_at` change (auto-recovery evidence, flapping detection input) |
| Active probes + auto-eject/recover | `probe_failures` / `ejected_at` / `probe_url`, `ClusterProbeWrite` port | 3 consecutive probe failures auto-eject (status error, `INSTANCE_AUTO_EJECTED` event); any success auto-recovers (`INSTANCE_AUTO_RECOVERED`) |
| Ops directives | heartbeat response `ops { routingEnabled, drainRequested }` | Registry-driven commands the node obeys on every heartbeat |

All admin actions are permission-gated (`web.cluster.write`) and emit
lifecycle events; the routing overlay (`sdkwork-webserver-cluster`) honors
`routing_enabled`/drain/eject state so cordoned, draining, and ejected
instances stop receiving new traffic immediately.

### Instance list search and filters (list surface)

`GET /backend/v3/api/clusters/instances` accepts, beyond cluster/host/
status/health/joinMode/syncStatus filters: a label selector
(`labels=k1=v1,k2=v2`, JSONB containment — every pair must match), free-text
`search` over name / public endpoint / hostname (case-insensitive
substring), and exact `buildVersion` (version-skew management: find every
instance not yet running the target build).

### Instance detail and flexible configuration (detail surface)

`GET /backend/v3/api/clusters/instances/{instanceId}` returns the complete
detail projection across six information planes: identity/ownership,
lifecycle (status/health/uptime/restarts/version), operations state
(routingEnabled/draining/ejected/probeUrl/probeFailures), sync plane
(desired vs applied per track + aggregate status), quality (score + latest
metrics sample), and networking (bind/public endpoints, remote IP, join
mode, tunnel domain, labels).

Flexible per-instance configuration (PATCH):
- `routingWeight` (1..=10000) — per-instance LB weight override feeding the
  routing topology's weighted strategies;
- `probeUrl` — active-probe target override;
- `routingEnabled` / `draining` — cordon/drain switches;
- `maintenanceNote` — operator context rendered on the detail surface;
- `labels` — free-form organization metadata.

Detail-page endpoints:
- `GET .../instances/{id}/metrics/history?limit=` — bounded heartbeat metric
  samples, newest first (trend charts, SLA evidence);
- `GET /backend/v3/api/clusters/events?instanceId=` — recent lifecycle events
  for one instance (drain/eject/recover/registration audit trail);
- `POST .../instances/{id}/probe` — on-demand connectivity probe ("Test
  connection"): raw HTTP GET with bounded timeout against `probeUrl` /
  bind endpoint / public endpoint, records the outcome through the
  auto-eject / auto-recover port, and returns
  `{ healthy, latencyMs, failures, ejected, recovered }`;
- `POST /internal/v3/api/web/cluster/instances/drain_complete` — node-side
  drain completion acknowledgment (closes the drain loop: flags cleared,
  instance marked stopped, `INSTANCE_DRAIN_COMPLETED` event).

## 10. Cloud-account DNS association (complete unattended renewal)

Renewal previously ran on the HTTP-01 path, so wildcard certificates could
never renew and DNS-provider credentials were unused after first issuance.
The engine now resolves cloud accounts per identifier:

- `DnsCloudAccountRegistry` (`dns_account.rs`) associates one cloud DNS
  account (Aliyun / DNSPod / Cloudflare credentials + hosted zone) per zone
  and resolves any certificate identifier by **longest zone-suffix match**
  (`api.dev.example.com` → a `dev.example.com` account before the apex
  account; wildcards normalize to their base zone).
- `AcmeChallengeMode::Dns01` carries a `DnsZoneResolver`, so a certificate
  spanning multiple zones/accounts validates every authorization through its
  own account; a missing association fails the authorization with an
  explicit error (`no cloud account is associated with identifier …`).
- `CertificateIssuer::dns01_context(hostnames)` selects DNS-01 for renewal
  when the registry covers every identifier; the zero-account deployment
  keeps the historical HTTP-01 path (non-wildcards) and the operator manual
  presenter (wildcards fail fast before an order is created).
- `DnsCloudAccountRegistry::from_configs` builds the registry from
  `DnsCloudAccountConfig` rows (`accountId`, `provider`, `zoneApex`,
  `credentials`) — fail-closed on malformed credentials: an account that
  cannot construct its presenter aborts the registry build instead of
  silently stranding its certificates at renewal.
