# REQ-2026-0068 Root Domain Zone Management

```yaml
id: REQ-2026-0068
title: Manage root-domain Zones and their hostname operations on dedicated pages
owner: sdkwork-webserver
status: in-progress
source: user
problem: Operators need to define a root domain first, open a stable detail page, and operate its apex and subdomains together with routing, verification, certificates, and deployment visibility.
goals:
  - Make a root domain a first-class tenant-owned Zone.
  - Open each Zone on a dedicated backend-admin route.
  - Page apex and subdomain hostnames independently from the root list.
  - Show application, deployment, verification, HTTPS, and certificate state without duplicating authority.
  - Provide a professional, state-aware operation column.
non_goals:
  - Acting as an authoritative DNS provider before provider write and observation contracts exist.
  - Copying application deployment state onto root-domain or hostname rows.
  - Inferring Zone ownership from browser-side public-suffix heuristics.
users:
  - tenant Web Server administrators
  - application deployment operators
  - certificate operators
acceptance_criteria:
  - Backend administrators can page, search, create, retrieve, and safely delete root domains.
  - Clicking a root domain opens /admin/root-domains/{rootDomainId}.
  - Every hostname has a required rootDomainId; @ creates the apex and normalized labels create children.
  - The detail page independently pages hostname children at the repository boundary.
  - Hostname rows expose application binding, latest application deployment, ownership verification, HTTPS readiness, certificate count, and update time.
  - The operation column offers Verify, Bind application or Unbind application, Manage certificates, and Delete according to permissions and current state.
  - Manage certificates supports inspection, repeated issuance, RSA/ECDSA selection, bind, unbind confirmation, priority, and default state.
  - Unbound hostnames cannot create listener bindings; unverified hostnames cannot issue a certificate.
  - Root deletion is disabled and rejected while hostname children exist.
  - Hostname deletion is disabled and rejected while application or certificate references exist.
  - Read responses are protected from stale pagination races and all user-facing copy is package-localized.
  - Root-domain and hostname queries are tenant filtered, indexed, and store paginated.
non_functional_requirements:
  security: UI permission hints never replace backend authorization; destructive and runtime-affecting operations require confirmation and idempotency.
  privacy: Domain and deployment views contain no secrets or private certificate material.
  performance: Root and child lists use independent LIMIT-based repository pagination and indexed tenant/root predicates.
  reliability: Latest deployment is a read projection from the bound application and never a second state machine.
affected_surfaces:
  - database
  - api
  - sdk
  - backend
  - pc
  - deployment
  - tls
trace:
  specs:
    - DATABASE_SPEC.md
    - API_SPEC.md
    - PAGINATION_SPEC.md
    - WEB_BACKEND_SPEC.md
    - SDK_SPEC.md
    - BACKEND_UI_SPEC.md
    - I18N_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - database
    - crates/sdkwork-webserver-contract
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - crates/sdkwork-routes-webserver-backend-api
    - sdks/sdkwork-web-backend-sdk
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-domains
verification:
  - pnpm db:validate
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx
  - cargo test -p sdkwork-intelligence-webserver-service
  - cargo test -p sdkwork-routes-webserver-backend-api
  - pnpm api:check
  - pnpm sdk:generate:check
  - node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
  - node ../sdkwork-specs/tools/check-i18n-standard.mjs --root apps/sdkwork-webserver-pc
  - pnpm --dir apps/sdkwork-webserver-pc check
```

## Product Decision

A Zone owns hostnames. A site binding owns application routing. A certificate identifier owns SAN
coverage. A listener certificate binding owns TLS selection intent. A deployment owns rollout
history. The root-domain page composes these facts into one operational view without copying their
mutable state.

## Change Control

- 2026-07-31: Made `rootDomainId` required, replaced direct application/certificate fields with
  join-table projections, and added listener certificate and operation-column acceptance criteria.
