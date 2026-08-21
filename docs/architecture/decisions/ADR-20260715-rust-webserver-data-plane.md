# ADR-20260715 Rust Web Server Data Plane

Status: accepted
Requirement: REQ-2026-0003
Owner: SDKWork maintainers
Date: 2026-07-15
Specs: ARCHITECTURE_DECISION_SPEC.md, RUST_CODE_SPEC.md, CONFIG_SPEC.md, SECURITY_SPEC.md, DEPLOYMENT_SPEC.md, NGINX_SPEC.md

## Context

The repository currently provides management APIs, persistence, certificate workflows, Nginx configuration materialization, and edge-agent synchronization. It does not provide a self-contained Rust request data plane capable of executing the application Web Server configuration defined by the product PRD.

The new data plane must coexist with existing management surfaces, preserve SDKWork crate layering, use proven protocol libraries, operate without database access on the request path, support HTTPS, and remain extensible toward the declared Nginx compatibility profile.

## Decision

- `specs/sdkwork.webserver.config.schema.json` is the machine authority for authored `sdkwork.webserver.app` configuration. Rust Serde models mirror it and semantic validation rejects cross-reference, conflict, security, and resource-bound violations that JSON Schema cannot express.
- `sdkwork-webserver-core` owns framework-independent configuration models, loading, canonicalization, semantic validation, host/path selection, and compiled immutable indexes. It does not bind sockets, execute HTTP, access databases, or own management APIs.
- `sdkwork-api-webserver-standalone-gateway` owns standalone request-plane composition: HTTP/HTTPS listeners, static serving, reverse proxying, process operations, graceful shutdown, and integration with the existing management router.
- The management plane and request data plane have distinct bootstrap paths. The data-plane-only path loads local configuration and does not initialize PostgreSQL.
- Axum/Hyper remain the HTTP service foundation. `axum-server` with Rustls owns HTTP/HTTPS listener integration and TLS handshakes. `tower-http` owns static file serving semantics. Reqwest with Rustls owns streaming upstream HTTP/HTTPS transport for the foundation slice.
- Request and response bodies are streamed. Limits, deadlines, pools, connections, configuration size, and background work have finite defaults and hard maxima.
- Application logical listeners compile under host policy into physical listeners. The first standalone slice accepts one app configuration and rejects conflicts rather than attempting unsafe merging.
- Nginx remains a compatibility/import/render/deployment target. It is not required for the Rust data plane to serve traffic.

## Alternatives

- Keep Nginx as the only request data plane: rejected because it does not meet the product goal of an independent Rust Web Server and leaves correctness dependent on external process orchestration.
- Implement raw HTTP and TLS directly on Tokio sockets: rejected because established Hyper/Rustls integrations provide safer protocol behavior and substantially stronger interoperability evidence.
- Add a new generically named runtime crate: rejected because the existing core and standalone-gateway roles already own the required responsibilities and `RUST_CODE_SPEC.md` forbids vague runtime ownership.
- Put configuration and routing models in the management service crate: rejected because the request data plane must operate without the business service or database and because framework-independent compilation is a separate responsibility.
- Use a fully buffered high-level HTTP client for proxying: rejected because unbounded or full-body buffering violates the PRD memory and streaming requirements.

## Consequences

- The existing gateway binary gains explicit operation modes and additional focused modules.
- New dependencies are limited to established HTTP/TLS/static/proxy/config validation libraries and are reviewed through Cargo metadata and supply-chain gates.
- Reqwest simplifies the first streaming proxy slice but advanced proxy behavior may later require a lower-level Hyper transport adapter; that change requires compatibility tests and an ADR update if it changes public runtime behavior.
- JSON Schema and Rust models can drift unless contract tests verify examples and schema/model compatibility on every change.
- Full Nginx matching, dynamic reload, cache, distributed policies, and cluster rollout remain later requirements and cannot be advertised as implemented by this foundation.

## Verification

- JSON Schema positive/negative fixtures and Rust semantic validation tests.
- Core routing precedence and bounded configuration tests.
- Gateway HTTP, HTTPS, static, proxy streaming, failure, and graceful-shutdown integration tests.
- Rust layering and backend composition validators.
- Dependency review, Cargo lockfile review, formatting, Clippy, and workspace tests.
- Nginx differential conformance is added incrementally only for directives marked implemented in the compatibility catalog.

## Supersedes / Superseded By

This decision does not supersede the certificate or Nginx deployment ADRs. Those records must be revised or superseded separately when their implementation-complete claims conflict with the new HTTPS and truthfulness gates.
