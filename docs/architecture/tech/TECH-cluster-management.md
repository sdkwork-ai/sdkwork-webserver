# Tech: Distributed Cluster Management (Cluster Plane)

- Status: implemented (self-cluster plane v1)
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

Cluster data is **platform infrastructure (tenant 0)**; the admin surface answers only the
platform operator tenant (PRD-FR-030). One host runs many instances; hosts join clusters.

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

The PC console (`@sdkwork/webserver-pc-admin-cluster`) adds the **集群 / Cluster** tab group:
auto-refreshing overview (10s polling), clusters / hosts / instances / events pages over the
shared registry engine. Status wire enums: instance `0=offline,1=online,2=starting,3=stopping,4=error,5=maintenance`;
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
