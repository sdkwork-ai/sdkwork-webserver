# sdks/

This directory follows `SDK_WORKSPACE_GENERATION_SPEC.md`. The HarmonyOS root
consumes the application-owned generated app SDK from the repository-level
`sdks/` workspace; it must not contain hand-edited generated output.

Current coverage of `sdks/sdkwork-webserver-app-sdk`:

| Target | Workspace | State |
| --- | --- | --- |
| typescript | `sdkwork-webserver-app-sdk-typescript` | materialized |
| flutter | `sdkwork-webserver-app-sdk-flutter` | materialized |
| dart | `sdkwork-webserver-app-sdk-dart` | declared, `generated/` empty |
| arkts | _none_ | not produced by the SDK generation chain yet |

Because no ArkTS target is produced, the HarmonyOS root reaches `/app/v3/api`
through a typed **port** declared in
`packages/sdkwork-webserver-harmony-mobile-core/src/main/ets/sdk/WebserverAppSdkClient.ets`.
The port owns base-URL normalization and the credential boundary; the concrete
transport is constructed only by `entry/src/main/ets/bootstrap/SdkClients.ets`.
Feature packages never perform raw HTTP and never hand-write auth headers
(`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6).
