# SDKWork Web Server HarmonyOS Mobile

Native HarmonyOS (ArkTS/ArkUI) client application root for SDKWork Web Server.

## Status

Architecture scaffold materialized against
`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md`. Package family, composition contracts,
host adapter boundaries, runtime profile matrix, and the first capability
(`applications`) are in place, together with a runnable static architecture
contract test.

## Blocking Prerequisites

The following are **not satisfied in this workspace** and are required before this
root can produce a signed HAP. They are declared, not worked around.

1. **HarmonyOS toolchain.** `ohpm`, `hvigor`, and the HarmonyOS SDK are not
   installed, so `ohpm install` and `hvigor assembleHap` cannot run. There is
   deliberately no `check:harmony-native` script and no `_sdkwork:*` facade entry:
   a command that cannot execute would be a false signal (`PNPM_SCRIPT_SPEC.md`).
2. **ArkTS SDK adaptation.** `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6 requires
   Harmony packages to consume `/app/v3/api` through generated ArkTS/TypeScript app
   SDK clients *adapted for the Harmony runtime*. **No ArkTS target exists for any
   app SDK family**, and the two families this root owns
   (`sdkwork-deployments-app-sdk`, `sdkwork-drive-app-sdk`) are TypeScript-only.
   `core` therefore declares the `deploy_app` port contract
   (`sdk/DeployAppSdkPort.ets`) and the adapter seam, and does not fabricate a
   vendored transport. A port built without an injected transport reports
   `available === false` and its readers reject with
   `WebserverDeployAppSdkUnavailableError`, so the screen shows its error state
   rather than claiming the tenant has no applications.
3. **Runtime config projection.** `entry/src/main/ets/bootstrap/Runtime.ets` fails
   fast until `config/app/runtime-env.<profileId>.json` is projected into the HAP
   as an ArkTS resource module. Falling back to a hand-written host is exactly the
   failure mode `SOURCE_CONFIG_SPEC.md` exists to prevent.
4. **Bundle signing profile.** `config/host/harmony.*.example.json` are secret-free
   templates; a real signing profile reference must come from DevEco Studio or CI
   secure storage.

## Package Family

| Package | Role | Layer role |
| --- | --- | --- |
| `packages/sdkwork-webserver-harmony-mobile-core` | runtime config, injected app SDK ports, token/session stores, route registry, host adapter contracts | frontend-core |
| `packages/sdkwork-webserver-harmony-mobile-commons` | domain-neutral ArkUI primitives, design tokens, locale helpers | frontend-commons |
| `packages/sdkwork-webserver-harmony-mobile-shell` | page stack, ordered navigation model, auth gate | frontend-shell |
| `packages/sdkwork-webserver-harmony-mobile-host` | typed HarmonyOS host adapters (secure storage, network status, lifecycle, device info) | frontend-host |
| `packages/sdkwork-webserver-harmony-mobile-applications` | tenant application catalog capability | frontend-feature |

`entry/` is the installable entry/composition module: entry ability, bootstrap,
provider assembly, route registry, SDK port construction, IAM runtime wiring, and
host adapter registration. It stays thin by contract — business pages belong to
capability packages.

## Configuration

Non-secret runtime config materializes as
`config/app/runtime-env.<deploymentProfile>.<environment>.json` and declares
matching `environment`, `deploymentProfile`, `profileId`, and
`runtimeTarget=harmony-native`. Host/platform metadata belongs to `config/host/`
and must stay secret-free.

The profile matrix mirrors the repository deployment index
(`../../../etc/sdkwork.deployment.config.json`): `standalone` ×
`development | test | staging | demo | production`.

## Verification

Static verification runs today, without the HarmonyOS toolchain.

Repository-scoped gates must be invoked **from the repository root**. Several of
them enumerate app roots via `listClientAppRoots(root)`; pointed at this
directory they still resolve, but they only see this one app and silently skip
the rest of the workspace, so the root is pinned below.

```bash
# from apps/sdkwork-webserver-harmony-mobile
node --test tests/harmony-surface-contract.test.mjs
node ../../../sdkwork-specs/tools/check-source-config-standard.mjs --root .
node ../../../sdkwork-specs/tools/check-app-manifest-standard.mjs --root .
node ../../../sdkwork-specs/tools/check-app-manifest-deployment-standard.mjs --root .

# from the repository root
node ../sdkwork-specs/tools/verify-repo.mjs --root .
node ../sdkwork-specs/tools/check-apps-directory-index.mjs --root .
node ../sdkwork-specs/tools/check-frontend-composition.mjs --root .
node ../sdkwork-specs/tools/check-component-port-bindings.mjs --root .
node ../sdkwork-specs/tools/check-permission-composition.mjs --root .
node ../sdkwork-specs/tools/check-i18n-standard.mjs --root .
node ../sdkwork-specs/tools/check-pagination.mjs --root .
node ../sdkwork-specs/tools/check-agent-workflow-standard.mjs --root .
```

### Gate coverage of `.ets`

ArkTS sources are not uniformly visible to the platform gates, so this root
compensates in `tests/harmony-surface-contract.test.mjs` (section 9) rather than
assuming coverage it does not have:

| Gate | Sees `.ets`? | Covered instead by |
| --- | --- | --- |
| `check-i18n-standard` | yes (`.ets` is a listed extension) | itself |
| `check-component-port-bindings` | yes (reads `component.spec.json`) | itself |
| `check-permission-composition` | yes (reads `component.spec.json`) | itself |
| `check-frontend-composition` | no (`.ts/.tsx/.js/.jsx` only) | contract test §9 import direction |
| `check-application-layering` | no (`.java/.js/.jsx/.ts/.tsx` only) | contract test §9 import direction |
| `check-pagination` | no (walks `apps/**` for `.ts/.tsx`) | contract test §11 |

`check-frontend-composition` additionally only discovers the `core` package here,
because `listPackages` requires `packages/<dir>/package.json` while the shared
packages carry `oh-package.json5`. Its role-dependency-direction rule therefore
has no capability package to reason about, which is why the contract test
asserts the same rule directly over `.ets` imports.

From the repository root, `pnpm check:client-native-roots` runs this contract
test, and `pnpm run _sdkwork:check` includes it.

The repository-level gates that also cover this root require `pnpm install` and
are listed in `../../../AGENTS.md`.
