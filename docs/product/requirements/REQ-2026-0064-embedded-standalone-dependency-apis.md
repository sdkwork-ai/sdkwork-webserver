# REQ-2026-0064 Embedded Standalone Dependency APIs

```yaml
id: REQ-2026-0064
title: Serve IAM and Drive dependency APIs from the Web Server standalone gateway process
owner: sdkwork-webserver
status: accepted
source: user
problem: The standalone PC runtime resolved IAM and Drive App SDKs to port 3900 even though the application uses API assembly composition. The Web Server gateway initialized IAM support but did not mount the IAM owner routes, so the browser depended on an independently started dependency gateway and failed with connection refused when that process was absent.
goals:
  - Link dependency-owned IAM and Drive Rust App API assembly contributions into the Web Server standalone gateway.
  - Serve application, IAM, and Drive browser APIs from the one application public ingress on port 3800.
  - Apply one gateway-owned Web Framework, request context, authorization, metrics, OpenAPI, readiness, and route-collision boundary to the composed route set.
  - Reject standalone server or browser platform-gateway URLs and reject dependency App SDK URLs outside the application ingress origin.
  - Make the same-origin assembly contract enforceable through component ports, topology validators, and tests.
non_goals:
  - Embedding cloud-only platform gateway behavior into the application repository.
  - Replacing generated dependency SDKs with application-owned HTTP clients.
  - Combining internal service/data-plane listeners with the browser application ingress.
  - Removing explicit platform gateway URLs from cloud profiles.
acceptance_criteria:
  - sdkwork-api-webserver-standalone-gateway directly depends on sdkwork-api-iam-assembly and sdkwork-api-drive-assembly and calls their App API contribution exports.
  - The composed standalone router validates every owner route manifest against owner OpenAPI, rejects route collisions, combines permission catalogs and readiness checks, and installs one process-wide Web Framework layer.
  - The standalone API target resolves to application.public-ingress, currently http://127.0.0.1:3800 in development; browser-visible SDK URLs follow REQ-2026-0065 and remain relative to the page origin.
  - Standalone topology env files contain no platform.api-gateway server or browser URL.
  - Runtime config parsing and materialization reject a standalone dependency SDK target outside application.public-ingress and separately enforce the browser-origin delivery contract from REQ-2026-0065.
  - The Web Server standalone gateway and Web Server owner assembly compile with the repository Rust toolchain.
  - Topology and component-port validators reject standalone platform URL keys and same-origin declarations without executable assembly ports and standalone coverage.
  - IAM reuses the installed canonical process database pool, while Drive's temporary AnyPool compatibility driver is identity-checked, budgeted, ADR-governed, and declared before canonical pool creation.
  - The standalone archive carries IAM and Drive owner runtime assets under stable package roots, and installed startup never depends on sibling source checkouts or build-machine paths.
  - Real standalone startup serves IAM and Drive App API inventory from port 3800 without requiring a dependency process on port 3900.
affected_surfaces:
  - rust-backend
  - api-assembly
  - standalone-gateway
  - pc-runtime-config
  - topology
  - iam
  - drive
trace:
  specs:
    - API_ASSEMBLY_SPEC.md
    - APPLICATION_GATEWAY_SPEC.md
    - APP_RUNTIME_TOPOLOGY_SPEC.md
    - CONFIG_SPEC.md
    - ENVIRONMENT_SPEC.md
    - IAM_SPEC.md
    - TEST_SPEC.md
  components:
    - crates/sdkwork-api-webserver-assembly
    - crates/sdkwork-api-webserver-standalone-gateway
    - apps/sdkwork-webserver-pc
    - ../sdkwork-iam/crates/sdkwork-api-iam-assembly
    - ../sdkwork-drive/crates/sdkwork-api-drive-assembly
verification:
  - cargo check -p sdkwork-api-webserver-assembly
  - cargo check -p sdkwork-api-webserver-standalone-gateway
  - pnpm --dir apps/sdkwork-webserver-pc test
  - pnpm --dir apps/sdkwork-webserver-pc typecheck
  - node ../sdkwork-specs/tools/check-topology-deployment-profiles.mjs --root .
  - node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
  - node ../sdkwork-specs/tools/check-process-shared-database-pool.mjs --root .
  - pnpm api:assembly:validate
  - pnpm topology:validate
  - node --test tests/contract/release-archive.contract.test.mjs
```

## Clarification

In this requirement, embedded means linked into the current Rust executable as a host-neutral owner assembly contribution. It does not mean launching the dependency repository's standalone gateway as a child process, proxying to its loopback port, or merely pointing a browser SDK at a second local origin.

The original browser-URL acceptance wording was narrowed by REQ-2026-0065: port 3800 is the private development proxy target and production application ingress, while development browser SDK requests use the Vite page origin.
