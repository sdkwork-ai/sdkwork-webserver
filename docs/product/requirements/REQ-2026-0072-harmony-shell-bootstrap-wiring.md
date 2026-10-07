# REQ-2026-0072 Harmony Mobile Shell Bootstrap Wiring

```yaml
id: REQ-2026-0072
title: Wire the Harmony mobile shell's runtime-env projection and mount the applications capability page
owner: sdkwork-webserver
status: accepted
source: audit-round-9
problem: The Harmony shell deliberately fails fast at bootstrap — `createWebserverHarmonyRuntimeConfig()` throws "runtime config projection is not wired yet", so environment resolution, SDK clients, the IAM runtime, host adapters, and routes are never composed and every launch renders a placeholder page. The applications capability ships a real screen (`ApplicationsCatalogPage.ets`) and a complete i18n fragment, but no page registry mounts them (`main_pages.json` carries only the placeholder `pages/Index`), so the one capability this product exposes on Harmony is unreachable.
goals:
  - Project `runtime-env.<profile>.<environment>.json` into HarmonyOS resources through an hvigor task so `createWebserverHarmonyRuntimeConfig()` reads a real deployment projection instead of throwing (the PC/H5/mini-program shells already materialize the same file per environment).
  - Register `ApplicationsCatalogPage` in `entry/src/main/resources/base/profile/main_pages.json` and route the shell to it after a successful bootstrap.
  - Move the placeholder `pages/Index` chrome copy into the resource/message catalogs so no hardcoded English strings remain on the mounted path.
  - Keep the fail-fast behavior for a genuinely missing projection: bootstrap still surfaces the failure through the existing status path instead of half-composing.
non_goals:
  - New Harmony capabilities beyond the applications list that the other shells already expose.
  - Flutter/mini-program/PC/H5 behavior; those shells are wired and verified.
  - Publishing to an app store or signing/provisioning automation.
users:
  - HarmonyOS end users
  - Release engineering
acceptance_criteria:
  - A Harmony build with the wired projection completes `bootstrapWebserverHarmonyApp` without the not-wired error and mounts `ApplicationsCatalogPage` as the first route.
  - The static native-root contract tests (`check:client-native-roots`, part of `_sdkwork:check`) assert the page registration and the runtime-env projection task, so the shell cannot silently regress to the placeholder.
  - No hardcoded user-facing English copy remains on the bootstrap-or-applications path; the zh-CN fragment engages for zh locales the same way the Flutter gate negotiates locale.
verification:
  - node ../sdkwork-specs/tools/check-client-native-roots.mjs --root . (or the equivalent root script `pnpm check:client-native-roots`)
  - A hvigor build of apps/sdkwork-webserver-harmony-mobile with the projection task wired, plus a device/emulator smoke of the bootstrap + applications list.
notes: |
  Tracked from audit round 9 (2026-10-08): the shell was authored as a
  fail-fast skeleton — correct for an unwired surface, unacceptable as a
  shipped state. Implementation is feature work (~a build task plus page
  registration), not polish, so it is tracked here rather than left as a
  silent stub. Until this lands, Harmony remains explicitly a placeholder
  surface and is not part of the production delivery matrix.
```
