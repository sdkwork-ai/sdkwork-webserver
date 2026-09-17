# SDKWork Web Server HarmonyOS Mobile Core

Runtime configuration, injected app SDK ports, token/session stores, route
registry, and host adapter contracts for the SDKWork Web Server HarmonyOS mobile
root.

Layer role: `frontend-core`. Every generated app SDK client is reached through a
ported interface declared here and constructed once by
`entry/src/main/ets/bootstrap/SdkClients.ets`; capability packages receive the
port and never build a transport
(`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6, `APP_SDK_INTEGRATION_SPEC.md` §2).

The `package.json` in this directory is **not** a pnpm workspace member. It
exists so repository-level composition gates can discover the core package of a
HarmonyOS root, whose real manifest is `oh-package.json5`.
