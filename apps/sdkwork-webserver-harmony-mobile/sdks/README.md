# sdks/

This directory follows `SDK_WORKSPACE_GENERATION_SPEC.md`. The HarmonyOS root
consumes generated app SDKs from the repository-level `sdks/` workspace; it must
not contain hand-edited generated output.

## What this root consumes

Only the two dependency families this root owns a surface for:

| Workspace | State |
| --- | --- |
| `sdks/sdkwork-deployments-app-sdk` | TypeScript-only — no ArkTS target produced |
| `sdks/sdkwork-drive-app-sdk` | TypeScript-only — no ArkTS target produced |

The `sdkwork-webserver-app-sdk` family was **retired** and no longer exists: with
it went this repository's entire `/app/v3/api` surface. The `deploy_app` entity is
owned by `sdkwork-deployments` (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7: every
normalized `(surface, method, path)` has one owner), so there is no second,
webserver-owned application surface left to consume.

## Why the port is declared but unbound

No ArkTS target is produced for any app SDK family, so the HarmonyOS root reaches
`/app/v3/api` through a typed **port** declared in
`packages/sdkwork-webserver-harmony-mobile-core/src/main/ets/sdk/DeployAppSdkPort.ets`.
The port owns base-URL normalization and the credential boundary; the concrete
transport is constructed only by `entry/src/main/ets/bootstrap/SdkClients.ets`.

A port built without an injected transport reports `available === false` and its
readers reject with `WebserverDeployAppSdkUnavailableError`. That is deliberate: a
port that resolved to an empty list would render "this tenant has no
applications", which is a lie about server state, whereas an unavailable transport
renders the error state the screen already owns. Feature packages never perform
raw HTTP, never hand-write auth headers, and never fill this gap with a vendored
transport copy (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6).

`tests/harmony-surface-contract.test.mjs` re-derives the mirrored `AppKind` /
`AppStatus` closed sets from the generated TypeScript deployments SDK, so the
mirror cannot drift unnoticed while no ArkTS target exists.
