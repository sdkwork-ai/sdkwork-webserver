# Node Divergence Runbook (sdkwork-webserver)

Standards: PRD §8.2 (idempotent, resumable, checksummed, fenced node sync), §5
(cluster operation), REQ-2026-0052/0054. Symptoms: a Web Node's desired
configuration/certificates/TLS snapshot diverge from the control plane,
`web_certificate_node_state` observations lag, or a cloud data-plane node's
runtime-set generation falls behind.

## 1. Detect divergence

```bash
# Certificate distribution observations (per node)
psql "$DATABASE_URL" -c "SELECT server_uuid, certificate_version_id, observed_at
       FROM web_certificate_node_state ORDER BY observed_at ASC LIMIT 20;"

# Node heartbeat and convergence
psql "$DATABASE_URL" -c "SELECT uuid, last_heartbeat_at, metadata->>'syncGeneration' AS gen
       FROM web_server WHERE deleted_at IS NULL;"

# Local node side (Web Node Daemon / agent)
systemctl status sdkwork-webserver-node-daemon   # or sdkwork-webserver-agent (v3 compat)
ls /var/lib/sdkwork/webserver/sync/              # desired/observed checkpoints
head /var/lib/sdkwork/webserver/tls-materials/tls-runtime.json
```

## 2. Scenarios

### 2.1 Node sync generation behind (desired > observed)

Sync is idempotent and resumable: the node pulls checksummed `sv1:sha256`
generations and replays from durable checkpoints after a crash. Wait one sync
cycle first; if still behind check:

- internal API reachability (`SDKWORK_WEBSERVER_INTERNAL_API_BASE_URL`);
- node token file (`/run/secrets/*`) validity and the `wagent_` prefix
  (machine credentials never fall back to user keys);
- the exclusive process lock (REQ-2026-0055): ensure a second daemon is not
  running concurrently.

### 2.2 Certificate material divergence

- The control plane is authoritative (`web_certificate_version` +
  `web_listener_certificate_binding`); nodes only project it. Fix the binding;
  the worker re-projects and publishes a new monotonic `tls-runtime.json`.
- The node-local recovery store rejects scope/hash conflicts. A conflict in the
  log means the desired snapshot and the local slot disagree — do not delete
  recovery slots by hand; verify the tenant/environment dimensions first.

### 2.3 Node persistently offline

- Cloud (K8s): check StatefulSet/PVC/Secret and the provider-event exact routing
  (deployments/kubernetes/README.md steps 5-7). Node identity is fixed; never
  clone a registered Node horizontally.
- Standalone: the single node is everything; recover process and host-side
  dependencies (PostgreSQL/Redis) per the deployment channel.

## 3. Recovery criteria

- `web_certificate_node_state.observed_at` caught up to the latest version;
- node `/healthz` and the data-plane `/readyz` (loopback operations listener) pass;
- the TLS snapshot generation matches the control-plane publication and the
  live edge fingerprint equals
  `web_certificate_version.fingerprint_sha256`.

Fleet-wide divergence (more than one node behind at once) points at the control
plane or the internal API first, not at per-node repair.
