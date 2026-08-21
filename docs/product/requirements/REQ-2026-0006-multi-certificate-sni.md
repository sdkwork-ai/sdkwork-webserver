# REQ-2026-0006 Multi-Certificate SNI Selection

```yaml
id: REQ-2026-0006
title: Select a verified certificate for each HTTPS SNI name on one listener
owner: SDKWork maintainers
status: accepted
source: security
problem: A production HTTPS listener must serve multiple independently assigned domains without presenting an unrelated certificate, accepting ambiguous ownership, trusting configuration-only hostname claims, or scanning every certificate during each handshake.
goals:
  - Preserve the single-certificate certificateRef configuration while adding a mutually exclusive bounded certificateRefs collection.
  - Build one immutable certificate map before listener activation.
  - Select normalized exact DNS names before standards-compliant single-label wildcards.
  - Fail the TLS handshake for unknown or absent DNS SNI because this slice has no explicit default-certificate policy.
  - Reject duplicate normalized names and ambiguous certificate ownership before activation.
  - Parse every bounded PEM, require one private key, verify public/private key consistency, current leaf validity, and leaf subjectAltName coverage of declared names.
  - Keep certificate collection, path, content, and policy changes restart-only until atomic TLS-context rotation is implemented separately.
non_goals:
  - Live certificate rotation, persisted rollback, listener handoff, executable upgrade, or cluster certificate distribution.
  - An implicit or explicit default certificate for unknown or absent SNI.
  - Multiple RSA/ECDSA certificates owning the same name with client-signature-aware selection.
  - Public trust-chain construction, OCSP, CRL, certificate transparency, ACME, KMS, mTLS, or HSM integration.
  - Claiming commercial HTTPS or parent PRD completion from this bounded requirement.
users:
  - Platform operators
  - Site reliability engineers
  - Web application developers
acceptance_criteria:
  - JSON Schema and Serde accept exactly one of certificateRef or certificateRefs; certificateRefs contains 1 through 100 unique ids.
  - Every certificate reference resolves and the complete policy covers every DNS virtual-host name attached to its listener.
  - Duplicate normalized names within one certificate and duplicate Exact or Wildcard ownership across a policy fail validation.
  - One real HTTPS listener presents distinct certificates for distinct exact SNI names.
  - A wildcard certificate serves exactly one left DNS label, while an exact certificate wins when both Exact and Wildcard match.
  - Unknown SNI and clients without DNS SNI fail during the TLS handshake.
  - Empty, malformed, oversized, mismatched-key, expired/not-yet-valid, or SAN-mismatched material prevents listener activation.
  - Exact and single-label Wildcard selection use bounded indexed lookup and do not linearly scan all policy certificates per handshake.
non_functional_requirements:
  security: Certificate selection fails closed; private-key bytes remain outside configuration, logs, metrics, traces, and diagnostics.
  privacy: Certificate public names and redacted file errors are the only identity details exposed by this runtime slice.
  performance: A policy is bounded to 100 certificates and 100 names per certificate; startup parsing is bounded by 1 MiB per PEM file and handshake selection is indexed by normalized name.
  reliability: The listener opens only after the entire immutable certificate map validates; a failed entry cannot expose a partial map.
affected_surfaces:
  - backend
  - composition
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - NGINX_SPEC.md
    - TEST_SPEC.md
  components:
    - specs/sdkwork.webserver.config.schema.json
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
verification:
  - cargo test -p sdkwork-webserver-core --test webserver_config
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt -- --check
  - pnpm verify
```

Product authority: [PRD-https-and-certificates.md](../prd/PRD-https-and-certificates.md). Runtime design: [TECH-runtime-data-plane.md](../../architecture/tech/TECH-runtime-data-plane.md).

## Acceptance Evidence

Accepted on 2026-07-16 for the bounded standalone static certificate-map slice only.

- Core contract tests prove mutually exclusive singular/plural references, multi-certificate listener coverage, normalized duplicate rejection, and ambiguous SNI ownership rejection.
- Real TLS integration tests compare served peer-certificate DER for multiple Exact names, one Wildcard name, and an Exact-over-Wildcard overlap on the same listener.
- Negative tests prove fail-closed unknown/no-SNI behavior and pre-listener rejection for oversized PEM, empty/malformed PEM, expired/not-yet-valid leaf certificates, SAN mismatch, and private-key mismatch.
- `pnpm verify`, strict full-workspace Clippy, and `cargo fmt -- --check` are the acceptance gates for this revision.

This acceptance does not include zero-downtime certificate rotation or commercial HTTPS readiness. Those remain parent PRD blockers.
