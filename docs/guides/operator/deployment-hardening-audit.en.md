# Deployment Hardening Audit

> Status: implemented and verified ｜ Scope: unified install bundles of sdkwork-webserver / sdkwork-api-cloud-gateway (sibling) / sdkwork-cloudrouter / sdkwork-im
> Basis: `sdkwork-specs/OPERATIONS_SPEC.md` §6 (runtime hardening), §1.2 (release.sh gates), ENVIRONMENT_SPEC (five environments)
> Image baseline: `registry.sdkwork.com/apps/sdkwork-webserver-standalone:0.1.2` (image id `a244f53e5a6c`)

---

## 1. Scope and Outcome

Every image in the four unified install bundles was audited across seven dimensions: **restart policy, healthcheck, log rotation, CPU/memory/pids ceilings, file-descriptor limit, Linux security options, and port exposure**. The following hardening was applied:

1. **Process-count ceiling migrated to `deploy.resources.limits.pids`** — Compose v5 rejects setting both `pids_limit` and `deploy.resources.limits.pids` (`can't set distinct values`); all 11 services were migrated, unblocking the demo reinstall.
2. **`security_opt: [no-new-privileges:true]` added to every service** — blocks privilege escalation (setuid/setgid) inside containers; compatible with postgres, redis, and the Rust binaries.
3. **All resource ceilings are env-tunable** (`*_CPU_LIMIT` / `*_MEMORY_LIMIT`); the defaults are production-safe.

## 2. Per-Image Hardening Matrix

| Service | restart | healthcheck | log rotation | CPU/mem | pids | nofile | security_opt | port binding |
|---|---|---|---|---|---|---|---|---|
| webserver (instance) | unless-stopped | mgmt :3800 `/healthz`, start_period 240s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | edge 0.0.0.0:80/443; **mgmt 127.0.0.1** |
| postgres (embedded) | unless-stopped | `pg_isready`, 5s×12 | json-file 50m×3 | 2.0 / 2g | 256 | — | no-new-privileges | internal network only |
| redis (embedded) | unless-stopped | `redis-cli ping`, 5s×12 | json-file 50m×3 | 1.0 / 1g | 128 | — | no-new-privileges | internal network only |
| gateway (sibling) | unless-stopped | `/readyz`, start_period 240s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | internal only (host 391x published by deploy.sh) |
| knowledgebase-rpc | unless-stopped | gRPC health probe | json-file 50m×3 | 1.0 / 1g | 256 | — | no-new-privileges | internal only (50054 mTLS) |
| cloudrouter | unless-stopped | `/healthz`, start_period 60s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | edge 0.0.0.0 (3900 → host 395x) |
| im-gateway | unless-stopped | `/healthz`, start_period 60s | json-file 50m×3 | 4.0 / 4g | 512 | 65536 | no-new-privileges | edge 0.0.0.0:18079 |

Additional governance:

- **Redis memory cap**: `--maxmemory 768mb --maxmemory-policy allkeys-lru` (`REDIS_MAXMEMORY` / `REDIS_MAXMEMORY_POLICY` tunable) — burst traffic evicts stale cache instead of failing writes; RDB/AOF intentionally disabled (sessions/limits are rebuildable; latency wins).
- **Gateway startup order**: `depends_on: knowledgebase-rpc: condition: service_healthy` prevents a burst of failed mTLS dials on first boot.
- **Minimal cap_add**: webserver keeps only `NET_BIND_SERVICE` (bind 80/443); `cap_drop: ALL` is deliberately not used yet (it breaks postgres/redis entrypoint chown — see §6 follow-ups).

## 3. PostgreSQL Connection Budget (external-deps mode)

External host PG 18 with `max_connections=400` (`superuser_reserved_connections=3`); the five environments currently hold ~**265 connections (66% utilization)**.

**Budget formula**:

```
total ≈ Σ_env [ webserver_instances × 10 (SDKWORK_DATABASE_MAX_CONNECTIONS)
             + gateway 50
             + cloudrouter 10
             + im 10
             + kb-rpc ≈ 10 ] + ops/migration transient connections
constraint: total ≤ max_connections − superuser_reserved − 15% burst headroom
```

**Scaling trigger**: recompute whenever any environment exceeds 4 webserver instances or a new environment is added; if exceeded, raise host `max_connections` (each +100 connections ≈ +0.5–1 GB host RAM) and review `shared_buffers`.

## 4. HA and Concurrency Assessment

**In place today**:

- Automatic single-instance crash recovery (`unless-stopped`) plus health gates (install/upgrade fail on unhealthy; release.sh auto-rollback).
- **Horizontal multi-instance scaling**: `install/upgrade --replicas N` runs each instance as its own compose project (`-i<i>`) with stepped edge ports and auto-assigned management ports; instances hold no local state (data lives in PG/Redis/named volumes).
- Health-gated dependency startup (kb-rpc → gateway) with start_period covering cold-start migrations.

**Concurrency envelope**: 4 CPU / 4 GB / 512 pids / nofile 65536 per app container; embedded PG 200 connections, Redis 768 MB. The fundamental limit on single-host scale-out is the **shared single host** (see §5).

**Production HA topology recommendations (resolving the single-host SPOF, in priority order)**:

1. **Load-balanced ingress**: an LB (Nginx/HAProxy/cloud NLB) replaces single-host 80/443 with health-checked round-robin across webserver instances on N hosts.
2. **PostgreSQL streaming replication**: 1 primary + 1 hot standby, `PG_MAX_CONNECTIONS` aligned with the §3 budget; Patroni/repmgr for failover.
3. **Redis Sentinel**: 1 primary + 2 replicas + 3 sentinels; switch `REDIS_HOST` to the sentinel endpoint.
4. **Host anti-affinity**: spread instances of one environment across ≥2 hosts (`--replicas` + repeated install per host).
5. **Registry HA**: dual instances + read replica for `registry.sdkwork.com` so rollbacks never block on image pulls.

## 5. Residual Risk Register (known, non-blocking)

| # | Risk | Impact | Current mitigation |
|---|---|---|---|
| 1 | Plaintext secrets in env files (DB passwords, signing secrets) | Host file disclosure = credential disclosure | 0600 perms + deploy.sh validation; production should adopt Docker secrets / Vault (gateway already reserves `SDKWORK_VAULT_*`) |
| 2 | Embedded/external Redis without a password | Lateral internal access | Internal-network-only binding; production should add `requirepass` + TLS |
| 3 | Management plane (:3800 etc.) without TLS | Local sniffing of admin traffic | Bound to 127.0.0.1; remote access via SSH tunnel |
| 4 | Single-host SPOF | Host outage takes down all environments | §4 HA items 1–4; until then rely on the backup regime (RPO/RTO, OPERATIONS_SPEC §8) |
| 5 | `cap_drop: ALL` not yet enabled | Attack surface wider than minimal | no-new-privileges blocks escalation; enabling cap_drop requires entrypoint validation first |

## 5a. Operational Caveats (discovered during verification)

1. **Correct sync path for compose changes**: every `install` pushes the **dist install bundle's** compose directory onto `/opt/deploy/<module>/bundle/compose/`, overwriting it. Compose edits must follow `repo deployments/docker/ → dist bundle compose/ → install`; editing `/opt/deploy` directly is silently reverted on the next install (verified the hard way during this audit).
2. **Lifecycle operations on multi-replica stacks**: `stop/restart` default `--replicas` back to 1 and only iterate instance 1. When operating a stack deployed with `--replicas N`, pass `--replicas N` again (`bin/docker-deploy.sh restart --environment X --replicas N`), otherwise the other instances are left untouched.

## 6. Follow-ups

1. `cap_drop: [ALL]` + per-service `cap_add` allowlist (validate postgres/redis entrypoints on dev first).
2. Gateway bundle `deploy.sh logs` parity for `--tail/--since/--export` (webserver already supports them).
3. Replace webserver entrypoint rsync `--checksum` with size+mtime (DrvFs cold-start path).
4. Before production cutover: rehearse LB + PG streaming replication + Redis Sentinel per §4.

## 7. Verification Record

- Compose v5 validation: all 4 compose files parse; 11/11 services carry `deploy.resources.limits.pids` and `security_opt`; the `pids_limit` keyword is fully eliminated across the repos (kernel cloud compose is a legacy single-instance flavor with no conflict, left as-is).
- Demo reinstall (three iterations): `bin/docker-deploy.sh install --environment demo --deps external --host wsl --yes` exits 0; after the third install recreated the containers, `docker inspect` confirms:
  - webserver: `Memory=4G NanoCpus=4 PidsLimit=512 SecurityOpt=[no-new-privileges:true] Restart=unless-stopped Health=healthy`
  - gateway: `Memory=4G NanoCpus=4 PidsLimit=512 SecurityOpt=[no-new-privileges:true] Health=healthy`
  - knowledgebase-rpc: `Memory=1G NanoCpus=1 PidsLimit=256 SecurityOpt=[no-new-privileges:true] Health=healthy`
- Health checks: mgmt `127.0.0.1:19080/healthz` → 200; edge `demo.sdkwork.com` (:19098, `--noproxy '*'`) → 200.
- Multi-instance HA: `install --replicas 2 --dry-run` passes (`--replicas 2` propagated to deploy.sh); the deploy.sh instance loop = one compose project per instance (`-i<i>`) + management ports stepped at `PORT_BASE+index-1` + instance 1 owning the 80/443 edge and running migrations first (see §5a caveat 2).

---

*中文版：[deployment-hardening-audit.md](deployment-hardening-audit.md)*
