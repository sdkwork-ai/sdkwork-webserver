# REQ-2026-0037 Bounded Backup Upstream Targets

```yaml
id: REQ-2026-0037
title: Add health-aware bounded backup upstream targets
owner: sdkwork-webserver
status: accepted
source: nginx-upstream-server-backup-commercial-readiness
problem: All currently eligible targets participate in normal weighted rotation. Operators cannot reserve a disaster-recovery peer that receives no routine traffic and activates only after the primary tier is unavailable.
goals:
  - Add a target-level `backup` boolean aligned with the common Nginx upstream `server ... backup` intent.
  - Require at least one non-backup primary target in every upstream.
  - Exclude backups while any unattempted primary target is currently active/passively eligible.
  - Fall back to the backup tier only when no eligible unattempted primary remains, including within a bounded retry sequence.
  - Preserve weighted selection, one half-open probe, active/passive health, target connection limits, and immutable Watch generation ownership inside each tier.
non_goals:
  - Nginx shared `zone`, cross-node failover consensus, priority levels beyond primary/backup, slow start, drain, or connection prewarming.
  - Sending mirrored, shadow, hedged, or simultaneous traffic to backup targets.
users:
  - web server application authors
  - node operators
acceptance_criteria:
  - Omitted `backup` preserves existing all-primary weighted round robin.
  - Schema rejects non-boolean values and semantic validation rejects an upstream containing only backups.
  - Healthy backups receive zero routine requests while an eligible primary exists.
  - Passive or active primary unavailability moves traffic to a healthy backup without a request queue or target-state copy.
  - After the passive deadline, one primary half-open probe takes precedence over backup traffic; success restores the primary tier and failure returns traffic to backup.
  - Retry selection exhausts distinct eligible primaries before selecting a distinct backup and never reuses an attempted target.
  - If every primary and backup is unavailable or probing, the existing bounded local 503 remains authoritative.
  - Watch publishes valid backup-role changes as a fresh generation and rejects invalid all-backup candidates without disturbing active traffic.
non_functional_requirements:
  security: Backup selection cannot bypass DNS/SSRF policy, TLS identity, target connection limits, health, replay, or request admission rules.
  privacy: No backup target identity, URL, address, host, route, tenant, user, request, or trace label is introduced.
  performance: Selection performs bounded scans over at most 1,000 fixed targets and allocates no tier list, queue, schedule, or request-level Set beyond the existing fixed retry bitmap.
  reliability: Primary/backup role is immutable per generation and composes with existing atomic health/probe state.
affected_surfaces:
  - sdkwork-webserver-app-config
  - request-data-plane
trace:
  specs:
    - CONFIG_SPEC.md
    - NGINX_SPEC.md
    - PERFORMANCE_SPEC.md
    - SECURITY_SPEC.md
    - RUST_CODE_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway data_plane::proxy
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_weighted_selection
  - cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_safe_retries
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Compatibility Boundary

The field implements a two-tier primary/backup selection policy compatible with the common Nginx `backup` intent. It does not claim Nginx shared-zone state, multi-priority service discovery, slow start, or cluster failover coordination.

## Acceptance Evidence

- The Rust model, root JSON Schema, semantic compiler, checked-in example, and configuration documentation define target `backup` as a strict boolean defaulting to false. Every upstream must contain at least one non-backup primary. `cargo test -p sdkwork-webserver-core --test webserver_config` passes 53 tests, including default, typed, valid tier, non-boolean, and all-backup rejection cases.
- The runtime keeps one immutable role bit in each existing fixed target entry. Selection first checks for an unattempted active/passively eligible primary, then applies the existing bounded cumulative-weight ticket and race fallback only inside that tier. If none remains, it applies the same algorithm to backups. No tier vector, queue, schedule, request-level Set, target-state copy, or additional I/O lock is created.
- The existing fixed retry bitmap composes with tier selection: distinct eligible primaries are exhausted before a distinct backup, and no target can be reused. DNS/SSRF, TLS/mTLS, aggregate/target physical connection limits, response Header bounds, passive/active health, request admission, replay eligibility, total deadline, and fixed metrics remain unchanged for both tiers.
- Unit tests prove primary exhaustion ordering, backup fallback, expired primary half-open precedence, and primary recovery. Real dual-origin tests prove zero routine backup traffic, passive primary failover, continued backup service, half-open primary restoration, primary-to-backup safe retry, Watch role replacement, and all-backup candidate retention. `cargo test -p sdkwork-api-webserver-standalone-gateway --test upstream_weighted_selection` passes 4 tests and `--test upstream_safe_retries` passes 2 tests.
- The complete standalone gateway suite passes 166 tests. `cargo clippy -p sdkwork-api-webserver-standalone-gateway --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, `cargo fmt --all -- --check`, and `git diff --check` pass.
- SDKWork pagination, API operation-pattern, response-envelope, app-SDK consumer-import, application-layering, Rust backend-composition, route-collision, and database-framework validators passed. PostgreSQL lifecycle was not executed or claimed by this requirement; REQ-2026-0004/0049 own that evidence.

## Accepted Boundary

Acceptance covers exactly two immutable tiers: primary and backup. It does not claim slow start, drain, connection prewarming, multi-priority service discovery, Nginx shared-zone state, cross-node failover consensus, mirrored traffic, hedging, or full Nginx directive import/render compatibility.
