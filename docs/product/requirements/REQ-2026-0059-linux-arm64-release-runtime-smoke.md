# REQ-2026-0059 Linux Arm64 Release Archive Runtime Smoke

```yaml
id: REQ-2026-0059
title: Build, identify, and smoke-test Linux arm64 Web Server release archives
owner: sdkwork-webserver
status: accepted
priority: P0
source: multi-architecture-commercial-release
problem: The PRD requires Linux x64 and arm64 server releases, but the producer, package manifest, workflow matrix, and runtime smoke were hard-coded to x64. Renaming an x64 archive as arm64 or accepting a mismatched host would create a non-runnable commercial artifact.
goals:
  - Bind release architecture to the artifact name, package manifest, workflow target, and runtime smoke.
  - Support exactly x64 and arm64 Linux server archives for standalone and cloud deployment profiles.
  - Fail before source recovery or build when the selected architecture does not match the Linux host architecture.
  - Build, validate, extract, identify, execute, serve HTTP/HTTPS traffic, terminate, and clean up both arm64 profile archives.
non_goals:
  - Native arm64 hardware performance, 100,000-connection capacity, allocator/OOM immunity, or 24-hour soak evidence.
  - OCI, deb, rpm, systemd, Kubernetes, upgrade, rollback, uninstall, SBOM, signing, or provenance completion.
  - Cross-compiling arm64 artifacts on an x64 process and claiming execution from compilation alone.
acceptance_criteria:
  - SDKWORK_PACKAGE_ARCHITECTURE or --architecture selects only x64 or arm64 and participates in the canonical artifact name.
  - Non-dry-run packaging and smoke require process.platform=linux and process.arch equal to the selected architecture.
  - The package manifest architecture must equal the selected artifact architecture and cannot be relabelled during validation.
  - The workflow contains standalone/cloud targets for both x64 and arm64 with canonical package ids, output globs, and architecture-appropriate runners.
  - Both real arm64 archives contain 17 exact entries and executable AArch64 gateway, Node Daemon, compatibility, and certificate-worker binaries.
  - Both arm64 packages pass extracted config validation, expected HTTP and HTTPS/SNI traffic, finite SIGTERM exit, and cleanup.
non_functional_requirements:
  security: Architecture input is a closed vocabulary, archive paths remain confined, manifest/checksum validation runs before extraction, and no credential or private key is packaged.
  reliability: Architecture mismatch fails before build and profile/architecture identities cannot silently fall back to x64.
  reproducibility: x64 and arm64 use isolated Cargo target directories and retain the same deterministic archive metadata and bounded inventory contract.
affected_surfaces:
  - release
  - deployment
  - workflow
  - linux-arm64
  - https
trace:
  specs:
    - NAMING_SPEC.md
    - DEPLOYMENT_SPEC.md
    - GITHUB_WORKFLOW_SPEC.md
    - RELEASE_SPEC.md
    - SUPPLY_CHAIN_SECURITY_SPEC.md
    - TEST_SPEC.md
  components:
    - scripts/webserver-release.mjs
    - scripts/webserver-release-smoke.mjs
    - sdkwork.workflow.json
    - tests/contract/deployment-profile-commands.contract.test.mjs
    - tests/contract/release-archive.contract.test.mjs
verification:
  - node --test tests/contract/deployment-profile-commands.contract.test.mjs tests/contract/release-archive.contract.test.mjs
  - node ../sdkwork-github-workflow/scripts/sdkwork-workflow.mjs validate --config sdkwork.workflow.json
  - Linux arm64 release package and validation for standalone and cloud
  - Linux arm64 extract/ELF identity/validate/start/readiness/HTTP/HTTPS/stop/cleanup smoke for standalone and cloud
  - pnpm verify
```

## Architecture Identity Boundary

The release architecture resolves from the explicit command option, then the workflow-provided
`SDKWORK_PACKAGE_ARCHITECTURE`, then the Node process architecture. Only `x64` and `arm64` are
accepted. Dry-runs may plan either architecture, but a real package or smoke requires a Linux process
whose `process.arch` is exactly the selected value. The same value controls the artifact basename
and `package.manifest.json#architecture`; validation selects the matching archive and rejects a
different manifest identity.

The four fixed workflow targets are x64/arm64 multiplied by standalone/cloud. Arm64 targets use the
SDKWork-compatible `ubuntu-24.04-arm` runner and produce canonical
`linux-arm64-<deployment-profile>-server-tar-gz` package ids.

## Verification Evidence

The official `docker.io/library/rust:1.92.0-bookworm` multi-architecture image reported
`host: aarch64-unknown-linux-gnu`; Node `22.22.0` reported `process.arch=arm64`. The build used an
arm64-only persistent Cargo target and produced:

- `sdkwork-web-linux-arm64-standalone-server-0.1.0.tar.gz`: `28,258,468` bytes,
  SHA-256 `45B00D377716F6DFA8E86ABFFFFE2EE874906A25D28D75D6F56EEAEBB527CC17`.
- `sdkwork-web-linux-arm64-cloud-server-0.1.0.tar.gz`: `28,258,467` bytes,
  SHA-256 `329B6D05C0E85CD6B468AF6EC275BC14016AD2FE2D092846A522770EC6F5DE6A`.

Both archives validated with 17 exact entries. `readelf` identified the extracted gateway, canonical
Node Daemon, and certificate worker as `Machine: AArch64`. Both profiles served the exact bounded
`release-smoke` body over HTTP and HTTPS with SNI `localhost`, exited with code 0 after `SIGTERM`,
and removed their temporary installation roots. No release-smoke container or temporary root
remained.

This execution used AArch64 userspace under Docker emulation on the current x64 workstation. It is
real arm64 binary execution and protocol smoke, but not native arm64 hardware performance, kernel,
capacity, thermal, NUMA, or long-duration evidence. Those claims require native production-class
arm64 runners and the separate load/soak gates.

## Remaining Release Gates

Linux archive runtime coverage now includes x64 and arm64 for standalone and cloud. Commercial
release still requires OCI/service packages, install/upgrade/rollback/uninstall evidence,
SBOM/signing/provenance/vulnerability and license gates, hard allocator/OOM proof, native-architecture
capacity and soak, and production HA convergence.
