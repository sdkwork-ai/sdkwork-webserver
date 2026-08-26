# REQ-2026-0065 Standalone Browser Same-Origin Delivery

```yaml
id: REQ-2026-0065
title: Deliver the standalone PC shell and composed APIs from one browser-visible origin
owner: sdkwork-webserver
status: accepted
source: user
problem: The standalone Rust gateway correctly embeds Web, IAM, and Drive APIs on port 3800, but the development PC page is served from Vite on port 5182 and its public runtime config points SDK clients directly to port 3800. This still creates a cross-origin browser deployment, requires CORS, exposes an internal listener in browser config, and leaves the production standalone package without the PC build output or a static application-shell mount.
goals:
  - Make every standalone browser SDK request use the page origin while preserving canonical API paths.
  - Keep Vite HMR in development by proxying canonical API paths from the renderer origin to application.public-ingress without path rewriting.
  - Serve the production PC build, runtime-env.json, and SPA navigation fallback from the Rust application public ingress.
  - Package the standalone PC build with the Rust server and fail startup/readiness when required production browser assets are unavailable.
  - Package dependency-owned runtime assets and resolve installed application roots independently from process CWD or compile-time repository paths.
  - Make browser-origin delivery explicit in topology runtime plans and validators, independently from dependency API assembly runtimeMode.
non_goals:
  - Starting a second backend, dependency gateway, or platform cloud gateway for standalone.
  - Replacing generated SDKs with raw HTTP or synthetic proxy URL prefixes.
  - Requiring cloud browser deployments to use one origin when their declared surfaces are intentionally multi-host.
  - Reusing the public website-delivery data plane as the management PC shell owner.
acceptance_criteria:
  - standalone.development resolves a browser delivery with originMode same-origin, deliveryMode dev-server-proxy, and application.public-ingress as its private API target.
  - The standalone browser runtime artifact contains root-relative SDK Base URLs and no loopback API origin or backend listener port.
  - Vite derives its renderer bind and proxy target from the selected topology profile and forwards canonical app, backend, OpenAPI, health, readiness, liveness, and metrics paths without rewriting them.
  - Browser network requests for IAM session and runtime APIs use the renderer origin in development and no normal standalone request requires CORS.
  - standalone.production resolves a gateway-static browser delivery with an immutable build output, runtime static root, root mount, and /index.html SPA fallback.
  - The Rust gateway gives API and infrastructure routes precedence over static files, never turns an unknown API path into the SPA document, serves runtime-env.json with no-store, and reports missing production assets before accepting traffic.
  - The standalone release archive includes every PC distribution file in its bounded, hashed package manifest and production smoke verification proves both the shell and an API path on the same origin.
  - The standalone archive includes bounded IAM and Drive runtime assets, and packaged relative roots resolve from the installation root even when the process starts from another working directory.
  - Adding the same-origin management smoke preserves the existing packaged data-plane HTTP/HTTPS smoke coverage.
affected_surfaces:
  - pc-runtime-config
  - vite-development
  - rust-standalone-gateway
  - topology
  - release-package
  - standards
non_functional_requirements:
  security: Browser runtime config contains no token, secret, private endpoint, or cross-origin standalone backend address; reserved API paths never fall through to HTML.
  privacy: No additional user or tenant data is introduced.
  performance: Hashed static assets use immutable caching while index.html and runtime-env.json remain revalidation-safe.
  reliability: Production startup and readiness fail closed when the declared PC asset root or required files are missing.
trace:
  specs:
    - APPLICATION_GATEWAY_SPEC.md
    - APP_RUNTIME_TOPOLOGY_SPEC.md
    - CONFIG_SPEC.md
    - ENVIRONMENT_SPEC.md
    - DEPLOYMENT_SPEC.md
    - APP_PC_ARCHITECTURE_SPEC.md
    - APP_SDK_INTEGRATION_SPEC.md
    - TEST_SPEC.md
  components:
    - apps/sdkwork-webserver-pc
    - crates/sdkwork-api-webserver-standalone-gateway
    - specs/topology.spec.json
    - scripts/webserver-release.mjs
verification:
  - pnpm --dir apps/sdkwork-webserver-pc typecheck
  - pnpm --dir apps/sdkwork-webserver-pc test
  - pnpm --dir apps/sdkwork-webserver-pc build:prod
  - cargo test -p sdkwork-api-webserver-standalone-gateway
  - node ../sdkwork-specs/tools/check-topology-deployment-profiles.mjs --root .
  - node ../sdkwork-specs/tools/resolve-app-runtime-plan.mjs --root . --deployment-profile standalone --environment development --runtime-target browser --client-architecture pc-web --json
  - node ../sdkwork-specs/tools/resolve-app-runtime-plan.mjs --root . --deployment-profile standalone --environment production --runtime-target browser --client-architecture pc-web --json
```

## Clarification

Dependency `runtimeMode: "same-origin"` proves that a dependency-owned API assembly is mounted in the application gateway. `browserDeliveries[].originMode: "same-origin"` separately proves that the page and its SDK requests share one browser-visible origin. Both contracts are required for a standalone browser client.
