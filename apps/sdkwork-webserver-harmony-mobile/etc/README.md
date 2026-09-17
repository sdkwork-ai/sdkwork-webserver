# Component Deployment

This application surface shares the enclosing application deployment unit.
Deployment profiles are owned by `../../../etc/sdkwork.deployment.config.json`;
runtime process topology is owned by `../../../specs/topology.spec.json`.
Surface-local build and test commands stay in this application root.

The parent pointers above resolve against **this `etc/` directory**, not against
the application root — `SOURCE_CONFIG_SPEC.md` consumers do
`path.resolve(root, "etc", parentDeploymentConfig)`. Three levels up from
`apps/sdkwork-webserver-harmony-mobile/etc/` is the `sdkwork-webserver`
repository root, which is what `check-source-config-standard.mjs` derives from
the nearest `.git` entry.

`materialization.format` is `json` because a HarmonyOS HAP consumes a single
projected ArkTS/JSON resource rather than `--dart-define` values. Until the
hvigor resource-projection task exists, `entry/src/main/ets/bootstrap/Runtime.ets`
fails fast instead of falling back to a hand-written host.
