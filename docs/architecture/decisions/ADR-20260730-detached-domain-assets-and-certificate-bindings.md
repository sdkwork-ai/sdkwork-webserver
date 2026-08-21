# ADR-20260730 Domain, Certificate, And Listener Bindings

Status: accepted
Requirement: REQ-2026-0067
Owner: sdkwork-webserver
Date: 2026-07-30
Updated: 2026-07-31
Specs: DATABASE_SPEC.md, API_SPEC.md, PAGINATION_SPEC.md, WEB_BACKEND_SPEC.md, SDK_SPEC.md, BACKEND_UI_SPEC.md, DEPLOYMENT_SPEC.md, NGINX_SPEC.md, SECURITY_SPEC.md

## Context

Domains, certificates, application routes, and listener activation have different lifecycles and
cardinalities. A hostname must be registrable and verifiable before it is routed. One application
must support many hostnames. One certificate may cover several SAN hostnames, while one hostname
may retain several certificate lifecycles and may actively serve an RSA and an ECDSA certificate.
Deployment history belongs to the application, not to the hostname or certificate row.
Domain ownership evidence, not application routing, is the prerequisite for managed certificate
issuance. Requiring a route first creates an artificial lifecycle dependency and prevents
operators from preparing HTTPS before traffic is attached.

Direct `site_id` columns on a domain and direct `domain_id` or `site_id` columns on a certificate
cannot represent those relationships without duplicate state or destructive rebinding.

## Decision

- `web_domain` is a tenant-owned hostname under a required `web_root_domain`; it does not contain
  application or certificate ownership columns.
- `web_site_binding` is the routing relation between `web_site` and `web_domain`. It owns
  environment, path prefix, serve/redirect behavior, primary state, and activation state.
- `web_certificate` is a lifecycle aggregate. `web_certificate_identifier` is the ordered
  many-to-many relation to covered hostnames and is bounded to eight SAN identifiers.
- `web_certificate_version` stores immutable certificate evidence and a protected material
  reference. Private keys and PEM data never enter domain rows, API responses, or browser state.
- `web_certificate_operation` stores durable `ISSUE` and `RENEW` intent. API acceptance returns
  HTTP `202`; workers claim with expiring leases and fencing tokens, and only a fenced terminal
  transaction may update the certificate aggregate and immutable version.
- Automatic renewal policy is available only to ACME certificates. Self-signed certificate
  lifecycles remain manually reissuable and are never selected by the automatic scheduler.
- Backend certificate inventory and issuance address verified hostnames through `domainIds` and do
  not require an application binding. An application-scoped API may still use an owned route as
  its authorization boundary without changing certificate ownership.
- `web_listener_certificate_binding` binds a certificate or explicit version to one
  `web_site_binding`. One active RSA and one active ECDSA binding may coexist for the same listener;
  only one active binding is the listener default. Attempting to bind a different certificate to an
  already occupied algorithm returns a typed conflict before the database uniqueness guard.
- A listener binding is allowed only when the certificate covers the hostname and has a usable
  version with matching algorithm. Activation remains fail closed.
- Application deployment visibility is projected through `web_site_binding.site_id` to the latest
  `web_deployment`; no domain-deployment join or copied deployment status is persisted.
- Tenant-level backend operations manage hostname assets and listener certificate bindings. The
  app surface remains owner scoped. Browser feature packages use the injected generated SDK
  facade and never construct HTTP clients.
- Domain management presents certificate lifecycle independently from listener binding. The
  certificate inventory and issue operation remain available for unbound verified hostnames;
  listener controls are conditional on an existing `web_site_binding`.
- Deleting an application binding does not delete the hostname or certificate. Hostname deletion
  is blocked while live route or certificate-identifier references exist.

## Alternatives

- Keep one optional application column on `web_domain`. Rejected because it collapses routing
  policy into asset ownership and cannot model environment/path routes.
- Keep one domain column on `web_certificate`. Rejected because it prevents multi-SAN certificates
  and encourages duplicate certificate lifecycle rows.
- Store relationships in JSON. Rejected because foreign-key integrity, bounded cardinality,
  pagination, filtering, and activation queries must remain enforceable in PostgreSQL.
- Copy deployment state to each hostname. Rejected because it creates a second mutable deployment
  state machine.

## Consequences

The model supports one application with many domains, certificates shared across covered domains,
several certificate lifecycles per domain, and RSA/ECDSA listener coexistence. Binding and removal
become explicit, auditable operations. Queries require joins, but the joins follow indexed tenant
and foreign-key columns and avoid JSON scans or in-memory pagination.

API process or browser termination no longer loses accepted issuance work. Client cancellation
stops observation only; retry, recovery, and terminal failure remain owned by the durable worker.

Operators can complete domain verification and certificate preparation before routing traffic to
an application. Creating or replacing an application route does not reissue, move, or duplicate
the certificate lifecycle.

This decision does not prove runtime certificate negotiation, public CA issuance, node
distribution, or public multi-vantage readiness. Those remain separate release gates.

## Verification

- PostgreSQL lifecycle, drift, foreign-key, uniqueness, typed same-algorithm conflict, ACME-only
  automatic renewal, and repository transaction tests.
- Service tests for an eight-SAN limit, hostname coverage, verification, and algorithm matching.
- Backend route/OpenAPI envelope, permission, pagination, and idempotency validation.
- Generated backend SDK regeneration and idempotency check.
- PC domain/certificate interaction, read-only permission, typecheck, test, and build checks.
- PC regression proving a verified unbound hostname can list and issue certificates without
  calling the application listener-binding API.

## Supersedes / Superseded By

This record replaces its prelaunch direct-column draft while retaining the stable ADR identifier.
It extends `ADR-20260726-admin-application-and-certificate-control-plane` and
`REQ-2026-0006-multi-certificate-sni.md`.
