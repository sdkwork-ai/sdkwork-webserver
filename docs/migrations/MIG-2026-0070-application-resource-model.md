# MIG-2026-0070 Application Resource Model

```yaml
id: MIG-2026-0070
owner: sdkwork-web-server
status: active
requirement: REQ-2026-0070
type: breaking
scope:
  producers:
    - apis/app-api/web/openapi.yaml
    - crates/sdkwork-routes-webserver-app-api
    - crates/sdkwork-webserver-contract
    - crates/sdkwork-intelligence-webserver-service
    - database/web_application
    - database/web_site
  consumers:
    - sdkwork-web-app-sdk
    - sdkwork-web-backend-sdk
    - sdkwork-webserver-pc-console-core
    - sdkwork-webserver-pc-console-sites
compatibility_window:
  starts_at: 2026-08-09
  ends_at: 0.2.0
strategy: no-compatibility-approved
rollback:
  supported: false
  steps:
    - The application resource model is a pre-launch (0.1.0) contract fix;
      MIGRATION_SPEC section 4.4 forbids compatibility aliases for pre-launch
      applications. Revert the contract, generated SDKs, and the database
      backfill together.
verification:
  - pnpm api:check
  - pnpm sdk:generate:check
  - cargo test -p sdkwork-webserver-contract -p sdkwork-intelligence-webserver-service
  - pnpm --dir apps/sdkwork-webserver-pc test
```

## Summary

The Web Server application-facing API named its top-level resource `site`
(`/app/v3/api/sites`) while the backend API used `application`
(`/backend/v3/api/applications`) for the same data — a dual-name violation of
`API_SPEC.md` section 1. This migration unifies the resource on `application`
for the app surface and introduces the application resource model in the
database:

- App API paths: `/app/v3/api/sites*` → `/app/v3/api/applications*`
  (`{siteId}` → `{applicationId}`), operationIds `sites.*` →
  `applications.*`.
- New `web_application` table: the tenant-facing application entity owns the
  resource identity (name/slug/description). `web_site` remains the internal
  site carrier row (runtime type, status, runtime config, domains,
  deployments); `web_application.site_id` links the two (1:1, mirroring the
  `deploy_app.site_id` model in sdkwork-deployments).
- Creating an application creates its backing site row in one transaction;
  child resources (domains, source versions, deployments, env variables,
  health checks) are resolved through the application's site.
- Permissions: `web.sites.read/write` → `web.applications.read/write`
  (IAM module manifest, OpenAPI `x-sdkwork-permission`, app manifests,
  PC frontend).

## Compatibility

None. Pre-launch contract fix: the old `/app/v3/api/sites` paths return 404,
the generated SDKs no longer expose `site.*` methods, and the console resource
is `/console/applications`.

## Forward Fix

- Database: `0001_web_baseline.sql` creates `web_application` and back-fills one
  application row per live site (idempotent `DO $$` block).
- Code: the service `create_application` transaction inserts the site carrier
  and the application row, then links `site_id`; reads join both tables.
- Frontend: console module id `sites` → `applications`, routes
  `/console/sites` → `/console/applications`.
