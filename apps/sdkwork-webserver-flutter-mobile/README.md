# SDKWork Web Server Flutter Mobile

Flutter (Dart) client application root for SDKWork Web Server.

## Status

Architecture scaffold materialized against
`FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md`. Package family, composition
contracts, the runtime profile matrix, the generated-Dart SDK boundary, and the
first capability (`applications`) are in place, together with a runnable static
architecture contract test.

## Blocking Prerequisites

The following are **not satisfied in this workspace** and are required before
this root can produce a signed Android app bundle or IPA. They are declared, not
worked around.

1. **Flutter/Dart toolchain.** `dart` and `flutter` are not on `PATH` and no
   SDK installation exists on this machine, so `flutter pub get`,
   `flutter analyze`, and `flutter test` cannot run. There is deliberately no
   `check:flutter-native` script and no `_sdkwork:*` facade entry: a command that
   cannot execute would be a false signal (`PNPM_SCRIPT_SPEC.md`).

   The `.dart_tool/` and `pubspec.lock` artifacts committed in sibling roots are
   **not** evidence of a local toolchain — their `package_config.json` points at
   `C:/Users/admin/AppData/Local/Pub/Cache`, a user account that does not exist
   on this machine. They arrived from another checkout.
2. **Dart SDK coverage gap.** `FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` §6
   expects capability packages to consume dependency domains through generated
   Dart clients. The workspace has **62** `*-app-sdk` families and only **six**
   ship a Flutter variant (`sdkwork-agents`, `sdkwork-cloudrouter`,
   `sdkwork-iam`, `sdkwork-im`, `sdkwork-mcp`, `sdkwork-webserver`) — and none of
   those six is owned by this root. The two families this root does own,
   `sdkwork-deployments-app-sdk` and `sdkwork-drive-app-sdk`, are TypeScript-only.

   Because the applications screen reads the **deployments** vocabulary
   (`deploy.apps.list`, `AppKind`, `AppStatus`) — the same authority the PC
   console mounts through `@sdkwork/deployments-pc-console-publishing` — `core`
   declares `WebserverDeployAppCatalogPort` with no binding. An unbound port
   reports `available == false` and its `reader` throws
   `WebserverDeployAppCatalogUnavailableError`; it never resolves an empty page,
   which would read as "this tenant has no applications". The screen renders the
   `applications.list.unavailable` state instead. See `sdks/README.md`.

   Note also that Dart is **nominal**: the type-slice trick that lets a
   TypeScript root hand a generated client straight through does not work here.
   When a Dart deployments SDK is generated, a thin adapter must be written in
   `core` — the port marks an adapter seam, not a zero-cost passthrough.
3. **Runtime config projection.** `lib/bootstrap/environment.dart` reads six
   `String.fromEnvironment` defines and fails fast on any invalid combination
   rather than falling back to a built-in host, because a silent default is how a
   staging build ends up talking to production (`SOURCE_CONFIG_SPEC.md`). Values
   arrive through `--dart-define-from-file=env/sdkwork.<profileId>.json`
   (`ENVIRONMENT_SPEC.md` §Flutter). An un-projected build throws at startup.
4. **Materialization is declared, not wired.** `etc/sdkwork.deployment.config.json`
   declares `materialization.command = "pnpm workflow:materialize-client-env"`,
   but this repository has neither an `etc/client-env.materialization.json`
   (the harness default path in `materialize-client-env.mjs`) nor a
   `workflow:materialize-client-env` script. The five `env/sdkwork.*.json` files
   are therefore hand-maintained against the repository deployment index today,
   and `tests/flutter-surface-contract.test.mjs` §5 is what keeps them honest.
5. **Signing profile.** `sdkwork.app.config.json` declares
   `signatureRequired: false` while `checksumRequired` and `sbomRequired` are
   `true`. That is a declared gap, not a decision: no signing profile reference
   exists yet, so the manifest must not claim one.

## Package Family

| Package | Role | Layer role |
| --- | --- | --- |
| `packages/sdkwork_webserver_flutter_mobile_core` | runtime environment resolution, generated SDK clients, the deploy-app catalog port, pagination narrowing, session/token stores, composition contracts | frontend-core |
| `packages/sdkwork_webserver_flutter_mobile_commons` | domain-neutral Flutter primitives, design tokens, screen state resolution, locale helpers | frontend-commons |
| `packages/sdkwork_webserver_flutter_mobile_shell` | route registry and placement contract, ordered navigation model, auth gate | frontend-shell |
| `packages/sdkwork_webserver_flutter_mobile_applications` | tenant application catalog capability | frontend-feature |

`lib/` is the installable entry/composition module: `main.dart`, the app shell,
the route-guard widget, and `lib/bootstrap/` (environment, SDK client
construction, IAM runtime wiring, host adapter registration, route assembly). It
stays thin by contract — business screens belong to capability packages.

The applications capability declares `sdkDependencies: []`. It reaches
`/app/v3/api` only through the `WebserverDeployAppCatalogReader` port injected by
`lib/bootstrap/sdk_clients.dart`, and `tests/flutter-surface-contract.test.mjs`
§9 asserts that direction on the `.dart` imports.

## Configuration

Non-secret runtime configuration materializes as
`env/sdkwork.<deploymentProfile>.<environment>.json`, keyed by `SDKWORK_*` plus a
`SDKWORK_WEBSERVER_*` alias set, and declares matching `deploymentProfile`,
`environment`, `profileId`, and `runtimeTarget=flutter-android`.

The profile matrix mirrors the repository deployment index
(`../../../etc/sdkwork.deployment.config.json`): `standalone` ×
`development | test | staging | demo | production`.

`SDKWORK_WEBSERVER_APP_API_BASE_URL` carries the `/app/v3/api` prefix; the client
resolver strips it before constructing the generated client, because that client
prepends the prefix itself (`ApiPaths.apiPrefix`). `config/app/` holds the
secret-free template; `env/*.local.json` is git-ignored.

## Verification

Static verification runs today, without the Flutter toolchain.

Repository-scoped gates must be invoked **from the repository root**. Several of
them enumerate app roots via `listClientAppRoots(root)`; pointed at this
directory they still resolve, but they only see this one app and silently skip
the rest of the workspace, so the root is pinned below.

```bash
# from apps/sdkwork-webserver-flutter-mobile
node --test tests/flutter-surface-contract.test.mjs
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

### Gate coverage of `.dart`

Dart sources are not uniformly visible to the platform gates. Nothing in this
workspace **executes** Dart, so the contract test is deliberately layered and
never claims "the Dart ran":

- **Static** — layout, package family, layer roles, root thinness, runtime profile
  matrix, secret-free config, app manifest, SDK boundary, and import direction.
- **Source-text contracts** — the shipped `.dart` decision expressions (the
  `hasMore` rule, the count clamp, the prefix strip, the identity drop) are pinned
  by text, so the semantics cannot drift while the gate stays green.
- **Scheduling assertions** — the three committed `flutter_test` suites are
  asserted to exist and to still name each behaviour, so a behaviour can never be
  quietly dropped from the suite that will run once the toolchain lands. The
  behavioural half itself lives in
  `packages/sdkwork_webserver_flutter_mobile_core/test/app_api_surface_url_test.dart`,
  `..._applications/test/applications_mapping_test.dart`, and
  `..._shell/test/route_contract_test.dart`.
- **A reference mirror** (pure JS) for the pagination narrowing, labelled as a
  mirror rather than as proof.

The gate was mutation-tested: 21 injected defects (env origin drift, a dropped
closed-set label, a missing locale key, a prefixed URL handed to the generated
client, an escaped relative import, a capability package importing the generated
SDK, a broken `pubspec.yaml` path dependency, an inverted `hasMore`, a raw HTTP
import, a stray `config/host/`, and so on) each turned it red **at the assertion
that defect should trip**, with the control green before and after and every
restored file SHA256-verified. The full table lives in
`.workbuddy/memory/2026-09-17.md`.

| Gate | Sees `.dart`? | Covered instead by |
| --- | --- | --- |
| `check-i18n-standard` | yes (`.dart` is in `SOURCE_EXTENSIONS` and `LITERAL_SCAN_EXTENSIONS`) | itself |
| `check-component-port-bindings` | yes (reads `component.spec.json`, not sources) | itself |
| `check-permission-composition` | yes (reads `component.spec.json`) | itself |
| `check-app-manifest-standard` / `-deployment-standard` | yes (reads `sdkwork.app.config.json` and `etc/`) | itself |
| `check-source-config-standard` | yes (config and env files) | itself |
| `check-frontend-composition` | no (`/\.(?:ts\|tsx\|js\|jsx)$/` only) | contract test §9 import direction |
| `check-application-layering` | no (`.java/.js/.jsx/.ts/.tsx` only) | contract test §9 import direction |
| `check-pagination` | partial — `.dart` under `apps/` reaches only `scanDocsAndTestsFile`; the client list rule (`TS_SMELLS`) walks `['.ts','.tsx']` | contract test §11 |
| `check-app-sdk-consumer-imports` | no (`isConsumerSourcePath` requires `\.(?:tsx?\|jsx?\|mjs\|cjs\|json)$`) | contract test §8 SDK boundary |

Two consequences worth stating plainly, because both are silent:

- `check-app-sdk-consumer-imports` was green against this root the moment it
  existed, and that green means nothing here — it cannot see a `.dart` import or
  a `pubspec.yaml` path dependency. §8 asserts the equivalent boundary directly.
- `check-frontend-composition` discovers only the `core` package, because
  `listPackages` requires `packages/<dir>/package.json` while Dart packages carry
  `pubspec.yaml`. Its role-dependency-direction rule therefore has no capability
  package to reason about, which is why the contract test asserts the same rule
  over `.dart` sources.

### Cross-root parity

Route identity is the one thing the client roots must agree on, and
`APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md` §7 makes that agreement normative
("aligned by route identity, **not by identical physical paths**"). §10 asserts
it in every direction the tree allows:

- **Against the mini program root** — the anchor this root was already pinned to:
  each id and permission hint must also appear in
  `apps/sdkwork-webserver-mini-program/.../routes/routeContributions.ts`.
- **Against the H5 root** — the H5 registry composes its id from four exported
  constants instead of spelling it out, so a substring search would prove
  nothing. §10 rebuilds the id from the same constants
  (`WEBSERVER_H5_ROUTE_SURFACE`, `WEBSERVER_H5_ROUTE_DOMAIN`,
  `APPLICATIONS_ROUTE`, `APPLICATIONS_SCREEN`) and compares, so a drift in any
  single segment fails the gate.
- **Route ids must be unique.** Every other field is pinned to `routeIds.length`,
  so a duplicated entry kept all of those equalities intact and passed; uniqueness
  is the one property column counts cannot express.

PC is deliberately not compared: the PC renderer declares no applications route —
it bridges the deployments console package — so there is no declaration on this
capability to compare against.

From the repository root, `pnpm check:client-native-roots` runs this contract
test alongside the HarmonyOS one, and `pnpm run _sdkwork:check` includes it.

The repository-level gates that also cover this root require `pnpm install` and
are listed in `../../../AGENTS.md`.
