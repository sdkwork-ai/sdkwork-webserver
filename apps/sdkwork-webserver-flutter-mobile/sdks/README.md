# sdks/

This directory follows `SDK_WORKSPACE_GENERATION_SPEC.md`. The Flutter root
consumes the application-owned generated app SDK from the repository-level
`sdks/` workspace; it must not contain hand-edited generated output.

Current coverage of `sdks/sdkwork-webserver-app-sdk`:

| Target | Workspace | State |
| --- | --- | --- |
| typescript | `sdkwork-webserver-app-sdk-typescript` | materialized |
| flutter | `sdkwork-webserver-app-sdk-flutter` | **materialized — consumed by this root** |
| arkts | _none_ | not produced by the SDK generation chain yet |

The Flutter variant is a real dependency, not a seam: `core/pubspec.yaml` path-
depends on
`sdks/sdkwork-webserver-app-sdk/sdkwork-webserver-app-sdk-flutter/generated/server-openapi`
and `lib/sdk/webserver_app_sdk_clients.dart` constructs the generated
`SdkworkAppClient` directly.

Two caveats the reader must not mistake for completeness:

1. **Workspace-wide Dart coverage is thin.** Of the 62 `*-app-sdk` families in
   this workspace, only six ship a Flutter variant (`sdkwork-agents`,
   `sdkwork-cloudrouter`, `sdkwork-iam`, `sdkwork-im`, `sdkwork-mcp`, and
   `sdkwork-webserver`). `sdkwork-deployments-app-sdk` and
   `sdkwork-drive-app-sdk` are TypeScript-only, which is why the deploy-app
   catalog port in `core` is declared but unbound — and why
   `specs/component.spec.json` marks both `pending-dart-artifact`.
2. **The generated package is generated.** It legitimately depends on
   `package:http`; the no-raw-HTTP rule in `CODE_STYLE_SPEC.md` governs
   **authored** source under `lib/`, not generated transport. The contract test
   scans authored sources only.

Because the generated client already prepends `/app/v3/api` itself
(`lib/src/api/paths.dart` → `ApiPaths.apiPrefix`), the surface prefix must be
stripped before constructing the client. `normalizeWebserverAppApiBaseUrl` and
`resolveWebserverAppTransportBaseUrl` in `core` own exactly that boundary.
