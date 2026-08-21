# REQ-2026-0067 Professional Domain And Certificate Relationships

```yaml
id: REQ-2026-0067
title: Manage domain assets, multi-SAN certificates, and listener bindings independently
owner: sdkwork-webserver
status: in-progress
source: user
problem: Domain, application, certificate, and deployment lifecycles require professional many-to-many relationships and state-aware operations instead of direct storage identifiers or destructive rebinding.
goals:
  - Let one application route multiple verified hostnames.
  - Let one certificate cover up to eight unique SAN hostnames and let each hostname retain multiple certificate lifecycles.
  - Let authorized operators prepare, inspect, and issue certificates for verified hostnames before any application route exists.
  - Bind compatible certificate versions to a hostname listener without changing domain or certificate ownership.
  - Support one active RSA and one active ECDSA certificate on a listener with an explicit default.
  - Keep application deployment visibility linked to the application authority.
  - Provide permission-aware backend-admin operations without raw ids, raw HTTP, or private material exposure.
non_goals:
  - Treating a database verification status as external DNS or HTTP ownership proof.
  - Claiming public-CA, node-distribution, or same-name runtime negotiation evidence before their release gates pass.
  - Storing private keys, PEM bundles, provider credentials, or tokens in browser-visible contracts.
users:
  - tenant Web Server administrators
  - application deployment operators
  - certificate operators
acceptance_criteria:
  - A hostname exists independently of application routing and certificates.
  - An application may own multiple active hostname routes through web_site_binding.
  - The certificate issue command accepts 1..8 unique verified domainIds and an RSA or ECDSA key algorithm.
  - Backend certificate selection loads one bounded hostname page at a time, excludes unverified assets, preserves selected ids and labels across pages, and keeps verified unbound hostnames eligible.
  - Backend certificate listing and issuance are filtered by domainIds and do not require a web_site_binding or applicationId.
  - Certificate issuance and renewal persist a durable operation and return HTTP 202 asynchronous data; generated SDK operation retrieval, not the acceptance payload, supplies terminal status.
  - Certificate identifiers are ordered relational rows with foreign keys to both certificate and hostname.
  - Authorized operators can repeatedly issue certificates and can list certificates by domain without downloading the tenant inventory.
  - Authorized operators can list, bind, and unbind listener certificates for a bound hostname.
  - Binding rejects a certificate that does not cover the hostname, has no usable version, has a mismatched algorithm, or conflicts with an active algorithm binding; occupied algorithms return a standard conflict instead of an internal database error.
  - One active RSA and one active ECDSA listener binding may coexist; only one active listener binding is default.
  - Listener responses expose certificate name, SANs, issuer, fingerprint, expiry, key algorithm, certificate status, and binding status, but no protected material.
  - Unbinding an application route does not delete hostname or certificate lifecycle records.
  - Hostname deletion is blocked while a route or certificate identifier references it.
  - Lists are tenant filtered and store paginated; mutations are idempotent and audited.
  - Read-only certificate permission can inspect bindings but cannot issue, bind, or unbind.
  - Domain management always exposes the domain certificate inventory; application listener binding controls appear only when the hostname has an application route.
non_functional_requirements:
  security: Subject scope comes from WebRequestContext; private material remains behind secret references; ownership and coverage fail closed.
  privacy: APIs expose operational certificate metadata only and never private keys or provider secrets.
  performance: Domain filtering and pagination execute in PostgreSQL using indexed predicates; interactive selectors never aggregate all pages; SAN cardinality is bounded to eight at UI, API, service, ACME, and database layers.
  reliability: Deployment, certificate lifecycle, listener intent, distribution, and target observation remain distinct truthful states.
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
    - SUBJECT_ID_SPEC.md
    - API_SPEC.md
    - PAGINATION_SPEC.md
    - DATABASE_SPEC.md
    - WEB_BACKEND_SPEC.md
    - SDK_SPEC.md
    - BACKEND_UI_SPEC.md
    - DEPLOYMENT_SPEC.md
    - NGINX_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - database
    - crates/sdkwork-webserver-http-host
    - crates/sdkwork-webserver-contract
    - crates/sdkwork-intelligence-webserver-service
    - crates/sdkwork-intelligence-webserver-repository-sqlx
    - crates/sdkwork-routes-webserver-backend-api
    - sdks/sdkwork-web-backend-sdk
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-domains
    - apps/sdkwork-webserver-pc/packages/sdkwork-webserver-pc-admin-certificates
verification:
  - pnpm db:validate
  - cargo test -p sdkwork-intelligence-webserver-repository-sqlx --test repository_parity postgres_repository_transactions_tenants_idempotency_and_pagination_are_bounded -- --ignored --exact
  - cargo test -p sdkwork-intelligence-webserver-service
  - cargo test -p sdkwork-routes-webserver-backend-api
  - pnpm api:check
  - pnpm sdk:generate:check
  - node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
  - pnpm --dir apps/sdkwork-webserver-pc check
```

## Product Decision

The canonical relationship is:

```text
web_site 1 -> N web_site_binding N -> 1 web_domain
web_certificate N <-> N web_domain through web_certificate_identifier
web_certificate 1 -> N web_certificate_version
web_certificate 1 -> N web_certificate_operation
web_site_binding 1 -> N web_listener_certificate_binding
```

Binding is desired control-plane state, not proof of deployment or public readiness. Runtime and
commercial acceptance remain gated by immutable rollout and observation evidence.

Domain verification is the issuance prerequisite. An application route is a later deployment
concern and is required only when an operator binds a compatible certificate to that route's
listener. Application-scoped APIs may use an existing route as an authorization boundary, but
that does not make the application the certificate owner or a backend-admin issuance prerequisite.

## Change Control

- 2026-07-30: Stable requirement id assigned.
- 2026-07-31: Replaced the prelaunch direct domain/certificate column model with the implemented
  certificate-identifier, certificate-version, site-route, and listener-binding relationships;
  clarified that verified-domain issuance precedes and does not depend on application routing.
- 2026-07-31: Made issue and renew durable asynchronous operations with generated SDK status
  retrieval, bounded worker leases/retries, and fenced transactional finalization.
