# sdks/

This directory follows `SDK_WORKSPACE_GENERATION_SPEC.md`. The Flutter root
consumes generated app SDKs from the repository-level `sdks/` workspace; it must
not contain hand-edited generated output.

## What this root consumes

Only the two dependency families this root owns a surface for:

| Workspace | State |
| --- | --- |
| `sdks/sdkwork-deployments-app-sdk` | TypeScript-only — no Dart variant generated |
| `sdks/sdkwork-drive-app-sdk` | TypeScript-only — no Dart variant generated |

The `sdkwork-webserver-app-sdk` family was **retired** and no longer exists: with
it went this repository's entire `/app/v3/api` surface. The `deploy_app` entity is
owned by `sdkwork-deployments` (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7: every
normalized `(surface, method, path)` has one owner), so there is no second,
webserver-owned application surface left to consume.

## Why nothing is `pubspec`-linked yet

`FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` §6 expects capability packages to
consume dependency domains through generated Dart clients. Neither owned family
generates one: of the 61 `*-app-sdk` families in this workspace only six ship a
Flutter variant (`sdkwork-agents`, `sdkwork-audio`, `sdkwork-cloudrouter`,
`sdkwork-iam`, `sdkwork-im`, `sdkwork-voice`), and none of those six is owned by
this root.

`core` therefore declares `WebserverDeployAppCatalogPort`
(`lib/sdk/webserver_deploy_app_catalog_port.dart`) with no transport injected. An
unbound port reports `available == false` and its reader throws
`WebserverDeployAppCatalogUnavailableError` — it never resolves an empty page,
which would read as "this tenant has no applications". `specs/component.spec.json`
marks both families `pending-dart-artifact`.

Two caveats the reader must not mistake for completeness:

1. **Dart is nominal.** The type-slice trick that lets a TypeScript root hand a
   generated client straight through does not work here. When a Dart deployments
   SDK lands, a thin adapter must be written in `core`; the port marks an adapter
   seam, not a zero-cost passthrough.
2. **A generated package is still generated.** Generated transport legitimately
   depends on `package:http`; the no-raw-HTTP rule in `CODE_STYLE_SPEC.md`
   governs **authored** source under `lib/`, not generated transport. The contract
   test scans authored sources only.

## Surface URL contract

The deployment profiles materialize a **prefixed** surface URL
(`SDKWORK_WEBSERVER_APP_API_BASE_URL`), so `/app/v3/api` must be present exactly
once. `lib/sdk/app_api_surface_url.dart` owns exactly that boundary
(`webserverAppApiPrefix` + `normalizeWebserverAppApiBaseUrl`); it is deliberately
independent of any generated client, because a profile that accidentally pastes
the surface twice used to travel all the way into a request path before failing as
a 404. `lib/bootstrap/environment.dart` calls it while the runtime environment is
built, and `test/app_api_surface_url_test.dart` pins the accepted/rejected shapes.
