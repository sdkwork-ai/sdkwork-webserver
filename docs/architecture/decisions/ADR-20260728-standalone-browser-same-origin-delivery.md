# ADR-20260728 Standalone Browser Same-Origin Delivery

Status: accepted
Requirement: REQ-2026-0065
Owner: sdkwork-webserver
Date: 2026-07-28
Specs: APPLICATION_GATEWAY_SPEC.md, APP_RUNTIME_TOPOLOGY_SPEC.md, CONFIG_SPEC.md, ENVIRONMENT_SPEC.md, DEPLOYMENT_SPEC.md, APP_PC_ARCHITECTURE_SPEC.md, TEST_SPEC.md

## Context

The standalone gateway now links the Web, IAM, and Drive API assemblies into one Rust process on `application.public-ingress`. That removed the undeclared dependency gateway on port 3900, but it did not make development browser traffic same-origin: Vite served the page on port 5182 while public runtime config instructed SDK clients to call port 3800 directly. Production packaging also omitted the PC `dist/` tree, so the standalone ingress could not serve the application shell.

API assembly placement and browser resource delivery are different decisions. Treating them as one `same-origin` concept allowed a configuration to pass validation even though the browser still crossed origins.

## Decision

- Topology profiles declare browser delivery separately from dependency API assembly. A standalone browser delivery always uses `originMode: "same-origin"` and `apiSurfaceId: "application.public-ingress"`.
- `standalone.development` keeps two internal processes for HMR: the Vite client process and the Rust application gateway. The browser opens only the Vite origin. Vite privately proxies canonical API and infrastructure paths to the application ingress and performs no path rewrite.
- Standalone public browser runtime config stores same-origin roots, not the gateway listener URL. Bootstrap resolves those roots against `window.location.origin` before constructing generated SDK clients.
- `standalone.production` has one browser-visible listener. The Rust gateway serves the packaged PC distribution after API and infrastructure routing, with navigation-only SPA fallback and reserved API-prefix protection.
- The production browser asset root is a typed runtime value. Startup fails before bind when the root, `index.html`, or `runtime-env.json` is missing; readiness continues to verify those immutable deployment inputs.
- The release archive stores the PC distribution under an application-owned share directory and includes every file, byte count, and digest in the package manifest.
- The standalone archive also stores the IAM and Drive database/module runtime assets under `share/sdkwork`. At process start, relative Web, IAM, Drive, and PC roots are resolved from the parent of the packaged `bin/` directory, so installed behavior does not depend on the service-manager working directory or compile-time source paths.
- The management PC shell does not use the configurable website-delivery data plane. That plane remains responsible for tenant website content and has different routing, provider, and listener contracts.

## Alternatives

- Direct browser calls from 5182 to 3800 with CORS were rejected because they expose an internal listener, create a second browser origin, and diverge from production behavior.
- Serving development assets from Rust was rejected because it would remove Vite HMR and slow the primary frontend workflow.
- Running Vite or a Node static server in production was rejected because it adds a second process and listener to the standalone deployment unit.
- Embedding frontend bytes into the Rust binary was rejected because it complicates cross-compilation and forces a Rust relink for every browser-only build; a manifest-hashed packaged asset tree preserves independent build ownership.
- Reusing tenant website static delivery was rejected because the management shell is an application-owned release asset, not tenant data-plane content.

## Consequences

Development still has two internal listeners, but only one origin is visible to browser JavaScript and network policy. Production has one application listener for the PC shell and composed APIs. CORS remains available for explicitly declared external clients, but normal standalone PC traffic does not depend on it. Release packaging becomes larger and must build both TypeScript and Rust artifacts plus dependency-owned runtime assets. Static route precedence, cache policy, and package-root resolution become part of gateway verification.

## Verification

- Runtime-plan tests compare browser-visible origin and private API target for both standalone profiles.
- PC tests reject absolute standalone Base URLs and resolve `/` against the supplied browser origin before SDK construction.
- Vite tests prove canonical proxy paths and topology-derived binds/targets.
- Rust tests prove static delivery, navigation fallback, reserved API 404 behavior, cache headers, startup failure, and readiness failure.
- Browser development verification records no direct port-3800 SDK request and no CORS error.
- Production-like verification starts the packaged binary outside its package CWD and serves `/`, `/runtime-env.json`, a nested SPA route, OpenAPI, and an IAM API request from one port.

## Supersedes / Superseded By

This decision narrows the browser-delivery interpretation of ADR-20260728 Embedded Standalone Dependency Assemblies. It does not supersede that ADR's dependency assembly ownership or one-process API composition decision.
