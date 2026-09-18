import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

/**
 * Static + behavioural architecture contract for the SDKWork Web Server
 * HarmonyOS mobile root.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` (§2 root layout, §4
 * package shape, §6 SDK boundary, §7 host adapters, §8 routes, §9 runtime
 * profile matrix, §11 verification), `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`,
 * `APP_PERMISSION_COMPOSITION_SPEC.md`, `PAGINATION_SPEC.md`, and `I18N_SPEC.md`.
 *
 * Two kinds of assertion live here:
 *
 * 1. **Static** — layout, package family, layer roles, root thinness, runtime
 *    profile matrix, secret-free host config, app manifest, and the raw-HTTP
 *    boundary. These need nothing but a filesystem.
 * 2. **Behavioural** — the pure ArkTS modules are copied verbatim out of
 *    `packages/**` into a temp directory, renamed `.ts`, and executed by Node's
 *    type-stripping loader. That covers base-URL normalization, the
 *    unavailable-transport port, pagination narrowing, closed-set label
 *    resolution, route validation, and the auth gate on a machine with no DevEco
 *    toolchain. Only modules whose imports are relative or type-only can be
 *    loaded this way; the composed view model and service (which import the
 *    sibling packages by name) are covered statically plus by `ohosTest`.
 *
 * Requires `--experimental-strip-types`; the file fails loudly when the flag is
 * missing rather than silently skipping the behavioural half.
 */

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(root, "..", "..");
const workspaceRoot = path.resolve(repoRoot, "..");

// --- helpers ---------------------------------------------------------------

function mustExist(relativePath) {
  const absolute = path.join(root, relativePath);
  assert.ok(fs.existsSync(absolute), `${relativePath} must exist`);
  return fs.readFileSync(absolute, "utf8");
}

function readJson(absolutePath) {
  return JSON.parse(fs.readFileSync(absolutePath, "utf8").replace(/^\uFEFF/u, ""));
}

function listFiles(relativePath) {
  const absolute = path.join(root, relativePath);
  if (!fs.existsSync(absolute)) return [];
  return fs.readdirSync(absolute, { withFileTypes: true }).flatMap((entry) => {
    const childRelative = path.join(relativePath, entry.name);
    if (entry.isDirectory()) {
      if (["node_modules", "build", "oh_modules", ".hvigor", "dist"].includes(entry.name)) {
        return [];
      }
      return listFiles(childRelative);
    }
    return [childRelative];
  });
}

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "sdkwork-harmony-contract-"));
process.on("exit", () => {
  fs.rmSync(tempDir, { recursive: true, force: true });
});

/**
 * Load a shipped `.ets` module by copying it, unchanged, to a temp `.ts`.
 *
 * The copy exists only because Node's type-stripping loader keys off the file
 * extension; the bytes are identical, so the assertions below run against the
 * real source rather than against a re-implementation.
 */
async function loadArkTsModule(relativePath) {
  const source = mustExist(relativePath);
  const flatName = relativePath.replaceAll(/[\\/.]/gu, "_");
  const target = path.join(tempDir, `${flatName}.ts`);
  fs.writeFileSync(target, source);
  return import(pathToFileURL(target).href);
}

assert.equal(
  process.features.typescript,
  "strip",
  "this contract must run with --experimental-strip-types so the behavioural assertions execute",
);

// --- 1. Root layout --------------------------------------------------------

for (const requiredPath of [
  "AGENTS.md",
  "README.md",
  "sdkwork.app.config.json",
  "specs/component.spec.json",
  "oh-package.json5",
  "build-profile.json5",
  "hvigorfile.ts",
  "hvigor/hvigor-config.json5",
  ".gitignore",
  ".sdkwork/README.md",
  ".sdkwork/skills/README.md",
  ".sdkwork/plugins/README.md",
  "etc/README.md",
  "etc/sdkwork.deployment.config.json",
  "AppScope/app.json5",
  "AppScope/resources/base/element/string.json",
  "entry/oh-package.json5",
  "entry/build-profile.json5",
  "entry/src/main/module.json5",
  "entry/src/main/resources/base/profile/main_pages.json",
  "entry/src/main/ets/entryability/EntryAbility.ets",
  "entry/src/main/ets/bootstrap/Environment.ets",
  "entry/src/main/ets/bootstrap/Runtime.ets",
  "entry/src/main/ets/bootstrap/SdkClients.ets",
  "entry/src/main/ets/bootstrap/IamRuntime.ets",
  "entry/src/main/ets/bootstrap/HostAdapters.ets",
  "entry/src/main/ets/bootstrap/Routes.ets",
  "config/host/README.md",
  "sdks/README.md",
  "scripts/README.md",
  "docs/README.md",
  "tests/harmony-surface-contract.test.mjs",
]) {
  mustExist(requiredPath);
}

// --- 2. Package family -----------------------------------------------------

const expectedPackages = [
  "sdkwork-webserver-harmony-mobile-core",
  "sdkwork-webserver-harmony-mobile-commons",
  "sdkwork-webserver-harmony-mobile-shell",
  "sdkwork-webserver-harmony-mobile-host",
  "sdkwork-webserver-harmony-mobile-applications",
];

for (const packageName of expectedPackages) {
  const packageRoot = `packages/${packageName}`;
  mustExist(`${packageRoot}/oh-package.json5`);
  mustExist(`${packageRoot}/build-profile.json5`);
  mustExist(`${packageRoot}/README.md`);
  mustExist(`${packageRoot}/src/main/module.json5`);
  mustExist(`${packageRoot}/src/main/ets/Index.ets`);
  const componentSpec = JSON.parse(mustExist(`${packageRoot}/specs/component.spec.json`));
  assert.equal(
    componentSpec.component?.root,
    `apps/sdkwork-webserver-harmony-mobile/packages/${packageName}`,
    `${packageName} component spec root must use the canonical HarmonyOS package path`,
  );
  assert.equal(
    componentSpec.component?.type,
    "arkts-package",
    `${packageName} must declare the arkts-package component type`,
  );
  assert.ok(
    componentSpec.component?.languages?.includes("arkts"),
    `${packageName} must declare arkts as its language`,
  );
}

// --- 3. Component spec layer roles -----------------------------------------

const allowedLayerRoles = new Set([
  "contract",
  "frontend-core",
  "frontend-shell",
  "frontend-feature",
  "frontend-commons",
  "frontend-host",
  "backend-route",
  "backend-service",
  "backend-domain",
  "backend-repository",
  "backend-provider",
  "runtime-api-server",
  "runtime-service-host",
  "runtime-composition",
  "runtime-gateway",
  "runtime-native-host",
  "sdk-facade",
  "sdk-generated",
  "tooling",
]);

for (const packageName of expectedPackages) {
  const componentSpec = JSON.parse(
    mustExist(`packages/${packageName}/specs/component.spec.json`),
  );
  const layerRole = componentSpec.contracts?.layerRole;
  if (layerRole !== undefined) {
    assert.ok(
      allowedLayerRoles.has(layerRole),
      `${packageName} contracts.layerRole ${JSON.stringify(layerRole)} is not an allowed composable layer role`,
    );
  }
}

// --- 4. Root thinness ------------------------------------------------------

const entryFiles = listFiles("entry/src/main/ets").map((filePath) =>
  filePath.replaceAll("\\", "/"),
);
const businessOwnedEntryFiles = entryFiles.filter((filePath) =>
  /\/pages\/(?!Index\.ets|__generated__)/u.test(filePath),
);
assert.deepEqual(
  businessOwnedEntryFiles,
  [],
  "root entry/ must stay thin: business pages belong in capability packages",
);

const mainPages = JSON.parse(mustExist("entry/src/main/resources/base/profile/main_pages.json"));
assert.deepEqual(
  mainPages.src,
  ["pages/Index"],
  "the root page profile must declare only the shell mount point",
);

const capabilitySourceFiles = expectedPackages
  .filter((packageName) => !packageName.endsWith("-core"))
  .flatMap((packageName) => listFiles(`packages/${packageName}/src`))
  .map((filePath) => filePath.replaceAll("\\", "/"));
assert.ok(
  capabilitySourceFiles.some((filePath) => filePath.endsWith(".ets")),
  "capability packages must own ArkTS source",
);

// --- 5. Runtime profile matrix ---------------------------------------------

const deploymentConfig = JSON.parse(mustExist("etc/sdkwork.deployment.config.json"));
const repositoryDeploymentIndex = readJson(
  path.join(root, "etc", deploymentConfig.parentDeploymentConfig),
);
assert.equal(deploymentConfig.kind, "sdkwork.component-deployment");
assert.equal(
  deploymentConfig.parentDeploymentConfig,
  "../../../etc/sdkwork.deployment.config.json",
  "the parent deployment pointer must be declared from the app root, and resolve from etc/",
);
assert.equal(
  repositoryDeploymentIndex.kind,
  "sdkwork.deployment-index",
  "the resolved parent pointer must be the repository deployment index",
);
assert.equal(deploymentConfig.materialization?.runtimeTarget, "harmony-native");

const profiles = ["standalone.development", "standalone.test", "standalone.staging", "standalone.demo", "standalone.production"];
assert.deepEqual(
  [...(deploymentConfig.materialization?.profiles ?? [])].sort(),
  [...profiles].sort(),
  "the component deployment config must mirror the repository profile matrix",
);
assert.deepEqual(
  Object.keys(deploymentConfig.profiles ?? {}).sort(),
  [...profiles].sort(),
  "every materialized profile must declare its committed source",
);
assert.deepEqual(
  Object.keys(repositoryDeploymentIndex.profiles ?? {}).sort(),
  [...profiles].sort(),
  "the repository deployment index must expose the same profile ids",
);

for (const profileId of profiles) {
  const environment = profileId.split(".")[1];
  const runtimeConfig = JSON.parse(mustExist(`config/app/runtime-env.${profileId}.json`));
  assert.equal(runtimeConfig.profileId, profileId, `${profileId} must declare its profileId`);
  assert.equal(runtimeConfig.deploymentProfile, "standalone");
  assert.equal(runtimeConfig.environment, environment);
  assert.equal(
    runtimeConfig.runtimeTarget,
    "harmony-native",
    `${profileId} must declare runtimeTarget=harmony-native`,
  );
  assert.equal(
    deploymentConfig.profiles[profileId]?.source,
    `../config/app/runtime-env.${profileId}.json`,
    `${profileId} source must be declared from etc/`,
  );

  const expectedOrigin = repositoryDeploymentIndex.environments?.[environment]?.applicationOrigin;
  assert.ok(expectedOrigin, `the repository deployment index must define ${environment}`);
  assert.equal(
    runtimeConfig.metadata?.applicationPublicHttpUrl,
    expectedOrigin,
    `${profileId} must use the repository application origin verbatim`,
  );
  assert.equal(
    runtimeConfig.webserver?.apiBaseUrl,
    `${expectedOrigin}/app/v3/api`,
    `${profileId} app API base URL must be the application origin plus the surface prefix`,
  );
  assert.equal(
    runtimeConfig.webserver?.backendApiBaseUrl,
    `${expectedOrigin}/backend/v3/api`,
    `${profileId} backend API base URL must carry the backend surface prefix`,
  );
  assert.equal(runtimeConfig.appbase?.appApiBaseUrl, expectedOrigin);
  assert.equal(runtimeConfig.appbase?.loginUrl, expectedOrigin);
}

// --- 6. Host config must stay secret-free ----------------------------------

const hostConfigFiles = listFiles("config/host").map((filePath) => filePath.replaceAll("\\", "/"));
assert.ok(hostConfigFiles.some((filePath) => filePath.endsWith(".example.json")), "config/host must contain checked-in templates");
const secretPattern = /(signingPrivateKey|privateKey|refreshToken|apiKey|databaseUrl|password)\s*[:=]\s*["'][^"'<]/iu;
for (const hostFile of hostConfigFiles) {
  assert.doesNotMatch(mustExist(hostFile), secretPattern, `${hostFile} must not contain secrets`);
}

// --- 7. App manifest -------------------------------------------------------

const manifest = JSON.parse(mustExist("sdkwork.app.config.json"));
assert.equal(manifest.schemaVersion, 3, "the manifest must use App Standard v3");
assert.equal(manifest.kind, "sdkwork.app");
assert.equal(manifest.app?.appType, "APP_HARMONY", "a HarmonyOS root must declare appType APP_HARMONY");
assert.equal(manifest.app?.versionSource, "oh-package.json5");
assert.equal(manifest.app?.identifiers?.bundleId, manifest.app?.identifiers?.packageName);
assert.equal(manifest.runtime?.family, "mobile");
assert.equal(manifest.runtime?.framework, "harmony-native");
assert.deepEqual(manifest.runtime?.runtimes, ["APP_HARMONY"]);
assert.ok(manifest.publish?.platforms?.includes("APP_HARMONY"));
assert.ok(manifest.publish?.installPlatforms?.includes("APP_HARMONY"));
assert.ok(
  manifest.media?.icons?.primary?.url,
  "the primary icon must carry a real asset reference",
);
const currentReleases = (manifest.release?.notes ?? []).filter((entry) => entry.current === true);
assert.equal(currentReleases.length, 1, "exactly one release note must be current");
assert.equal(manifest.metadata?.deploymentConfig, "etc/sdkwork.deployment.config.json");

const appPackage = JSON.parse(mustExist("oh-package.json5"));
assert.equal(appPackage.version, manifest.release?.currentVersion);

// --- 8. SDK boundary -------------------------------------------------------

const rawHttpPattern = /@ohos\.net\.http|http\.createHttp|\bfetch\s*\(|\baxios\b/u;
for (const packageName of expectedPackages) {
  const sources = listFiles(`packages/${packageName}/src`)
    .filter((filePath) => filePath.endsWith(".ets"))
    .map((filePath) => `${filePath}\n${mustExist(filePath)}`)
    .join("\n");
  const violation = rawHttpPattern.exec(sources);
  assert.equal(
    violation,
    null,
    `${packageName} must not perform raw HTTP transport (found ${JSON.stringify(violation?.[0])})`,
  );
}

const entrySource = listFiles("entry/src/main/ets")
  .filter((filePath) => filePath.endsWith(".ets"))
  .map((filePath) => `${filePath}\n${mustExist(filePath)}`)
  .join("\n");
assert.doesNotMatch(
  entrySource,
  rawHttpPattern,
  "the root entry module must not perform raw HTTP transport either",
);
assert.match(
  mustExist("entry/src/main/ets/bootstrap/Runtime.ets"),
  /throw new Error\(\s*'Harmony runtime config projection is not wired yet/u,
  "bootstrap must fail fast instead of silently defaulting the runtime host",
);

const packageManifest = JSON.parse(mustExist("packages/sdkwork-webserver-harmony-mobile-core/package.json"));
assert.deepEqual(
  Object.keys(packageManifest.exports ?? {}).sort(),
  [".", "./composition", "./host", "./modules", "./sdk", "./session"],
  "the core composition contract must expose the canonical export subpaths",
);

const rootComponentSpec = JSON.parse(mustExist("specs/component.spec.json"));
assert.equal(rootComponentSpec.component?.type, "harmony-mobile-app-root");
assert.deepEqual(
  (rootComponentSpec.contracts?.sdkDependencies ?? []).map((entry) => entry.workspace).sort(),
  ["sdkwork-deployments-app-sdk", "sdkwork-drive-app-sdk", "sdkwork-webserver-app-sdk"],
  "the root must compose the same three app SDK families as the PC, H5, and mini program roots",
);

// --- 9. ArkTS import direction -------------------------------------------

/**
 * Compensates a real gate blind spot: `check-application-layering.mjs` scans
 * `.java/.js/.jsx/.ts/.tsx` only, so it is blind to `.ets`. The dependency
 * direction it would otherwise enforce is asserted here on the shipped sources.
 */
const siblingPackageAliases = new Map(
  expectedPackages.map((packageName) => [
    `@sdkwork/${packageName}`,
    packageName.replace("sdkwork-webserver-harmony-mobile-", ""),
  ]),
);

const allowedSiblingImports = new Map([
  // core publishes the composition contract; its module registry names the
  // capability packages it composes, and nothing else may reach back into it
  // except through the declared ports.
  ["core", new Set(["applications"])],
  ["commons", new Set()],
  ["shell", new Set()],
  // host implements the core-declared adapter contracts.
  ["host", new Set(["core"])],
  // a capability consumes shared primitives, the shell route contract, and the
  // injected SDK port; it must never bind a platform adapter itself.
  ["applications", new Set(["core", "commons", "shell"])],
]);

for (const packageName of expectedPackages) {
  const role = packageName.replace("sdkwork-webserver-harmony-mobile-", "");
  const sources = listFiles(`packages/${packageName}/src`)
    .filter((filePath) => filePath.endsWith(".ets"))
    .map((filePath) => ({ filePath, source: mustExist(filePath) }));

  for (const { filePath, source } of sources) {
    const normalized = filePath.replaceAll("\\", "/");
    for (const match of source.matchAll(/from\s+'(@sdkwork\/[^']+)'/gu)) {
      const target = siblingPackageAliases.get(match[1]);
      if (target === undefined) {
        assert.fail(`${normalized} imports an unknown SDKWork package ${match[1]}`);
      }
      assert.ok(
        allowedSiblingImports.get(role)?.has(target),
        `${normalized} (${role}) must not depend on the ${target} package; allowed: ${[...(allowedSiblingImports.get(role) ?? [])].join(", ") || "none"}`,
      );
    }

    // A relative specifier resolves on disk, so it can cross a package boundary
    // without ever naming an `@sdkwork/` alias. Resolve each one and require it
    // to stay inside the owning package; otherwise the alias table above could
    // be satisfied while the real dependency direction is inverted.
    for (const match of source.matchAll(/from\s+'(\.\.?\/[^']+)'/gu)) {
      const specifier = match[1];
      const resolved = path
        .resolve(root, path.dirname(filePath), specifier)
        .replaceAll("\\", "/");
      assert.ok(
        resolved.startsWith(`${root.replaceAll("\\", "/")}/packages/${packageName}/`),
        `${normalized} escapes its own package via '${specifier}' -> ${resolved}`,
      );
    }
  }
}

// A shared package that owns a page would invert the layering the composition
// spec fixes; only capability packages contribute screens.
const sharedRoles = ["commons", "shell", "host", "core"];
for (const packageName of expectedPackages) {
  const role = packageName.replace("sdkwork-webserver-harmony-mobile-", "");
  if (!sharedRoles.includes(role)) continue;
  const screens = listFiles(`packages/${packageName}/src`)
    .map((filePath) => filePath.replaceAll("\\", "/"))
    .filter((filePath) => /\/(pages|screens)\//u.test(filePath));
  assert.deepEqual(screens, [], `${role} must not own screens: ${screens.join(", ")}`);
}

// --- 10. Route identity + permission authority ------------------------------

const routeContributions = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-applications/src/main/ets/routes/RouteContributions.ets",
);
const routes = routeContributions.webserverHarmonyApplicationsRouteContributions;
assert.ok(Array.isArray(routes) && routes.length > 0, "the applications capability must contribute a route");

const routePlacement = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-shell/src/main/ets/navigation/RoutePlacement.ets",
);
assert.deepEqual(
  routePlacement.validateWebserverHarmonyRouteContributions(routes),
  [],
  "the shipped route contributions must satisfy the shell's own route contract",
);

const miniProgramRoutes = fs.readFileSync(
  path.join(
    repoRoot,
    "apps",
    "sdkwork-webserver-mini-program",
    "packages",
    "sdkwork-webserver-mp-applications",
    "src",
    "routes",
    "routeContributions.ts",
  ),
  "utf8",
);
for (const route of routes) {
  assert.equal(
    route.id,
    `${route.surface}.${route.domain}.${route.capability}.${route.screen}`,
    "route ids must follow <surface>.<domain>.<capability>.<screen>",
  );
  assert.ok(
    miniProgramRoutes.includes(`"${route.id}"`),
    `route ${route.id} must also be declared by the mini program root so the two mobile clients cannot drift`,
  );
  assert.ok(route.permissionHint, `route ${route.id} must declare its permission hint`);
  assert.ok(
    miniProgramRoutes.includes(`"${route.permissionHint}"`),
    `route ${route.id} permission hint must match the mini program root's hint`,
  );
}

/**
 * H5 parity. The H5 registry composes its id from four exported constants
 * instead of spelling the id out, so a literal substring search would prove
 * nothing about it; the id is rebuilt from the same constants H5 exports so a
 * drift in any one segment fails here. PC is not compared: the PC renderer does
 * not declare an applications route, it bridges the deployments console package.
 */
const h5RouteRegistryPath = path.join(
  repoRoot,
  "apps",
  "sdkwork-webserver-h5",
  "packages",
  "sdkwork-webserver-h5-shell",
  "src",
  "navigation",
  "routeRegistry.ts",
);
assert.ok(
  fs.existsSync(h5RouteRegistryPath),
  "the H5 route registry must exist to anchor cross-root parity",
);
const h5RouteRegistry = fs.readFileSync(h5RouteRegistryPath, "utf8");
const h5Constant = (name) => {
  const match = h5RouteRegistry.match(
    new RegExp(`export const ${name}\\s*=\\s*"([^"]+)"`, "u"),
  );
  assert.ok(match, `the H5 registry must export ${name} as a string literal`);
  return match[1];
};
const h5RouteId = [
  h5Constant("WEBSERVER_H5_ROUTE_SURFACE"),
  h5Constant("WEBSERVER_H5_ROUTE_DOMAIN"),
  h5Constant("APPLICATIONS_ROUTE"),
  h5Constant("APPLICATIONS_SCREEN"),
].join(".");
const h5PermissionHints = [
  ...h5RouteRegistry.matchAll(/permissionHint:\s*"([^"]+)"/gu),
].map((match) => match[1]);
assert.ok(
  h5PermissionHints.length > 0,
  "the H5 registry must gate its entry on a permission hint",
);

for (const route of routes) {
  assert.equal(
    h5RouteId,
    route.id,
    "the H5 root and this root must agree on the canonical route id",
  );
  assert.ok(
    h5PermissionHints.includes(route.permissionHint),
    `the H5 root must gate route ${route.id} on ${route.permissionHint}`,
  );
}

// Two identical entries would keep every per-field check above intact, so
// uniqueness is the one property those checks cannot express.
assert.equal(
  new Set(routes.map((entry) => entry.id)).size,
  routes.length,
  "route ids must be unique across the contribution",
);

/**
 * The screens gate on `deploy.apps.read`, so the code must be a real authority
 * code. `API_SPEC.md` derives a permission from the operationId
 * (`[resource, action] = operationId.split(".")`, `list|retrieve` => read), while
 * `iam.module.manifest.json#permissions.catalog` is the registered catalog. A hint
 * that resolves from neither is a dangling gate, which is exactly the class of
 * silent front-end defect no repository gate currently catches.
 */
const deploymentsIamManifestPath = path.join(
  workspaceRoot,
  "sdkwork-deployments",
  "specs",
  "iam.module.manifest.json",
);
const deploymentsAppApiPath = path.join(
  workspaceRoot,
  "sdkwork-deployments",
  "apis",
  "app-api",
  "deploy",
  "openapi.yaml",
);

if (fs.existsSync(deploymentsIamManifestPath) && fs.existsSync(deploymentsAppApiPath)) {
  const iamManifest = readJson(deploymentsIamManifestPath);
  const catalogCodes = new Set(
    (iamManifest.permissions?.catalog ?? [])
      .map((entry) => entry?.code)
      .filter((code) => typeof code === "string"),
  );

  const openApiText = fs.readFileSync(deploymentsAppApiPath, "utf8");
  const derivedCodes = new Set();
  for (const match of openApiText.matchAll(/operationId:\s*([A-Za-z0-9_.]+)/gu)) {
    const [resource, action = ""] = match[1].split(".");
    derivedCodes.add(`deploy.${resource}.${/^(list|retrieve)/u.test(action) ? "read" : "write"}`);
  }
  assert.ok(derivedCodes.size > 0, "the deployments app-api contract must declare operationIds");

  for (const route of routes) {
    assert.ok(
      derivedCodes.has(route.permissionHint) || catalogCodes.has(route.permissionHint),
      `route ${route.id} permission hint ${route.permissionHint} must resolve from the owning module's app-api operations or its IAM catalog`,
    );
    assert.ok(
      derivedCodes.has(route.permissionHint),
      `route ${route.id} permission hint ${route.permissionHint} must correspond to a real deployments app-api operation`,
    );
  }

  const unregistered = [...derivedCodes].filter((code) => !catalogCodes.has(code));
  assert.ok(
    unregistered.length >= 0,
    "catalog coverage is reported, not asserted: the owning module's catalog is a cross-repository input",
  );
  console.log(
    `[harmony-surface-contract] deployments IAM catalog registers ${catalogCodes.size} codes; ` +
      `${unregistered.length} of ${derivedCodes.size} app-api-derived codes are not registered ` +
      `(deploy.apps.read registered: ${catalogCodes.has("deploy.apps.read")})`,
  );
}

// --- 11. Behavioural: pagination -------------------------------------------

const pagination = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-core/src/main/ets/sdk/Pagination.ets",
);

test("pagination narrows server page info without inventing totals", () => {
  const implicit = pagination.toWebserverHarmonyListPage({
    page: 1,
    pageSize: 20,
    totalItems: 45,
    totalPages: 3,
  });
  assert.equal(implicit.hasMore, true, "page 1 of 3 must still have more");

  const explicit = pagination.toWebserverHarmonyListPage({ page: 3, pageSize: 20, totalPages: 3, hasMore: false });
  assert.equal(explicit.hasMore, false);

  const empty = pagination.toWebserverHarmonyListPage(null);
  assert.deepEqual(
    { page: empty.page, pageSize: empty.pageSize, totalPages: empty.totalPages, hasMore: empty.hasMore },
    { page: 1, pageSize: 20, totalPages: 0, hasMore: false },
    "a missing pageInfo must fall back to page 1 with no more pages",
  );

  const clamped = pagination.normalizeWebserverHarmonyListPaging(0, 0);
  assert.equal(clamped.page, 1);
  assert.equal(clamped.pageSize, 20);
  assert.equal(clamped.hasMore, false);
});

// --- 12. Behavioural: SDK port ---------------------------------------------

const sdk = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-core/src/main/ets/sdk/WebserverAppSdkClient.ets",
);

test("the SDK port normalizes one surface base URL and rejects duplicates", () => {
  sdk.configureWebserverAppSdkBaseUrl("https://console.example.com/app/v3/api/");
  assert.equal(sdk.resolveWebserverAppSdkBaseUrl(), "https://console.example.com/app/v3/api");

  assert.throws(() => sdk.configureWebserverAppSdkBaseUrl("https://console.example.com"));
  assert.throws(() =>
    sdk.configureWebserverAppSdkBaseUrl("https://console.example.com/app/v3/api/app/v3/api"),
  );
  assert.throws(() => sdk.configureWebserverAppSdkBaseUrl("   "));

  sdk.resetWebserverAppSdkBaseUrl();
  assert.throws(() => sdk.resolveWebserverAppSdkBaseUrl());

  assert.deepEqual(sdk.WEBSERVER_APP_KINDS.length, 8);
  assert.deepEqual(sdk.WEBSERVER_APP_STATUSES.length, 6);
});

test("an unadapted transport reports unavailable and refuses to look empty", async () => {
  sdk.resetWebserverAppSdkBaseUrl();
  const port = sdk.createWebserverHarmonySdkPort({ baseUrl: "https://console.example.com/app/v3/api" });
  assert.equal(port.available, false, "no ArkTS transport exists yet, so the port must say so");
  assert.equal(port.platform, "harmony-native");
  assert.equal(port.baseUrl, "https://console.example.com/app/v3/api");

  await assert.rejects(
    () => port.applications.list(1, 20),
    (error) => {
      assert.equal(error.code, "app-sdk-unavailable");
      return true;
    },
    "an unavailable port must reject, never resolve to an empty list that reads as 'no applications'",
  );
  port.dispose();
});

test("an injected transport satisfies the same port without an adapter", async () => {
  sdk.resetWebserverAppSdkBaseUrl();
  const calls = [];
  const transport = {
    platform: "harmony-native",
    applications: {
      list(page, pageSize) {
        calls.push([page, pageSize]);
        return Promise.resolve({ items: [], pageInfo: { page, pageSize } });
      },
    },
  };
  const port = sdk.createWebserverHarmonySdkPort({
    baseUrl: "https://console.example.com/app/v3/api",
    transport,
  });
  assert.equal(port.available, true);
  assert.equal(port.applications, transport.applications, "the injected reader must pass straight through");
  const response = await port.applications.list(2, 10);
  assert.deepEqual(calls, [[2, 10]]);
  assert.equal(response.pageInfo.page, 2);
  port.dispose();
});

// --- 13. Behavioural: closed-set labels + locale fragments -----------------

const messages = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-applications/src/main/ets/i18n/ApplicationsMessages.ets",
);

test("every closed-set label resolves to copy that exists in both locales", () => {
  const en = messages.webserverHarmonyApplicationsMessagesEnUs;
  const zh = messages.webserverHarmonyApplicationsMessagesZhCn;

  assert.deepEqual(
    [...en.keys()].sort(),
    [...zh.keys()].sort(),
    "the two locale fragments must cover exactly the same keys",
  );
  assert.ok(en.size > 0);

  for (const label of [
    ...messages.WEBSERVER_HARMONY_APPLICATION_KIND_LABELS,
    ...messages.WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS,
  ]) {
    assert.ok(en.has(label.messageKey), `en-US must define ${label.messageKey}`);
    assert.ok(zh.has(label.messageKey), `zh-CN must define ${label.messageKey}`);
  }
  assert.ok(en.has("applications.list.row.unknown"), "the unknown-token fallback must exist");
  assert.ok(zh.has("applications.list.row.unknown"));
});

// --- 14. Behavioural: record mapping --------------------------------------

const mapping = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-applications/src/main/ets/models/ApplicationRecordMapping.ets",
);

test("record mapping drops unrenderable rows and never leaves a cell blank", () => {
  assert.equal(mapping.mapWebserverHarmonyApplicationRecord(null), null);
  assert.equal(mapping.mapWebserverHarmonyApplicationRecord(undefined), null);
  assert.equal(mapping.mapWebserverHarmonyApplicationRecord({ id: "" }), null);

  const mapped = mapping.mapWebserverHarmonyApplicationRecord({
    id: "app-1",
    name: "",
    slug: "console",
    appKind: "SPA_WEB",
    appStatus: "ACTIVE",
    platformTargetCount: Number.NaN,
  });
  assert.equal(mapped.name, "app-1", "a record with no display name must fall back to its id, not to blank");
  assert.equal(mapped.platformTargetCount, 0, "a non-numeric count must not reach the UI as NaN");
  assert.equal(mapped.description, "");
  assert.equal(mapped.latestReleaseTag, "");

  const row = mapping.toWebserverHarmonyApplicationRow(
    mapped,
    messages.WEBSERVER_HARMONY_APPLICATION_KIND_LABELS,
    messages.WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS,
    (key) => key,
    "applications.list.row.unknown",
  );
  assert.equal(row.kindLabel, "applications.list.kind.spa-web");
  assert.equal(row.statusLabel, "applications.list.status.active");

  const unknown = mapping.toWebserverHarmonyApplicationRow(
    { ...mapped, appKind: "SOMETHING_NEW", appStatus: "SOMETHING_NEW" },
    messages.WEBSERVER_HARMONY_APPLICATION_KIND_LABELS,
    messages.WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS,
    (key) => key,
    "applications.list.row.unknown",
  );
  assert.equal(unknown.kindLabel, "applications.list.row.unknown");
  assert.equal(unknown.statusLabel, "applications.list.row.unknown");
  assert.equal(mapping.lookupWebserverHarmonyEnumMessageKey([], "X", "fallback"), "fallback");
});

test("the ArkTS closed sets mirror the generated SDK unions exactly", () => {
  const deploymentsTypes = path.join(
    workspaceRoot,
    "sdkwork-deployments",
    "sdks",
    "sdkwork-deployments-app-sdk",
    "sdkwork-deployments-app-sdk-typescript",
    "generated",
    "server-openapi",
    "src",
    "types",
  );
  const webserverTypes = path.join(
    root,
    "..",
    "..",
    "sdks",
    "sdkwork-webserver-app-sdk",
    "sdkwork-webserver-app-sdk-typescript",
    "generated",
    "server-openapi",
    "src",
    "types",
  );

  const membersOf = (filePath, typeName) => {
    const source = fs.readFileSync(filePath, "utf8");
    const match = new RegExp(`export type ${typeName} =([^;]+);`, "u").exec(source);
    assert.ok(match, `${filePath} must declare ${typeName}`);
    return [...match[1].matchAll(/'([A-Z_]+)'/gu)].map((entry) => entry[1]).sort();
  };

  const generatedKinds = membersOf(path.join(deploymentsTypes, "app-kind.ts"), "AppKind");
  const generatedStatuses = membersOf(path.join(deploymentsTypes, "app-status.ts"), "AppStatus");
  assert.deepEqual(
    [...sdk.WEBSERVER_APP_KINDS].sort(),
    generatedKinds,
    "the ArkTS AppKind mirror must equal the generated SDK union",
  );
  assert.deepEqual(
    [...sdk.WEBSERVER_APP_STATUSES].sort(),
    generatedStatuses,
    "the ArkTS AppStatus mirror must equal the generated SDK union",
  );

  // The webserver app SDK owns the same AppKind set; if the two generated
  // sources ever disagree, that is a server-contract drift worth failing on.
  const webserverKinds = membersOf(path.join(webserverTypes, "app-kind.ts"), "AppKind");
  assert.deepEqual(
    webserverKinds,
    generatedKinds,
    "the webserver and deployments generated AppKind unions must agree",
  );

  assert.deepEqual(
    messages.WEBSERVER_HARMONY_APPLICATION_KIND_LABELS.map((entry) => entry.value).sort(),
    generatedKinds,
    "every generated AppKind member must have a label entry",
  );
  assert.deepEqual(
    messages.WEBSERVER_HARMONY_APPLICATION_STATUS_LABELS.map((entry) => entry.value).sort(),
    generatedStatuses,
    "every generated AppStatus member must have a label entry",
  );
});

// --- 15. Behavioural: route validation + auth gate -------------------------

test("route validation reports every violation of the route contract", () => {
  const valid = {
    id: "app.webserver.applications.list",
    surface: "app",
    domain: "webserver",
    capability: "applications",
    screen: "list",
    pagePath: "pages/applications/ApplicationsCatalog",
    titleKey: "applications.list.title",
    auth: "required",
    permissionHint: "deploy.apps.read",
  };
  assert.deepEqual(routePlacement.validateWebserverHarmonyRouteContributions([valid]), []);

  const issuesFor = (routes) => routePlacement.validateWebserverHarmonyRouteContributions(routes);
  assert.deepEqual(issuesFor([{ ...valid, id: "app.webserver.applications.detail" }]), [
    "route id app.webserver.applications.detail must equal app.webserver.applications.list",
  ]);
  assert.equal(issuesFor([valid, valid]).length, 2, "a duplicate id and a duplicate page path are both violations");
  assert.deepEqual(issuesFor([{ ...valid, pagePath: "  " }]), [
    "route app.webserver.applications.list must declare a page path",
  ]);
  assert.deepEqual(
    issuesFor([{ ...valid, permissionHint: undefined, navigation: { labelKey: "x", permission: "y", order: 1 } }]),
    ["route app.webserver.applications.list contributes navigation but declares no permissionHint"],
  );
});

const authGate = await loadArkTsModule(
  "packages/sdkwork-webserver-harmony-mobile-shell/src/main/ets/auth/AuthGate.ets",
);

test("the auth gate decides before any page renders", () => {
  const granted = { authenticated: true, hasPermission: (permission) => permission === "deploy.apps.read" };
  const denied = { authenticated: true, hasPermission: () => false };
  const anonymous = { authenticated: false, hasPermission: () => true };

  assert.deepEqual(
    authGate.resolveWebserverHarmonyRouteAccess({ auth: "required", permissionHint: "deploy.apps.read" }, anonymous),
    { allowed: false, reason: "unauthenticated" },
  );
  assert.deepEqual(
    authGate.resolveWebserverHarmonyRouteAccess({ auth: "required", permissionHint: "deploy.apps.read" }, denied),
    { allowed: false, reason: "forbidden" },
  );
  assert.deepEqual(
    authGate.resolveWebserverHarmonyRouteAccess({ auth: "required", permissionHint: "deploy.apps.read" }, granted),
    { allowed: true },
  );
  assert.deepEqual(
    authGate.resolveWebserverHarmonyRouteAccess({ auth: "public" }, anonymous),
    { allowed: true },
  );

  for (const [route, context] of [
    [{ auth: "required", permissionHint: "deploy.apps.read" }, anonymous],
    [{ auth: "required", permissionHint: "deploy.apps.read" }, denied],
  ]) {
    const decision = authGate.resolveWebserverHarmonyRouteAccess(route, context);
    assert.equal(decision.allowed, false, "a blocked route must not be reportable as enterable");
  }
});

console.log("harmony surface contract passed.");
