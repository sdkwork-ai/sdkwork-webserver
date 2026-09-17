# scripts/

HarmonyOS build and release helper scripts belong here once the DevEco Studio /
HarmonyOS SDK toolchain is available. Static checks run from the repository root
today:

```bash
node ../sdkwork-specs/tools/check-apps-directory-index.mjs --root .
node ../sdkwork-specs/tools/check-frontend-composition.mjs --root .
node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
node --test apps/sdkwork-webserver-harmony-mobile/tests/harmony-surface-contract.test.mjs
```

There is deliberately no `check:harmony-native` script and no `_sdkwork:*`
facade entry: no HarmonyOS build command can run until `ohpm` and `hvigor` are
installed, and a script that cannot execute would be a false signal
(`PNPM_SCRIPT_SPEC.md`). The runnable static contract test **is** wired into the
repository's `_sdkwork:check` through `check:client-native-roots`.
