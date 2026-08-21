# Webserver PC Source Configuration

`sdkwork.deployment.config.json` delegates runtime topology and deployment ownership to the enclosing Web Server application. The PC surface keeps only browser runtime sources and materialization metadata; it does not copy the parent topology profiles or start a second gateway.

Supported environments are `development`, `test`, `staging`, and `production`. Browser runtime sources use `browser/runtime-env.<deployment-profile>.<environment>.json` when a profile needs distinct values and otherwise fall back to `browser/runtime-env.<environment>.json`. Every selected source must declare the requested `deploymentProfile` and `environment`; `sdkwork.app.config.json` remains application identity metadata and is not a runtime-value source.

Browser profiles declare separate authenticated API authorities. `appApiBaseUrl` owns Web Server tenant operations, `backendApiBaseUrl` owns the lazy admin surface, `appbaseAppApiBaseUrl` owns IAM bootstrap, and `driveAppApiBaseUrl` owns application-package upload. The Console shares one bootstrap TokenManager across the Web and Drive App SDK clients.

Standalone browser profiles declare `browserOriginMode: "same-origin"` and use `/` as every explicit SDK Base URL. Browser bootstrap resolves those roots from `window.location.origin` before constructing SDK clients. During development, Vite reads its bind and private proxy target through the parent topology and forwards canonical API paths to `application.public-ingress` without rewriting them. Production standalone delivery serves the built renderer and the same API paths from that ingress. Internal listener ports never enter browser runtime config.

Cloud browser profiles declare `browserOriginMode: "cross-origin"` and keep explicit deployed application, backend-admin, Drive, and IAM origins. Both profile families declare `profileId` and `runtimeTarget: "browser"`; Vite mode remains a build adapter for the same selected profile rather than a runtime authority.

`messagingPcUrl` is a public cross-application navigation target for the independently deployed SDKWork Messaging PC notification center. Development standalone profiles point to its dedicated local browser origin; production profiles use the deployed Messaging PC URL. The Web Server portal must not construct a Messaging SDK client or own notification business state.

The schema authority is `CONFIG_SPEC.md`, `SOURCE_CONFIG_SPEC.md`, and `ENVIRONMENT_SPEC.md` in `sdkwork-specs`. Local overrides must use ignored local files or process-local environment input and must never modify tracked profiles. Secrets, access tokens, refresh tokens, API keys, certificate private keys, and bootstrap credentials are forbidden in browser runtime configuration; authenticated state comes from IAM and the shared TokenManager.

The tracked `../.env.development.example` declares a blank `SDKWORK_ACCESS_TOKEN=` for the private credential-entry bootstrap input. A live development value is process-local or written only to an ignored local override by the shared IAM tooling; it must not use a `VITE_*` key or be materialized into `public/runtime-env.json`.

`scripts/materialize-runtime-env.mjs --deployment-profile <standalone|cloud> --environment <environment>` materializes exactly one selected source to `public/runtime-env.json` before Vite starts or builds. Add `--check` to verify the tracked output without writing. `pnpm build:standalone` selects `standalone.production`; `pnpm build:cloud` selects `cloud.production`. Validate this root with:

```powershell
node ../../../sdkwork-specs/tools/check-source-config-standard.mjs --root .
```
