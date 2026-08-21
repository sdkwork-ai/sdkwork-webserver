# MIG-2026-0068 Root Domain Zones

```yaml
id: MIG-2026-0068
owner: sdkwork-webserver
status: active
requirement: REQ-2026-0068
type: mixed
scope:
  producers:
    - database/web_root_domain
    - sdkwork-web.backend
  consumers:
    - sdkwork-web-backend-sdk
    - sdkwork-webserver-pc-admin-domains
compatibility_window:
  starts_at: 2026-07-30
  ends_at: 0.2.0
strategy: expand-contract
rollback:
  supported: false
  steps:
    - Keep the additive table and nullable foreign-key column in place.
    - Restore the prior application version while root-domain rows remain dormant.
    - Correct incompatible data or code through a forward migration before re-enabling the feature.
verification:
  - pnpm db:validate
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity
  - pnpm api:check
  - pnpm sdk:generate:check
  - pnpm --dir apps/sdkwork-webserver-pc check
```

## Compatibility

The migration adds `web_root_domain` and nullable `web_domain.root_domain_id`. Existing flat domain
rows and APIs remain valid. No public-suffix inference or historical data backfill is performed.
Operators explicitly define Zones and add new apex or child hostnames through the new API.

## Forward Fix

The migration is intentionally irreversible because later hostnames may reference the new table.
Recovery keeps the additive schema, disables the feature at the application layer, and ships a
forward migration after the failed precondition or contract is corrected.
