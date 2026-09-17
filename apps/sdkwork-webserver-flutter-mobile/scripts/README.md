# scripts/

Flutter build and release helper scripts belong here once the Flutter SDK and
Dart toolchain are installed. Static checks run from the repository root today:

```bash
node ../sdkwork-specs/tools/check-apps-directory-index.mjs --root .
node ../sdkwork-specs/tools/check-frontend-composition.mjs --root .
node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
node --test apps/sdkwork-webserver-flutter-mobile/tests/flutter-surface-contract.test.mjs
```

There is deliberately no `check:flutter-native` script and no `_sdkwork:*`
facade entry: `flutter pub get`, `flutter analyze`, and `flutter test` cannot run
until the Dart toolchain is installed, and a script that cannot execute would be
a false signal (`PNPM_SCRIPT_SPEC.md`). The runnable static contract test **is**
wired into the repository's `_sdkwork:check` through `check:client-native-roots`.
