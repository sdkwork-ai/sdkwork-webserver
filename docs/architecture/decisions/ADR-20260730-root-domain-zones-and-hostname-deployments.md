# ADR-20260730 Root Domain Zones And Hostname Deployments

Status: accepted
Requirement: REQ-2026-0068
Owner: sdkwork-webserver
Date: 2026-07-30
Updated: 2026-07-31
Specs: DATABASE_SPEC.md, API_SPEC.md, PAGINATION_SPEC.md, WEB_BACKEND_SPEC.md, SDK_SPEC.md, BACKEND_UI_SPEC.md, SECURITY_SPEC.md

## Context

A flat hostname inventory cannot provide an industry-standard domain workflow. Operators first
claim a root-domain Zone, then open that Zone to manage its apex and subdomains, application
routes, ownership evidence, certificates, and deployment readiness. Browser-side public-suffix
guessing cannot be the ownership authority and cannot provide store-level pagination.

## Decision

- `web_root_domain` is the explicit tenant-owned Zone authority. Active normalized root hostnames
  are globally unique because two tenants cannot safely control the same public name.
- Every `web_domain` has a required `(tenant_id, root_domain_id)` foreign key. The apex uses record
  name `@`; child labels are normalized by the service into fully qualified hostnames.
- Root domains and hostname children have independent, tenant-filtered repository pagination.
- Application association is represented only by `web_site_binding`; deployment visibility is a
  read projection from the bound `web_site` and its latest `web_deployment`.
- Hostname rows expose verification, route binding, certificate count, HTTPS readiness, latest
  deployment, and state-aware operations. They do not duplicate those authorities.
- Root-domain deletion is blocked while any live hostname child exists. Hostname deletion is
  blocked while route or certificate references exist.
- DNS provider metadata may identify an external Zone, but Web does not claim authoritative DNS
  propagation until a provider contract, reconciliation loop, and observation evidence exist.

## Alternatives

- Derive Zones from suffixes at query time. Rejected because ownership and deletion require an
  explicit durable resource and public suffixes do not express operator intent.
- Make `root_domain_id` nullable for compatibility. Rejected in the prelaunch model because it
  preserves an ungrouped state that the product cannot manage consistently.
- Persist a latest deployment on each hostname. Rejected because deployment remains application
  authority and projections can be indexed without introducing reconciliation drift.

## Consequences

The backend-admin UI has a stable root list and `/admin/root-domains/{rootDomainId}` detail route.
Each list stays bounded at the repository. Application, certificate, and deployment lifecycles can
evolve independently while their relationship remains visible in one operational table.

## Verification

- PostgreSQL baseline and contract validation.
- Tenant isolation, store pagination, normalized hostname, and deletion conflict tests.
- Route manifest, OpenAPI, SDK generation, and consumer import checks.
- Backend-admin interaction, stale-response protection, responsive layout, i18n, and permission
  tests.

## Supersedes / Superseded By

This record replaces its prelaunch nullable-root/direct-application draft while retaining the
stable ADR identifier.
