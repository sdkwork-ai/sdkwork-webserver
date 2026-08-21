# REQ-2026-0058 Linux X64 Release Archive Runtime Smoke

```yaml
id: REQ-2026-0058
title: Build, install, and smoke-test the Linux x64 Web Server release archive
owner: sdkwork-webserver
status: accepted
priority: P0
source: release-runtime-evidence
problem: Windows dry-runs and synthetic tar fixtures prove archive selection and validation but do not prove that the published Linux x64 binaries can be built, extracted, executed, configured, served over HTTP and HTTPS, or stopped cleanly on the target operating system.
goals:
  - Build standalone and cloud Linux x64 server archives in a Linux x64 environment from the frozen workspace and isolated Cargo target directory.
  - Validate each real archive and its checksum before any runtime smoke.
  - Extract each archive into a confined temporary installation root and execute the packaged gateway validate operation.
  - Start a bounded self-contained HTTP/HTTPS data plane using generated test certificates, verify health traffic on both protocols, and stop it within a finite drain deadline.
  - Preserve the source workspace and host Cargo target by honoring an explicit CARGO_TARGET_DIR during release packaging.
non_goals:
  - Linux arm64, OCI/container, deb/rpm, Windows service, macOS, installer, upgrade, rollback, or uninstall parity.
  - PostgreSQL production topology, automatic election, fencing, multi-zone failover, 100,000-connection capacity, or 24-hour soak.
  - SBOM, signing, provenance, attestation, vulnerability, or license evidence beyond the separate supply-chain requirements.
users:
  - release engineer
  - Linux platform operator
  - CI release workflow
acceptance_criteria:
  - The release producer resolves CARGO_TARGET_DIR as an absolute or repository-relative Cargo target root and packages binaries from that root's release directory; the default remains repository target/release.
  - A Linux x64 runner with the frozen workspace builds both standalone and cloud archives, and each package operation validates the resulting archive and checksum before returning.
  - Each real archive passes explicit profile validation, extracts only inside a unique temporary root, and contains executable packaged gateway, canonical Node Daemon, v3 compatibility alias, and certificate-worker binaries.
  - The packaged example includes every required local static asset and validates successfully from the extracted package root.
  - The packaged gateway validates a confined Web Server config and starts a data-plane listener without initializing a database.
  - The smoke config serves a bounded fixed health response over HTTP and HTTPS with a generated certificate, and the test observes the expected status/body on both listeners.
  - The smoke process receives a finite termination signal, exits within the configured drain budget, and no package, temporary install root, or container is left behind.
  - Windows remains a supported dry-run host but fails closed before build for real Linux package operations.
non_functional_requirements:
  security: Build and extraction paths are confined, test certificates are ephemeral, no production token is packaged, and the smoke never enables an external management/database dependency.
  performance: Cargo output is isolated from the source target and archive validation remains bounded by REQ-2026-0057 limits.
  reliability: Runtime readiness, HTTP/HTTPS traffic, graceful stop, and cleanup are observed rather than inferred from process spawn success.
affected_surfaces:
  - release
  - deployment
  - server-runtime
  - https
trace:
  specs:
    - REQUIREMENTS_SPEC.md
    - DEPLOYMENT_SPEC.md
    - RELEASE_SPEC.md
    - GITHUB_WORKFLOW_SPEC.md
    - CONFIG_SPEC.md
    - SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - scripts/webserver-release.mjs
    - tests/contract/release-archive.contract.test.mjs
    - crates/sdkwork-api-webserver-standalone-gateway
    - etc/examples/sdkwork.webserver.config.json
verification:
  - node --test tests/contract/release-archive.contract.test.mjs tests/contract/deployment-profile-commands.contract.test.mjs
  - pnpm install --frozen-lockfile
  - Linux x64 release package and validation for standalone and cloud
  - Linux x64 extract/validate/start/readiness/HTTP/HTTPS/stop/cleanup smoke for standalone and cloud
  - pnpm verify
```

## Isolated Build Boundary

The release script accepts the native Cargo `CARGO_TARGET_DIR` contract. An absolute value is used as
provided; a relative value is resolved from the repository root; when absent, the repository's
`target/` directory remains the default. Cargo and the package producer therefore use one explicit
target root without copying or rewriting Cargo manifests.

The Linux smoke runs in a Linux x64 environment with the repository and sibling SDKWork sources
assembled according to the frozen workspace. It is evidence for this release lane, not proof that
every future runner image or mutable dependency ref is equivalent.

## Runtime Smoke Boundary

The smoke validates the extracted gateway first, then uses an ephemeral self-signed certificate and
a confined fixed-response configuration. It observes HTTP and HTTPS health responses, process exit,
and cleanup. It does not use the packaged example's external upstreams or production certificate
paths, and it does not claim management API, Node Sync, ACME, or database startup.

## Verification Evidence

Using the official `docker.io/library/rust:1.92.0-bookworm` image
(`sha256:e90e846de4124376164ddfbaab4b0774c7bdeef5e738866295e5a90a34a307a2`),
Rust `1.92.0`, Node `22.22.0`, pnpm `10.33.0`, and Linux x86_64, the frozen
workspace produced and validated both `0.1.0` archives. Each archive contains
17 exact entries, including `bin/sdkwork-web-node-daemon` and the retained
`bin/sdkwork-web-agent` compatibility alias. The standalone archive is
`29,505,599` bytes and the cloud archive is `29,505,596` bytes before their
checksum sidecars. Both extracted into confined temporary roots, validated the
packaged example, served the bounded `release-smoke` response over HTTP and
HTTPS with SNI `localhost`, exited cleanly after `SIGTERM`, and removed their
temporary installation roots. No package container or smoke temporary root was
left behind.

## Remaining Release Gates

This requirement closes Linux x64 archive execution and minimal HTTP/HTTPS process evidence only.
REQ-2026-0059 subsequently closes the equivalent arm64 functional archive/runtime lane. Neither
requirement closes container/service packaging, supply-chain signing/SBOM/provenance, upgrade or
rollback, hard allocator/OOM immunity, high-scale/soak capacity, or production HA convergence.
