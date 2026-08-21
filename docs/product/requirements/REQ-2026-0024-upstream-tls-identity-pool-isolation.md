# REQ-2026-0024 Upstream TLS Identity And Pool Isolation

```yaml
id: REQ-2026-0024
title: Verify HTTPS upstream identity with bounded trust and client credentials
owner: sdkwork-webserver
status: accepted
source: security
problem: HTTPS proxy targets currently rely only on Reqwest defaults. Operators cannot select a private trust anchor, require a client identity, constrain TLS versions, or prove that invalid TLS material prevents a configuration generation from becoming active.
goals:
  - Add an optional per-upstream TLS policy with system, custom, or combined trust roots.
  - Support a bounded protected-file client certificate and private key pair for mutual TLS.
  - Enforce TLS 1.2/1.3 minimum and maximum versions without any certificate-verification bypass.
  - Build one independently configured Reqwest client and connection pool per upstream security context.
  - Validate and load all referenced TLS material before startup or atomic Watch publication.
non_goals:
  - Disabling certificate or hostname verification, arbitrary SNI override, plaintext fallback, or opportunistic TLS.
  - SPKI/certificate pinning, CRL, OCSP, certificate transparency enforcement, PKCS#11, KMS, Vault, or dynamic secret-provider integration.
  - Live reloading a TLS file whose configuration revision did not change.
  - Upstream retries, health checks, balancing, circuit breaking, or HTTP/3.
users:
  - application operators
  - security operators
  - private-service and zero-trust deployments
acceptance_criteria:
  - Upstreams expose optional tls.trustMode, caCertificateFiles, clientCertificateFile, clientPrivateKeyFile, minimumVersion, and maximumVersion fields with strict unknown-field rejection and finite collection/string bounds.
  - TLS policy on an HTTP target, incomplete client identity pairs, custom trust without CA files, CA files under system-only trust, invalid version ranges, unsafe paths, missing files, malformed PEM, empty CA bundles, oversized material, and mismatched client keys fail before generation publication.
  - System trust remains the safe default for HTTPS targets; custom trust disables built-in roots; system-and-custom retains built-in roots and adds the declared anchors.
  - Hostname verification and SNI always use the configured target hostname; no insecure-verification or server-name override is exposed.
  - A real private-CA HTTPS upstream fails under system trust and succeeds under custom trust; a wrong hostname or wrong CA fails closed.
  - A real mutual-TLS upstream rejects a client without the configured identity and succeeds with the configured client certificate/key.
  - TLS 1.3-only client policy rejects a TLS 1.2-only upstream, and a compatible policy succeeds.
  - A failed Watch candidate retains the active generation, while a valid changed TLS policy publishes one complete new client/pool generation.
non_functional_requirements:
  security: No certificate-verification bypass exists; trust anchors and private keys are bounded protected-file inputs and their bytes are never logged.
  privacy: Certificate contents, subject names, and private-key material are not persisted to logs, metrics, or diagnostics.
  performance: TLS files and certificate counts are bounded; clients and pools are built only at startup/reload and not per request.
  reliability: Each immutable generation owns complete upstream TLS clients; publication is atomic and failed candidates retain the previous generation.
affected_surfaces:
  - config
  - backend
  - runtime
  - security
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - CODE_STYLE_SPEC.md
    - NAMING_SPEC.md
    - RUST_CODE_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-webserver-core
    - crates/sdkwork-api-webserver-standalone-gateway
    - specs/sdkwork.webserver.config.schema.json
verification:
  - cargo test -p sdkwork-webserver-core
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - cargo clippy --workspace --all-targets -- -D warnings
  - pnpm.cmd verify
  - cargo fmt --all -- --check
  - git diff --check
```

## Design Decision

The policy is attached to one upstream, and each upstream already owns one Reqwest client. This makes the upstream id the isolation boundary for trust roots, client identity, DNS policy, timeouts, and pooled connections. Targets in one upstream must all use HTTPS when `tls` is present; plaintext and TLS targets cannot share a security context.

`system` uses Reqwest/Rustls built-in WebPKI roots and forbids custom CA files. `custom` disables built-in roots and requires at least one declared CA certificate. `system-and-custom` retains built-in roots and requires custom anchors. The target hostname remains the URL authority, Host source, SNI name, and certificate-verification name.

CA and client-identity files resolve beneath the configuration directory using the Core compiler's protected-file rules. Runtime loading uses the existing one-MiB-per-file TLS bound, limits the total number of parsed custom roots, combines a bounded client certificate/key pair only during immutable client construction, and drops source bytes after the Reqwest client is built. Watch reload constructs the entire candidate generation before ArcSwap publication.

## Acceptance

Accepted on 2026-07-16 for the declared upstream TLS trust, client identity, version, and pool-isolation boundary.

- The root Schema and Core Serde model expose optional `upstreams[].tls` with `trustMode`, bounded `caCertificateFiles`, paired `clientCertificateFile`/`clientPrivateKeyFile`, and TLS 1.2/1.3 `minimumVersion`/`maximumVersion`. Existing configurations remain compatible and HTTPS without an explicit policy keeps verified system WebPKI trust.
- Semantic validation rejects TLS on any HTTP target, system trust with custom roots, custom/combined trust without roots, incomplete client identity, reversed version ranges, parent/absolute/backslash/NUL paths, unknown fields, and more than eight CA files. Compilation resolves regular files canonically and rejects any protected TLS resource escaping the configuration directory.
- Runtime loading reuses the one-MiB-per-file TLS bound, distinguishes empty and invalid CA bundles without logging contents, limits parsed custom roots to 64, parses client identity through Reqwest/Rustls, and rejects mismatched certificate/private keys during client construction before listener activation.
- Every upstream owns one immutable Reqwest client/pool containing its resolver, SSRF policy, trust roots, client identity, TLS versions, and timeouts. Watch constructs a complete candidate generation before `ArcSwap` publication. A malformed CA candidate retained the active trusted pool; a valid switch to system trust produced `502`, proving the old custom-trust pool was not reused; switching back to custom trust produced `200`.
- Real private-CA HTTPS evidence proves system trust returns `502`, custom trust returns `200`, and a custom-trusted certificate with the wrong hostname still returns `502`. No insecure verification or SNI override exists.
- Real mTLS evidence proves the upstream rejects an anonymous client and succeeds only with the configured CA-signed client certificate/private key. Real protocol evidence proves TLS 1.3-only policy rejects a TLS 1.2-only upstream while TLS 1.2-only policy succeeds.
- Core verification passed 8 unit tests and 37 configuration contract tests. Gateway verification passed 38 unit tests, 52 data-plane integration tests, and 4 raw HTTP/1 connection tests.
- `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm.cmd verify`, example configuration validation, pagination, API envelope, API operation-pattern, route-collision, app-SDK import, repository-doc, formatting, and diff checks passed.

Acceptance is limited to this requirement. Certificate/SPKI pinning, CRL/OCSP, certificate-transparency enforcement, PKCS#11/KMS/Vault integration, automatic secret-file watching, live upstream credential rotation, upstream HTTP/2 policy, retries, health checks, balancing, and circuit breaking remain separate gates. PostgreSQL lifecycle execution remains ignored because `SDKWORK_DATABASE_TEST_POSTGRES_URL` is not configured. Backend OpenAPI encoding corruption and the unreviewed public `agent.sync` to `agent.retrieve` operation rename still require human review. Adaptive RSS/cgroup admission, 100,000-connection and 24-hour soak evidence, HA/failover/rolling upgrade, signed SBOM/provenance, and commercial operations remain unresolved.
