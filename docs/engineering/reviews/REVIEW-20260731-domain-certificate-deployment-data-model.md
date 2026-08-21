# REVIEW-20260731 Domain, Certificate, And Deployment Data Model

Status: changes-requested
Owner: sdkwork-webserver
Date: 2026-07-31
Scope: sdkwork-webserver, sdkwork-deployments, sdkwork-iam
Specs: SUBJECT_ID_SPEC.md, DATABASE_SPEC.md, DATABASE_FRAMEWORK_SPEC.md, API_SPEC.md, PAGINATION_SPEC.md, SDK_SPEC.md, SECURITY_SPEC.md, DEPLOYMENT_SPEC.md, REQUIREMENTS_SPEC.md, ARCHITECTURE_DECISION_SPEC.md

## 1. Executive Result

The Web repository now has the correct relational shape for the requested product cardinalities.
The remaining release blockers are cross-repository ownership and runtime evidence, not another
domain table redesign.

| Area | Result | Decision |
| --- | --- | --- |
| Web domain model | Aligned | Keep as the mutable domain, route, certificate, and listener intent authority |
| Web backend/API/SDK/UI | Aligned for management workflows | Keep generated Backend SDK injection and state-aware operations |
| Deploy database | Conflicting duplicate authority | Retain rollout/distribution/observation evidence; retire mutable domain/certificate copies through a reviewed migration |
| IAM database subject ids | P0 standards conflict | Migrate subject and scope columns to positive `BIGINT` semantics with token/session repair |
| IAM application domains | Conflicting duplicate authority | Remove `primary_domain` and `domain_config_json`; reference Web public ids through an integration projection if required |
| Commercial TLS readiness | Not yet proven | Require runtime RSA/ECDSA negotiation, public issuance, distribution, and public observation evidence |

No sibling schema was modified by this review. Deploy and IAM changes affect data ownership,
tokens, and migrations and therefore require human review.

## 2. Industry Operating Model

A professional Web Server control plane separates the following facts:

1. Zone ownership: the registrable root domain and external DNS-provider identity.
2. Hostname ownership: apex, exact, or wildcard name plus challenge evidence.
3. Application route: listener/environment/path behavior that connects a hostname to an app.
4. Certificate lifecycle: requested identifiers, CA policy, renewals, and immutable versions.
5. Listener TLS intent: which compatible certificate versions may serve one hostname.
6. Deployment intent: immutable application artifact and desired revision.
7. Distribution evidence: which node received which certificate or runtime snapshot.
8. Observation evidence: what each node or external vantage actually activated and served.

Nginx, Apache, Envoy, HAProxy, Kubernetes ingress controllers, and public-cloud certificate
managers use equivalent separation even when their object names differ. Combining these states
causes destructive rebinds, stale deployment badges, unsafe certificate reuse, and impossible
rollout diagnosis.

## 3. Canonical Web Relationships

```text
web_root_domain 1 -> N web_domain

web_site 1 -> N web_site_binding N -> 1 web_domain
web_site 1 -> N web_deployment

web_certificate 1 -> N web_certificate_identifier N -> 1 web_domain
web_certificate 1 -> N web_certificate_version

web_site_binding 1 -> N web_listener_certificate_binding
web_listener_certificate_binding N -> 1 web_certificate
web_listener_certificate_binding N -> 0..1 web_certificate_version
```

This satisfies the explicit cardinalities:

- one application supports multiple hostnames through `web_site_binding`;
- one certificate supports up to eight SAN hostnames through
  `web_certificate_identifier`;
- one hostname supports multiple certificate lifecycles because several certificates may include
  the same domain;
- one listener supports multiple certificate bindings, with at most one active binding per RSA or
  ECDSA algorithm and one active default;
- hostname deployment visibility is projected through its bound site's deployment history.

## 4. Web Database Review

### 4.1 Strengths

- Every hostname has a required composite foreign key to its tenant-owned root Zone.
- Active root and hostname names are unique, preventing conflicting public ownership.
- Route state is normalized into `web_site_binding`, including environment, path, serve/redirect,
  primary, activation, and soft-delete lifecycle.
- Active route uniqueness on `(domain_id, environment, path_prefix)` prevents ambiguous traffic
  ownership while allowing explicit path-based composition.
- Certificates contain lifecycle policy, not private material or copied SAN JSON.
- Certificate identifiers use composite tenant foreign keys, unique hostname and position rules,
  a domain lookup index, and a database-enforced eight-SAN ceiling.
- Certificate versions preserve hashes, issuer/subject, validity, algorithm, protected material
  reference, and immutable version numbering.
- Listener certificate bindings enforce algorithm, priority, active-version readiness, one active
  algorithm binding, one default, and tenant-filtered certificate lookup.
- Root-domain, hostname, certificate, and deployment lists use store-level pagination and indexed
  predicates.

### 4.2 Web Gaps And Release Gates

| Priority | Gap | Required closure |
| --- | --- | --- |
| P0 | Same-name RSA/ECDSA selection is represented in control-plane state but not yet proven by the public TLS runtime | Interoperability tests for RSA-only, ECDSA-capable, TLS 1.2/1.3, SNI, default cert, reload, and rollback |
| P0 | Binding is desired state, not public serving evidence | Node distribution, activation observation, and external HTTPS fingerprint probes must converge before success |
| P0 | Public CA issuance and renewal are not commercial evidence | Real staging/production CA account, challenge, retry, expiry, renewal, and incident drills |
| P1 | DNS provider metadata does not implement record writes or propagation observation | Provider port, idempotent record reconciliation, bounded polling, and observed-value evidence |
| P1 | Wildcard and IDN policy needs explicit product coverage | Normalize IDNA at the service boundary; require DNS-01 for wildcard; test public-suffix and ownership constraints |
| P1 | Cross-repository authority is unresolved | Accept and execute the proposed ownership ADR without dual writes |

## 5. Product And Operation Review

The backend-admin interaction now follows the expected Zone-first model:

| Surface | Primary operations | State constraints |
| --- | --- | --- |
| Root-domain list | Search, page, create, open, add hostname, delete | Delete only when no hostname child exists |
| Root-domain detail | Add apex/subdomain, refresh, delete Zone | Stable route `/admin/root-domains/{id}` |
| Hostname row | Verify, bind app, unbind app, manage certificates, delete | Delete only when unbound and without certificate references |
| Certificate dialog | Inspect, issue RSA/ECDSA, select, bind, set priority/default, unbind | Issue only when verified; listener binding only when an application route exists |
| Deployment cell | Inspect latest status/version/time | Read-only projection from the bound application |

Read-only certificate users can inspect bindings. Write permissions gate issuance and mutation.
Destructive and runtime-affecting operations use confirmation and idempotency keys. Pagination
responses use request sequencing so stale responses cannot replace newer state.

## 6. Deploy Repository Review

The Deploy PostgreSQL baseline duplicates mutable Web authority through:

- `deploy_site`, `deploy_dns_zone`, `deploy_domain`, and `deploy_domain_verification`;
- `deploy_certificate`, `deploy_certificate_identifier`, and `deploy_certificate_version`;
- `deploy_site_binding`, `deploy_tls_policy`, and `deploy_listener_certificate_binding`;
- `deploy_deployment`.

The same repository also owns legitimate deployment evidence:

- site revisions and frozen targets;
- runtime assignments and target observations;
- certificate orders/challenges when Deploy is selected as the workflow executor;
- certificate distribution authorization and status;
- TLS runtime snapshots, assignments, and target observations.

Maintaining both sets as mutable authorities creates split-brain behavior: a Web mutation can
succeed while Deploy still routes or distributes an older duplicate row. The target boundary is:

| Owner | Mutable authority | References received from the other side |
| --- | --- | --- |
| sdkwork-web | Zones, hostnames, verification intent/evidence, sites, routes, certificate aggregates/versions, listener bindings, application deployment intent | Deploy rollout/observation summaries by immutable operation id |
| sdkwork-deploy | Immutable rollout plan, target set, assignment, distribution, and observation records | Web public UUIDs, version numbers, hashes, and desired snapshot ids |

Cross-service links must use immutable public ids and generated internal SDK/event contracts, not
cross-database foreign keys or shared tables. Deploy may snapshot the exact Web inputs used for a
rollout, but those snapshots are immutable evidence and never editable domain configuration.

## 7. IAM Repository Review

### 7.1 P0 Subject-ID Conflict

The IAM PostgreSQL baseline declares `iam_tenant.id`, `iam_user.id`, `iam_organization.id`, and
most `tenant_id`, `organization_id`, and `user_id` columns as `TEXT`. `SUBJECT_ID_SPEC.md` requires
positive snowflake `BIGINT` SQL subject ids, with organization `0` reserved for tenant-level
scope. Web repositories bind those values as native `i64`.

This mismatch is a production blocker. An authenticated legacy opaque subject must fail with HTTP
422 and business code 42201, not 403 or 500. Web now enforces that boundary, but IAM must repair
the source data and issue numeric claims.

### 7.2 Duplicate Application Domain Authority

`iam_tenant_application` stores `primary_domain` and `domain_config_json`, including a unique
domain index. Domain ownership, verification, routing, certificate coverage, and deployment are
Web concerns. IAM should own application registration, grants, memberships, and authorization,
not mutable domain configuration.

### 7.3 Baseline Duplication

The baseline repeats `CREATE TABLE IF NOT EXISTS iam_application_template`, its package table, and
`iam_tenant_application` sections. `IF NOT EXISTS` hides drift instead of making initialization
deterministic. Consolidate the baseline during the reviewed IAM migration.

## 8. Security Review

- Tenant, organization, and user scope must originate from `WebRequestContext`; clients never
  choose ambient scope.
- Invalid numeric subject projection returns 422/42201 with safe repair guidance.
- Domain verification must retain challenge hashes and bounded attempts; a UI action alone cannot
  mark ownership successful.
- Certificate private keys and PEM bundles remain referenced by protected secret/file handles and
  are never returned by API/SDK/UI.
- Listener binding validates hostname coverage, version state, key algorithm, and tenant ownership.
- Backend operations retain OpenAPI permission metadata; UI checks are ergonomic hints only.
- Audit must record actor, tenant, operation id, target public id, before/after version, request id,
  and result without logging tokens or private material.

## 9. Performance Review

The critical read paths are index-backed:

- roots: `(tenant_id, updated_at, id)`;
- hostnames: `(tenant_id, root_domain_id, updated_at, id)`;
- certificates by domain: `(tenant_id, domain_id, certificate_id)`;
- site routes: tenant/site or tenant/domain plus environment/status/update order;
- listener bindings by certificate: `(tenant_id, certificate_id, status)`;
- deployments: site/environment/time ordering.

Production query evidence must include PostgreSQL `EXPLAIN (ANALYZE, BUFFERS)` for representative
tenant sizes, keyset or bounded offset behavior, no N+1 certificate fetches, and no full tenant
download followed by browser slicing.

## 10. Reviewed Migration Sequence

1. Accept the cross-repository ownership ADR and freeze new mutable domain/certificate writes in
   IAM and Deploy.
2. Inventory and reconcile duplicates by normalized hostname, Web public UUID, certificate
   fingerprint, version, and tenant.
3. Migrate IAM subjects and dependent foreign keys to numeric snowflake ids; preserve reserved
   bootstrap ids, invalidate old sessions, and issue new tokens.
4. Add immutable Web reference/version fields to Deploy evidence records and backfill them.
5. Switch Deploy workers to generated Web internal reads/events and make duplicate mutable tables
   read-only.
6. Remove IAM domain fields and duplicate Deploy mutable authorities after reconciliation and a
   rollback window.
7. Run PostgreSQL lifecycle, parity, rollout, distribution, security, and public TLS evidence gates.

Dual writes are forbidden as the steady-state solution. Every destructive schema phase requires
backup/restore evidence, a rollback plan, and human approval.

## 11. Commercial Acceptance Gates

- PostgreSQL lifecycle, drift, migration, backup/restore, failover, and query-plan evidence passes.
- IAM numeric subject migration and session/token compatibility plan are accepted and tested.
- Deploy no longer accepts independent mutable domain/certificate configuration.
- Real ACME issuance/renewal and DNS/HTTP ownership evidence passes.
- RSA/ECDSA SNI negotiation and zero-downtime reload pass across supported clients.
- Certificate distribution and target observation prove exact fingerprint convergence.
- External multi-vantage probes prove public DNS, route, TLS chain, hostname, and deployment health.
- Audit, alerting, expiry incident, rollback, compromised-key, and node-divergence runbooks pass.

Until every gate has evidence, management feature completeness must not be described as commercial
TLS or production rollout completion.
