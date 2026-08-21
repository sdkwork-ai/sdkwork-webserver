# ADR-20260726 Admin Application And Certificate Control Plane

Status: accepted
Requirement: REQ-2026-0061
Owner: sdkwork-webserver
Date: 2026-07-26
Specs: REQUIREMENTS_SPEC.md, ARCHITECTURE_DECISION_SPEC.md, API_SPEC.md, DATABASE_SPEC.md, SDK_SPEC.md, BACKEND_UI_SPEC.md, SECURITY_SPEC.md, DEPLOYMENT_SPEC.md

## Context

The current backend-admin surface owns Nginx, Web Node inventory, diagnostics, and audit, while application deployment and certificate operations exist only on app-api. The persisted `site_type` field classifies runtime technology (`static`, `SPA`, `Node`, `PHP`, `Python`, `other`); it cannot truthfully represent the new `WEB` and `API` application category. Certificate issuance, bounded automatic renewal, encrypted canonical persistence, Node Sync Manifest generation, atomic node activation, reload, and applied-version heartbeat already exist.

## Decision

- Extend the backend-api authority and generated Backend SDK with tenant-bound application, application-domain, application-deployment, certificate, and certificate-distribution operations.
- Add `application_type` to `web_site` with values `WEB` and `API`. Keep `site_type` unchanged as the runtime technology classification.
- Keep one canonical `web_certificate` row per certificate. Renewal updates that row in place, including the leaf fingerprint and encrypted private key; no per-server certificate rows are created.
- Continue distributing certificates through the authenticated bounded Node Sync Manifest. Its single tenant-wide `syncVersion` is derived from active Nginx revisions and canonical certificate fingerprints.
- Treat node certificate files as replaceable runtime projections. A Web Node reports the manifest version as applied only after atomic certificate/config activation and a successful real reload.
- Expose distribution status by comparing every paginated server observation with one desired tenant manifest version. This is operational evidence, not a second certificate state store.
- Add separate `pc-admin-applications` and `pc-admin-certificates` feature packages. They contribute resource services to the host composition and consume only the injected generated Backend SDK.

## Alternatives

- Reusing `site_type` for `WEB`/`API` was rejected because it would overwrite an existing, different dictionary and break runtime filtering.
- Storing `applicationType` only inside `runtime_config` was rejected because list filtering, validation, API generation, and database review would become implicit and weakly typed.
- Copying certificate records for each server was rejected because it creates multiple mutable authorities and makes renewal conflict resolution ambiguous.
- Pushing certificate files directly over SSH was rejected because it bypasses the authenticated Node SDK, bounded manifest, durable desired/observed state, atomic activation, and reload evidence.

## Consequences

The database baseline gains one additive constrained column while existing site records default to `WEB`. Backend OpenAPI and generated SDK artifacts expand additively. Cluster convergence remains eventually consistent and pull-based; an offline node remains visibly pending until it reconnects and applies the current manifest. A manifest version covers the full Nginx and certificate set, which intentionally proves atomic fleet configuration convergence rather than only one certificate file.

## Verification

- OpenAPI materialization and response-envelope, operation-pattern, route-collision, and pagination validators.
- Rust route/service/repository tests, including PostgreSQL repository parity and sync-version fingerprint changes.
- Backend SDK regeneration followed by PC admin typecheck, tests, and build.
- Contract tests proving one canonical certificate source, no per-server certificate table, and desired/applied manifest convergence fields.

## Supersedes / Superseded By

This decision extends, and does not supersede, `ADR-20260623-acme-certificate-authority`, `ADR-20260623-cert-distribution-topology`, `REQ-2026-0048`, `REQ-2026-0052`, and `REQ-2026-0054`.
