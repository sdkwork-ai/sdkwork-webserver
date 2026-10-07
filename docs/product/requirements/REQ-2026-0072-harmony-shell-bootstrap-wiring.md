# REQ-2026-0072 Harmony Mobile Shell Bootstrap Wiring

```yaml
id: REQ-2026-0072
title: Wire the Harmony mobile shell's runtime-env projection and mount the applications capability page
owner: sdkwork-webserver
status: implemented
source: audit-round-9
problem: The Harmony shell deliberately fails fast at bootstrap — `createWebserverHarmonyRuntimeConfig()` throws "runtime config projection is not wired yet", so environment resolution, SDK clients, the IAM runtime, host adapters, and routes are never composed and every launch renders a placeholder page. The applications capability ships a real screen (`ApplicationsCatalogPage.ets`) and a complete i18n fragment, but no page registry mounts them (`main_pages.json` carries only the placeholder `pages/Index`), so the one capability this product exposes on Harmony is unreachable.
goals:
  - Project `runtime-env.<profile>.<environment>.json` into the HAP through a build-step script so `createWebserverHarmonyRuntimeConfig()` reads a real deployment projection instead of throwing (the PC/H5/mini-program shells already materialize the same file per environment; the mini-program's `build-runtime.mjs` is the precedent — a Node projection step, not an hvigor task).
  - Mount `ApplicationsCatalogPage` from the root page (`pages/Index`) after a successful bootstrap, driven by the capability's view model.
  - Remove the placeholder chrome from the mounted path: the root page renders the capability screen, and the capability's shipped zh-CN catalog engages for zh locales the same way the Flutter gate negotiates locale.
  - Keep the fail-fast behavior for a genuinely missing projection: bootstrap still surfaces the failure through the existing status path with the fix in the message instead of half-composing.
non_goals:
  - New Harmony capabilities beyond the applications list that the other shells already expose.
  - Flutter/mini-program/PC/H5 behavior; those shells are wired and verified.
  - Publishing to an app store or signing/provisioning automation.
users:
  - HarmonyOS end users
  - Release engineering
acceptance_criteria:
  - A Harmony build with the projected rawfile completes `bootstrapWebserverHarmonyApp` and mounts `ApplicationsCatalogPage` as the first route.
  - The static native-root contract tests (`check:client-native-roots`, part of `_sdkwork:check`) assert the rawfile read, the projection script, and the fail-fast semantics, so the shell cannot silently regress to the placeholder.
  - The mounted path carries no hardcoded user-facing English copy; the zh-CN fragment engages for zh locales.
verification:
  - node --test apps/sdkwork-webserver-harmony-mobile/tests/harmony-surface-contract.test.mjs (wired through the root script `pnpm check:client-native-roots`, part of `_sdkwork:check`)
  - A hvigor/device build of apps/sdkwork-webserver-harmony-mobile after `node scripts/project-runtime.mjs --profile <id>`, plus an emulator smoke of the bootstrap + applications list — pending real-machine sign-off.
notes: |
  Tracked from audit round 9 (2026-10-08): the shell was authored as a
  fail-fast skeleton — correct for an unwired surface, unacceptable as a
  shipped state. Implemented in audit round 10 without a second
  host-writing fallback: `scripts/project-runtime.mjs --profile <id>`
  projects one committed `config/app/runtime-env.<profileId>.json` into the
  HAP rawfile (the rawfile is a git-ignored per-build artifact, so a build
  without a projection fails the launch with the fix in the message), the
  bootstrap reads that rawfile, and the root page mounts the applications
  capability view model with zh/en locale negotiation. The static contract
  test asserts the rawfile read, the projection script, and the fail-fast
  semantics (`check:client-native-roots`, part of `_sdkwork:check`).
  Remaining before release sign-off: one hvigor/device build smoke (the
  ArkTS edits compile against the real SDK), the same class of real-machine
  verification as the deploy.sh rollback drill.
```
