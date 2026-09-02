# SDKWork Web Server Production Operations PRD

Status: active
Owner: SDKWork maintainers
Application: sdkwork-web
Updated: 2026-07-15
Parent: [PRD.md](PRD.md)
Specs: DEPLOYMENT_SPEC.md, CONFIG_SPEC.md, RUNTIME_DIRECTORY_SPEC.md, SECURITY_SPEC.md, OBSERVABILITY_SPEC.md, PERFORMANCE_SPEC.md, TEST_SPEC.md

## 1. Purpose

Define the process lifecycle, host integration, overload behavior, observability, high availability, upgrade, recovery, support, and release requirements needed to operate SDKWork Web Server commercially in standalone and cloud environments.

A fast request handler is not a production Web Server unless operators can validate it before start, know exactly what revision is serving, constrain its resources, reload and upgrade it without traffic corruption, diagnose it under pressure, recover from dependency outages, and roll it back using verified evidence.

## 2. Deployment Profiles

The product supports the SDKWork `standalone` and `cloud` deployment profiles without changing application traffic semantics.

| Profile | Runtime expectation |
| --- | --- |
| Standalone service | One self-contained Web Server installation or container serves one or more configured apps, uses PostgreSQL authoritative control-plane state and local verified runtime snapshots, and is supervised by the OS/container runtime. |
| Cloud | Horizontally scaled stateless request data-plane nodes serve node-scoped immutable snapshots; PostgreSQL is control-plane authority; managed secrets, ingress/load balancing, orchestration, autoscaling, disruption control, and multi-zone placement are explicit. |

Standalone is not automatically highly available. Cloud is not automatically highly available merely because multiple replicas exist. Availability claims require independent failure domains, health-based traffic removal, bounded shared dependencies, rollout safety, capacity headroom, and tested recovery.

The Web Server may run directly on the public edge or behind an approved L4/L7 load balancer. When Nginx is placed in front, it is a deployment choice and compatibility target, not a dependency required for the Rust runtime to provide HTTP/HTTPS behavior.

## 3. Host Runtime Configuration

Host runtime configuration follows SDKWork typed server configuration and canonical runtime directories. It owns:

- Environment, deployment profile, runtime target, process identity, and local node identity.
- Physical bind addresses, socket options, public URL, trusted proxy networks, and administrative exposure.
- Service account, privilege policy, runtime/data/cache/log/temp/config directories, file permissions, and secret providers.
- Worker/runtime sizing, blocking/crypto pools, global memory ceiling, emergency reserve, descriptor budget, disk quotas, and queue ceilings.
- Snapshot source, signature trust, rollback retention, bootstrap deadline, reload deadline, drain deadline, and offline behavior.
- Observability exporters, log sinks, redaction, support diagnostics, profiling policy, and telemetry queue bounds.

Application traffic configuration cannot weaken host policy. Environment variables may select or narrowly override safe operational values according to `CONFIG_SPEC.md`; production secrets and large structured route catalogs do not belong in environment variables.

Production configuration comes from administrator-managed files, deployment infrastructure, or secret managers. Committed templates contain safe placeholders only.

## 4. Process And Privilege Model

- Production request workers run as a dedicated non-root service identity with only required filesystem, network, and secret access.
- Binding privileged ports uses an approved capability, socket activation, container capability, or external load balancer. The long-running process does not retain unrestricted root privilege.
- Config, binaries, shared assets, mutable data, logs, cache, runtime state, temporary files, and secrets use SDKWork canonical directories with explicit owner and mode/ACL verification.
- Linux service packages integrate with systemd or an approved equivalent; Windows packages integrate with Windows Service Control; macOS service packages use launchd where supported; containers follow PID 1, signal, stdout/stderr, read-only root filesystem, and termination-grace conventions.
- The runtime does not daemonize inside containers. Service-manager modes do not fork into an untracked process.
- Core dumps, panic dumps, and diagnostic captures are disabled or protected according to secret exposure policy.

The implementation may use one multi-threaded process, a supervisor plus workers, or socket-activated generations, but the selected model requires an ADR covering crash isolation, socket ownership, TLS/session state, reload, executable upgrade, signals, metrics aggregation, and platform parity.

## 5. Operator Commands And Exit Contract

The packaged server provides non-interactive operations equivalent to:

| Operation | Required result |
| --- | --- |
| `validate` | Parse and fully validate host config, app snapshots, references, budgets, TLS, and socket compatibility without accepting traffic. |
| `dump` | Emit canonical redacted effective configuration and checksums without secret values. |
| `explain` | Show source/precedence for a field and source-mapped diagnostics. |
| `start` | Start the selected verified generation and wait for bootstrap result. |
| `status` | Report process, listener, snapshot, readiness, drain, dependency, and resource state. |
| `reload` | Stage and atomically activate a verified candidate without dropping accepted healthy connections. |
| `drain` | Stop new traffic and complete eligible work to a deadline. |
| `stop` | Perform graceful stop, then deterministic forced termination at the configured deadline. |
| `version` | Report product version, build identity, protocol/TLS features, compatibility profile, and supply-chain identity. |

Exit codes distinguish configuration, permission, bind, secret, certificate, resource, compatibility, dependency, activation, and internal failures. Commands do not report success because a file was written or a signal was sent; they wait for or query verifiable generation state according to their timeout.

## 6. Signals And Lifecycle

Lifecycle events have platform-equivalent behavior:

- Graceful termination stops new accepts, marks not-ready, initiates protocol-aware drain, flushes bounded essential telemetry, releases leases, and exits by deadline.
- Immediate termination is reserved for operator escalation after graceful timeout and is never the normal deployment path.
- Reload requests are coalesced and generation-fenced. A later candidate supersedes an older unactivated candidate without concurrent mutation of live state.
- Diagnostic or log-reopen operations cannot trigger configuration reload or expose secret state.
- Unexpected worker/task/process exit is observed by the supervisor/orchestrator and cannot leave the node falsely ready.

For HTTP/1.x, drain disables keep-alive for new responses where safe and stops new accepts. For HTTP/2, drain sends GOAWAY with correct stream handling. WebSocket, SSE, uploads, downloads, and long-lived streams follow route-specific drain policies with a hard global deadline.

## 7. Atomic Configuration Reload

Reload uses generation state, never in-place mutation:

1. Acquire a per-node/per-listener generation lease or local transition lock.
2. Load and authenticate the exact candidate snapshot and resources.
3. Parse, validate, compile, probe, and allocate all required state off the active path.
4. Confirm sockets, certificates, routes, upstream policy, caches, budgets, and host policy are internally consistent.
5. Atomically publish the complete candidate for new requests.
6. Retain old state only for requests/connections that reference it and for the bounded rollback window.
7. Verify local and external served behavior and node revision.
8. Mark success and garbage-collect superseded generations when no longer referenced.

Failed candidates never partially alter active listeners, TLS maps, routes, policies, or upstreams. Reference-counted generations and caches have hard retention limits; a stuck connection cannot retain unlimited historical snapshots. When the limit is approached, the runtime applies the declared long-lived-connection drain policy before accepting another reload.

## 8. Zero-Downtime Executable Upgrade

Executable upgrade is distinct from configuration reload. It requires:

- Immutable signed binary/image identity, SBOM, provenance, checksum, compatibility metadata, and migration preflight.
- A new generation that validates config and resources before receiving traffic.
- Socket activation, controlled descriptor inheritance, `reusePort`, load-balancer handoff, or another reviewed mechanism that prevents an accept gap and split ownership.
- Readiness proof for the new generation before the old generation drains.
- Protocol-aware connection drain and a bounded maximum overlap period.
- Rollback to the previous compatible binary and snapshot when health/SLO checks fail.
- Compatibility rules for snapshot schema, local state, cache format, control-plane protocol, database migrations, and agent protocol.

An in-place binary replacement followed by process restart is not a zero-downtime upgrade. Database schema changes use expand/contract sequencing and cannot make the previous release unable to roll back during the declared window.

## 9. Health, Readiness, And Startup

Health surfaces are separately defined:

| Surface | Meaning |
| --- | --- |
| Startup | Bootstrap is still making bounded progress and has not exceeded its deadline. |
| Liveness | The process, event loops, supervisors, and essential internal control paths can make progress. It does not depend on every upstream. |
| Readiness | The node can safely accept its assigned traffic with the intended snapshot, listeners, certificates, resource reserve, and required local dependencies. |
| Application probe | A selected host/route/upstream behavior works from the required vantage point. |

Liveness does not fail merely because PostgreSQL, the control plane, one upstream, ACME, or telemetry export is unavailable. Readiness is scoped to affected listeners/applications where the platform can route that granularity; otherwise the node is removed conservatively.

Health responses are bounded and reveal no tenant data, secret, internal hostname, SQL, stack trace, certificate private data, or complete configuration. Public exposure is disabled by default or restricted to an explicitly safe summary.

## 10. Administrative Surface Isolation

- Administration binds to a separate loopback, Unix socket/named pipe, private network, or mTLS-protected listener according to deployment profile.
- Application virtual hosts cannot route to internal admin, metrics, profiling, snapshot, cache-purge, certificate, reload, or debug handlers unless a separate reviewed exposure policy explicitly publishes a safe operation.
- Remote management uses SDKWork IAM/RBAC, tenant and object authorization, rate limits, idempotency, optimistic concurrency, audit, and standard API contracts.
- Node-local destructive or secret-sensitive operations require local privilege and are not made available through ordinary tenant app-api routes.
- Debug/profiling endpoints are disabled by default, bounded in duration/output, rate-limited, audited, and never exposed on public production listeners.

The request data plane remains operable when the remote administrative surface is unavailable or intentionally disabled.

## 11. Overload And Load Shedding

The server maintains operating headroom instead of failing only at resource exhaustion:

1. Observe memory, descriptors, event-loop lag, worker queues, handshakes, connections, streams, upstream pools, disk, and telemetry saturation.
2. Stop optional background work and reduce nonessential telemetry/detail before the emergency reserve is threatened.
3. Reject new low-priority work at the narrowest responsible scope.
4. Return protocol-appropriate bounded overload responses with safe retry guidance when possible.
5. Preserve health, readiness transition, diagnostics, drain, rollback, and already accepted high-priority work within policy.
6. Remove the node from traffic before hard failure when recovery is not occurring.

Admission priorities and tenant fairness are explicit. One application, route, source, TLS handshake flood, cache miss storm, slow upstream, or log exporter cannot consume the entire node budget. Retry responses and connection resets are designed to avoid synchronized client retry storms.

Overload control itself uses bounded state and low-cardinality keys. It must not allocate per untrusted identity indefinitely.

Current verified boundary: REQ-2026-0027 implements one optional standalone-process governor for Windows Working Set/HANDLE count, Linux RSS/FD count and finite cgroup v2 memory, plus cross-platform event-loop wake lag. It uses absolute reserves, strict effective admission/recovery thresholds, consecutive-sample hysteresis, a total request ceiling with ordinary-business partition, pre-task socket shedding, and a finite established-connection reserve limited to exact fixed health responses. Real HTTPS/HTTP2 tests exercise HANDLE exhaustion, Stream-scoped `503`/`Retry-After`, operations availability, invalid reserve candidates, new-socket close, recovery, Restart-only Watch behavior, and sampler shutdown. Hard allocator enforcement, CPU/PSI/disk pressure, per-tenant fairness, separate management ingress, metrics/alerts, autoscaling, cluster coordination, and load/soak proof remain required by this PRD.

## 12. Dependency Failure And Degraded Modes

Each dependency declares whether it is bootstrap-required, request-path-required, background-required, or optional, plus timeout, retry, circuit, stale-data, readiness, and alert behavior.

| Failure | Required behavior |
| --- | --- |
| Control plane/PostgreSQL | Continue last verified data-plane snapshot; management mutations become unavailable or durable-pending, never fake-success. |
| PostgreSQL unavailable | Continue the last verified runtime snapshot where safe; block state-changing management, fail readiness for control-plane writes, and alert persistence failure. |
| Redis/shared limiter | Follow explicit fail-open/fail-closed policy per security tier; never silently change scope. |
| DNS resolver | Use bounded last-valid addresses only within declared stale window; stop retry storms. |
| Upstream | Health-aware failover, bounded queues/retries, circuit behavior, and deterministic error/stale-cache policy. |
| KMS/secret manager | Continue already loaded authorized keys within policy; block new activation/rotation that cannot resolve secrets. |
| ACME/OCSP | Continue valid active certificate according to HTTPS policy; alert renewal/revocation risk. |
| Telemetry backend | Buffer/drop according to bounded policy; never block request serving indefinitely. |
| Disk full/read-only | Preserve request serving where possible, stop cache/spool/support writes, bound logs, and fail operations requiring durable writes. |

No degraded mode weakens authentication, tenant isolation, TLS verification, path confinement, or private-key protection without an explicit approved security exception.

## 13. High Availability And Traffic Topology

Cloud production requires:

- At least three data-plane replicas across declared failure domains for the 99.99% target.
- Health-based L4/L7 traffic distribution with connection draining and readiness-aware rollout.
- No single-node local cache, limiter, session ticket key, or health state misrepresented as cluster-global behavior.
- Node-scoped signed snapshot and secret distribution with convergence evidence.
- Capacity for one failure domain loss while remaining within latency and resource targets.
- Canary and phased rollout with automatic stop, SLO gates, and bounded rollback.
- Anti-affinity, disruption budgets, surge/unavailable limits, autoscaling signals, and scale-down drain.
- Time synchronization and bounded clock-skew handling for TLS, signatures, leases, logs, and expiry.

Request data-plane nodes do not coordinate on every request. Shared rate limiting, global quotas, distributed cache invalidation, or consistent session behavior requires an explicit shared-state design and failure policy. Local fallback scopes are visible and tested.

Multi-region active/active or active/passive operation requires a separate topology requirement covering DNS/anycast/load balancing, configuration authority, certificate/KMS locality, data replication, failover, split brain, client affinity, capacity, RPO/RTO, and regional isolation.

## 14. Observability

Operational telemetry provides four signals plus capacity:

- Traffic: requests, connections, streams, handshakes, bytes, protocols, hosts/routes, and response classes.
- Errors: client protocol, policy rejection, server, upstream, TLS, DNS, cache, config, process, and dependency failures.
- Latency: accept, handshake, request phases, upstream attempts, queue time, response, config reload, drain, and upgrade.
- Saturation: memory, descriptors, event-loop lag, worker/queue depth, upstream pools, disk, cache, spool, telemetry, and emergency reserve.
- State: binary/build, active snapshot, certificates, node assignment, readiness, drain, rollout, and convergence.

Logs are structured and redacted. Container profiles default to stdout/stderr; service profiles use journald/system logging or the canonical SDKWork log directory. File rotation, retention, compression, deletion, and disk quotas are owned by one declared layer so external and internal rotation cannot race.

Metrics and traces never use raw domain, path, query, user, tenant, certificate, upstream address, or error text as unbounded labels. Export is asynchronous and bounded. Audit records are append-oriented and tamper-resistant for sensitive operations.

Current verified boundary: REQ-2026-0032 shares one framework HTTP metric registry across the management process's app-api, backend-api, and `/metrics` handler. Request and pipeline-stage series have hard count and label-byte ceilings, unresolved routes collapse to one `unmatched` label, and overflow is counted without allocating another series or rejecting business traffic. Only canonical SDKWork environment, deployment profile, runtime target, and database profile dimensions are admitted.

REQ-2026-0033 adds a separate opt-in loopback host operations listener and fixed-atomic request-data-plane registry. It reports connection/request lifetime and admission, response classes, upstream outcomes, aggregate target health, resource-pressure state/reasons, reload outcomes, and WebSocket tunnel lifecycle with constant series cardinality. The listener itself is HTTP/1-only and bounded to 32 non-queuing connections, a 16 KiB Header buffer, five-second Header/request deadlines, a 60-second connection lifetime, and one-second drain. Application virtual hosts never receive these routes.

REQ-2026-0034 adds fixed request and upstream response-Header histograms, request/response/WebSocket byte counters, normalized protocol/body/write failures, DNS active/result metrics, and aggregate current-generation request/physical-connection capacity. Histogram buckets and every label vocabulary are fixed; hot-path observations perform bounded atomic work and no payload collection. Tunnel byte totals are authoritative on successful bidirectional-copy completion only because Tokio exposes no partial totals on copy error. Hyper idle-pool occupancy, accept/TLS/cache phase histograms, traces, bounded exporters, authenticated remote access, dashboards, alerts, autoscaling policy, cluster aggregation, and long-term storage remain release requirements.

REQ-2026-0035 adds opt-in node-local sequential upstream retries for Body-end-of-stream idempotent requests. The attempt count, total wall-clock budget, target scan, attempted-target state, reason vocabulary, admission ownership, and every transport phase are bounded; cancellation releases the active metric/probe ownership. Local connection/request saturation, client Body failure, POST/PATCH, any pending Body/Trailer, and WebSocket upgrades never retry. Shared-zone or cluster retry budgets, non-idempotent/idempotency-key replay, payload buffering/spooling, hedging, and cluster circuit coordination remain release requirements where product scope demands them.

REQ-2026-0036 adds optional node-local physical connection capacity per unique target authority beneath the existing aggregate upstream cap. Connector admission is non-queuing and both permits follow the socket through connect, TLS, active H1, multiplexed H2, idle pooling, Watch retirement, and shutdown. Operations telemetry exposes only aggregate configured/in-use/available target capacity. Nginx shared zones and cross-process/cross-node accounting remain release requirements for scale-out profiles.

REQ-2026-0037 adds a bounded primary/backup target tier. Healthy backups receive no routine business traffic; passive or active primary unavailability activates only currently healthy backups, and one expired primary half-open probe takes precedence for recovery. Safe retries can cross from exhausted distinct primaries into backups without parallel attempts or payload replay. Multi-priority discovery, slow start, drain, shared failover state, and cross-node coordination remain release requirements where deployment scope demands them.

## 15. Capacity Planning And Autoscaling

Every production profile publishes tested capacity per instance for:

- Requests and bandwidth by static, proxy, cached, TLS, WebSocket/SSE, and gRPC workload.
- Concurrent accepted/active/idle connections and HTTP/2 streams.
- TLS full/resumed handshakes and certificate/SNI cardinality.
- Routes, virtual hosts, upstream targets, certificates, snapshots, cache entries/bytes, and log volume.
- Memory, CPU, descriptors, network, disk IOPS/space, event-loop lag, and worker queues at normal, surge, and failure-domain-loss load.

Autoscaling uses leading saturation and concurrency signals in addition to CPU. Scale-up lead time, new-node snapshot/certificate warmup, readiness, minimum replicas, maximum replicas, and scale-down drain are included. Autoscaling is not a substitute for per-node admission control.

Capacity claims identify hardware, OS/kernel, network, TLS algorithms, payload mix, upstream latency, config size, test duration, generator topology, confidence interval, and limiting resource.

## 16. Backup, Restore, And Disaster Recovery

- PostgreSQL backups, point-in-time recovery, migration state, encrypted control-plane secrets, configuration revisions, audit evidence, and certificate metadata follow documented retention and restore procedures.
- Private keys are backed up only through approved encrypted KMS/secret mechanisms; plaintext key archives are forbidden.
- Local data-plane snapshots are recovery caches, not the sole authoritative cloud backup.
- Restore exercises prove database integrity, tenant isolation, snapshot regeneration, certificate usability, node re-enrollment, served traffic, and audit continuity.
- Disaster-recovery exercises measure the parent PRD RPO/RTO and record unmet dependencies rather than declaring success after database restore alone.

Current verified boundary: REQ-2026-0050 adds a disposable, bounded recovery drill using PostgreSQL custom-format `pg_dump`/`pg_restore`. It proves independent restored schema integrity and a tenant-scoped canary after the source diverges. Production scheduling, encryption/KMS, immutable off-host retention, PostgreSQL WAL/PITR, managed-provider recovery, node/certificate reconstruction, audit continuity, and measured RPO/RTO remain required by this PRD.

REQ-2026-0051 adds a bounded two-node PostgreSQL physical-replication drill. It proves a tenant write whose primary flush LSN is explicitly replayed on the standby survives primary shutdown, standby promotion, and subsequent writes. It does not establish automatic failure detection, leader election, client endpoint failover, synchronous-replication RPO, split-brain fencing, failback/rejoin, managed-provider behavior, independent failure domains, three-node capacity, or the product availability and RPO/RTO targets.

REQ-2026-0052 replaces the Web Node Daemon's legacy best-effort temporary `lastSyncVersion` file with a bounded, checksummed, atomically persisted desired/observed generation checkpoint. Desired is durable before artifact application; observed advances only after the supplied bundle and real Nginx reload succeed, so interrupted generations request a complete replay. This is a local apply checkpoint, not served-state, inventory equality, control-plane acknowledgement, readiness, quorum, or cluster convergence evidence. The v3 state field remains a compatibility identifier rather than canonical product terminology.

## 17. Release And Supply Chain

Commercial releases include:

- Reproducible or controlled builds, locked dependencies, vulnerability and license review, SBOM, provenance, signatures, checksums, and immutable artifact identity.
- Linux x64/arm64 cloud and standalone targets required by the release plan; additional Windows/macOS service targets declare parity or documented limitations.
- Protocol, TLS, Nginx profile, snapshot schema, control-plane API, database migration, agent protocol, config schema, and platform support matrices.
- Upgrade and downgrade paths, deprecations, security advisories, migration notes, rollback window, and end-of-support policy.
- Container manifests with non-root user, read-only root filesystem where possible, dropped capabilities, seccomp/AppArmor/SELinux guidance, resource requests/limits, probes, disruption policy, and secret mounts.
- Service packages with canonical directories, permissions, service manager units, config preflight, log policy, uninstall/data retention behavior, and no embedded production secret.

A release is not commercial-ready when it relies on mutable tags, undocumented manual server edits, unverified generated configuration, unbounded defaults, privileged execution without justification, or tests that mock the external effect being claimed.

Current verified boundary: REQ-2026-0056 provides paired `dev:standalone`/`dev:cloud` and
`release:package:standalone`/`release:package:cloud` commands. Cloud development resolves a tracked,
token-free remote-HTTPS profile and starts only the local Web Node Daemon. The workflow plans
separate Linux x64 server archives, and the producer binds names to the workflow package version,
uses deterministic tar metadata, records per-file hashes, writes an archive checksum atomically,
and enforces a 512 MiB ceiling. REQ-2026-0057 additionally freezes the assembled pnpm workspace and
requires an exact-inventory streaming archive validator before upload, with fixed archive, file,
total-content, entry-count, manifest, checksum, and read-buffer limits. Current Windows evidence
uses real bounded tar fixtures but remains contract evidence only. REQ-2026-0058 closes Linux x64
archive extraction, packaged gateway validation, HTTP/HTTPS readiness and traffic, SIGTERM drain,
and cleanup smoke for both standalone and cloud profiles. REQ-2026-0059 adds the same bounded
archive and HTTP/HTTPS/SNI/stop evidence for arm64 using an AArch64 userspace and architecture-bound
manifests. The smoke includes the canonical
`sdkwork-webserver-node-daemon` and the explicitly labelled `sdkwork-webserver-agent` compatibility binary.
Container/service packages, SBOM, signing, provenance, upgrade, rollback, uninstall, native-arm64
capacity/soak, and production HA evidence remain required.

## 18. Runbooks And Supportability

Production requires owned runbooks for:

- Start failure, bind/permission failure, invalid snapshot, and rollback.
- Memory pressure/OOM prevention, descriptor exhaustion, CPU saturation, event-loop lag, disk full, and log loss.
- TLS expiry/rotation/revocation, KMS outage, ACME outage, and clock skew.
- DNS failure/rebinding, upstream outage, retry storm, cache poisoning, and purge.
- Failed reload, stuck drain, failed executable upgrade, divergent nodes, and split rollout.
- PostgreSQL recovery, backup restore, control-plane outage, agent re-enrollment, and audit investigation.
- Security vulnerability, secret exposure, request smuggling, abuse traffic, and tenant isolation incident.

Support bundles are generated through a bounded, redacted, authorized operation and include version/build, host profile summary, checksums, active/previous generations, listener status, resource saturation, recent classified errors, and dependency state. They exclude secrets, private keys, raw tokens, request bodies, unrestricted logs, and tenant content.

## 19. Verification

Required evidence includes:

- Fresh install, upgrade, downgrade where supported, rollback, uninstall, restart, crash, and corrupt-local-state tests for each package/profile.
- Non-root bind, permissions, read-only filesystem, secret mount, signal, systemd/service/container, and runtime-directory tests.
- Reload/upgrade races, concurrent operator command, stale generation, stuck long-lived connection, process crash, and supervisor recovery tests.
- Control-plane, PostgreSQL, Redis, DNS, KMS, ACME, telemetry, upstream, disk, and network partition fault injection.
- Memory, descriptor, disk, CPU, event-loop, handshake, connection, queue, cache, and log overload tests with emergency-reserve verification.
- Multi-node canary, failure-domain loss, node divergence, scale up/down, rolling upgrade, disruption, backup/restore, and RPO/RTO exercises.
- Security, supply-chain, SBOM/signature, vulnerability-response, compatibility, and support-bundle redaction gates.

Operational tests verify the real process, socket, filesystem, TLS handshake, traffic path, node revision, and restored service. A mocked process runner or database state transition cannot be the only evidence.

## 20. Acceptance Criteria

- Standalone and cloud packages can validate, start, become ready, serve, reload, drain, stop, restart, and recover using documented non-interactive operations.
- Production runs without unrestricted root privilege and uses canonical SDKWork runtime directories and secret handling.
- Config reload and executable upgrade preserve accepted healthy traffic, fence concurrent generations, prove served state, and roll back on failure.
- Health/readiness accurately remove unsafe nodes without coupling liveness to every external dependency.
- Overload and dependency failures preserve the emergency operations reserve and degrade according to explicit policy without OOM, deadlock, disk exhaustion, or false readiness.
- Three-node/multi-zone evidence meets the parent availability target and capacity survives the declared failure domain loss.
- Logs, metrics, traces, audits, alerts, dashboards, support bundles, and runbooks cover every release-critical failure mode without secrets or unbounded cardinality.
- Backup/restore and disaster-recovery exercises meet the parent RPO/RTO using PostgreSQL authority for every server deployment profile.
- Release artifacts include signature, checksum, SBOM, provenance, compatibility matrix, migration/rollback guidance, and verified package/container security posture.
