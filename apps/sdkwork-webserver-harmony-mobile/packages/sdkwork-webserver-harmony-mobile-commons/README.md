# SDKWork Web Server HarmonyOS Mobile Commons

Domain-neutral ArkUI primitives, design tokens, and locale helpers shared by the
HarmonyOS capability packages.

Layer role: `frontend-commons`. Shared packages must not import the application
app shell; the absence of an `oh-package.json5` dependency on `-shell` or
`-applications` is the enforcement, and `check-frontend-composition.mjs` plus
`tests/harmony-surface-contract.test.mjs` assert the import direction.
