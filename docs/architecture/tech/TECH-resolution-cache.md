# Resolution Cache (Multi-Layer DNS/IP Cache)

Status: active
Owner: SDKWork Web Server
Application: sdkwork-webserver
Updated: 2026-08-22

## 1. Purpose

Hostname/IP resolution in the SDKWork data plane is served through a
pluggable multi-layer cache chain, aligned to nginx-grade operational
behavior:

```
local file (deployment seed)
        │ miss
        ▼
in-process memory (LRU + TTL)
        │ miss
        ▼
Redis (distributed, cluster-ready)
        │ miss
        ▼
database (web_resolution_cache)
        │ miss
        ▼
system resolver (fallback) ──► back-fills every layer (positive or negative)
```

Hits on any layer back-fill the upper layers; failed upstream resolutions
are negative-cached with a short TTL so transient DNS outages do not
amplify into a thundering herd (per-domain single-flight collapses
concurrent lookups to one upstream call).

## 2. Component

`crates/sdkwork-webserver-resolver-cache` is an independent crate:

- `ResolverCacheBackend` — the cache plugin interface (`get`/`set`/`remove`
  with TTL-carrying `ResolvedRecord`).
- `InMemoryResolverCache` — bounded LRU + TTL in-process backend.
- `RedisResolverCache` (`redis` feature) — distributed backend with
  namespaced keys and cluster-side expiry.
- `ResolutionDatabase` — database boundary; the SQL implementation lives in
  `sdkwork-intelligence-webserver-repository-sqlx` over the
  `web_resolution_cache` table.
- `ResolutionChain` — the orchestrator (walk order, back-fill, negative
  caching, single-flight).
- `FileResolverSource` — `/etc/hosts`-style seed parsing.

The component has no hard Redis/database dependency: layers activate by
configuration, so a bare deployment runs on file + memory alone.

## 3. Configuration

JSON app config (`resolutionCache`) or the layout-v2 equivalent:

```json
"resolutionCache": {
  "enabled": true,
  "file": "/etc/sdkwork/webserver/resolver.seed",
  "memory": true,
  "memoryMaxEntries": 65536,
  "memoryTtlSeconds": 60,
  "negativeTtlSeconds": 10,
  "redis": {
    "url": "redis://redis-cluster:6379",
    "ttlSeconds": 300,
    "prefix": "sdkwork:resolver"
  },
  "database": true
}
```

- `file` — deployment seed path (hosts style). Loading failures fail
  closed.
- `redis` — distributed layer; connection failure degrades to a warning
  and the chain keeps serving the upper layers.
- `database` — reads/writes `web_resolution_cache` through the
  process-shared database pool (management deployments); without a pool it
  degrades with a warning.
- `negativeTtlSeconds` — TTL for failed resolutions (fast-fail absorption).

All cached addresses still pass the upstream SSRF address policy before
use.

## 4. Deployment System Integration (sdkwork-deployments)

The deploy control plane (`sdkwork-deployments`) is the authority for
domains and reverse-proxy inventory:

1. **Seed file** — `sdkwork-deployments` exports its domain/IP inventory
   into the configured `file` (hosts style, one entry per line:
   `IP hostname [alias ...]`). The data plane parses it into the memory
   layer at startup; requests never touch the system resolver for seeded
   names.
2. **Database inventory** — the deploy plane seeds and maintains
   `web_resolution_cache` (same table the data plane reads and
   back-fills). Entries carry `expires_at`; expired rows are ignored by
   `load` and the data plane refreshes them from upstream resolutions.
3. **Negative entries** — when the data plane cannot resolve a name, it
   writes a short-TTL negative row so concurrent nodes share the fast-fail
   signal instead of each hammering DNS.

## 5. Operations

- Invalidation: `ResolutionChain::invalidate(domain)` drops the entry from
  memory and Redis (deploy-side domain changes should call it or rely on
  TTL expiry).
- Observability: chain build logs the active layers; DNS metrics
  (`dns_result` histogram) still record cache hits vs system lookups.
- Redis is optional and cluster-safe: records are JSON under
  `<prefix>:<domain>` with `EX` TTL; any node may serve or refresh them.
