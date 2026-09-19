import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

/**
 * Static + reference-semantics architecture contract for the SDKWork Web Server
 * Flutter mobile root.
 *
 * Authority: `FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` (§3 root layout, §4
 * package shape, §6 SDK boundary, §8 routes, §9 runtime profile matrix, §11
 * verification), `APP_CLIENT_ARCHITECTURE_ALIGNMENT_SPEC.md`,
 * `APP_PERMISSION_COMPOSITION_SPEC.md`, `PAGINATION_SPEC.md`, and `I18N_SPEC.md`.
 *
 * **What this file can and cannot prove.** No Dart or Flutter toolchain exists
 * in this workspace (see `README.md` §Blocking Prerequisites), and Node cannot
 * execute Dart the way it can execute the HarmonyOS root's `.ets` sources
 * through its type-stripping loader. So this file never claims "the Dart ran".
 * It has three honest layers instead:
 *
 * 1. **Static** — layout, package family, layer roles, root thinness, runtime
 *    profile matrix, secret-free config, app manifest, SDK boundary, and import
 *    direction. Real filesystem evidence.
 * 2. **Source-text contracts** — the shipped `.dart` decision expressions are
 *    pinned by text, so the semantics cannot silently change while this gate
 *    stays green.
 * 3. **Scheduling assertions** — the committed `flutter_test` suites are
 *    asserted to name each behaviour, so a behaviour can never be dropped from
 *    the suite that will run once the toolchain lands.
 *
 * Where a runnable check is possible it is a **reference mirror** (pure JS)
 * explicitly labelled as such, never presented as proof of Dart execution.
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
  return JSON.parse(
    fs.readFileSync(absolutePath, "utf8").replace(/^\uFEFF/u, ""),
  );
}

const SKIP_DIRS = new Set([
  "node_modules",
  "build",
  ".dart_tool",
  ".git",
  "dist",
  "coverage",
]);

function listFiles(relativePath) {
  const absolute = path.join(root, relativePath);
  if (!fs.existsSync(absolute)) return [];
  return fs.readdirSync(absolute, { withFileTypes: true }).flatMap((entry) => {
    const childRelative = path.join(relativePath, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) return [];
      return listFiles(childRelative);
    }
    return [childRelative];
  });
}

/** Dart sources under `lib/` of the root and of every package. */
function authoredDartSources() {
  const dirs = ["lib", ...expectedPackages.map((n) => `packages/${n}/lib`)];
  return dirs
    .flatMap((dir) => listFiles(dir))
    .filter((filePath) => filePath.endsWith(".dart"))
    .map((filePath) => ({
      filePath: filePath.replaceAll("\\", "/"),
      source: mustExist(filePath),
    }));
}

/** Extract the string members of a Dart `const List<String> <name> = <String>[...]`. */
function extractConstStringList(source, constName) {
  const start = source.indexOf(constName);
  assert.ok(start >= 0, `${constName} must be declared`);
  const end = source.indexOf("];", start);
  assert.ok(end > start, `${constName} must be a terminated list literal`);
  return [...source.slice(start, end).matchAll(/'([^']+)'/gu)].map((m) => m[1]);
}

/** Extract the `value`/`messageKey` pairs of a Dart `WebserverFlutterEnumLabel` list. */
function extractEnumLabels(source, constName) {
  const start = source.indexOf(constName);
  assert.ok(start >= 0, `${constName} must be declared`);
  const end = source.indexOf("];", start);
  assert.ok(end > start, `${constName} must be a terminated list literal`);
  const pairs = [
    ...source
      .slice(start, end)
      .matchAll(/value:\s*'([^']+)',\s*messageKey:\s*'([^']+)'/gu),
  ].map((m) => ({ value: m[1], messageKey: m[2] }));
  assert.ok(pairs.length > 0, `${constName} must carry at least one label`);
  return pairs;
}

/** Extract the keys of a Dart `const Map<String, String> <name> = {...}`. */
function extractDartMessageKeys(source, constName) {
  const start = source.indexOf(constName);
  assert.ok(start >= 0, `${constName} must be declared`);
  const end = source.indexOf("};", start);
  assert.ok(end > start, `${constName} must be a terminated map literal`);
  return new Set(
    [...source.slice(start, end).matchAll(/^\s*'([^']+)':/gmu)].map((m) => m[1]),
  );
}

const expectedPackages = [
  "sdkwork_webserver_flutter_mobile_core",
  "sdkwork_webserver_flutter_mobile_commons",
  "sdkwork_webserver_flutter_mobile_shell",
  "sdkwork_webserver_flutter_mobile_applications",
];

const profiles = [
  "standalone.development",
  "standalone.test",
  "standalone.staging",
  "standalone.demo",
  "standalone.production",
];

const environmentKeys = [
  "SDKWORK_DEPLOYMENT_PROFILE",
  "SDKWORK_ENVIRONMENT",
  "SDKWORK_PROFILE_ID",
  "SDKWORK_RUNTIME_TARGET",
  "SDKWORK_WEBSERVER_APP_API_BASE_URL",
  "SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL",
  "SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE",
  "SDKWORK_WEBSERVER_ENVIRONMENT",
  "SDKWORK_WEBSERVER_PROFILE_ID",
  "SDKWORK_WEBSERVER_RUNTIME_TARGET",
];

// --- 1. Root layout --------------------------------------------------------

for (const requiredPath of [
  "AGENTS.md",
  "README.md",
  ".gitignore",
  ".env.example",
  "pubspec.yaml",
  "sdkwork.app.config.json",
  "specs/component.spec.json",
  ".sdkwork/README.md",
  ".sdkwork/skills/README.md",
  ".sdkwork/plugins/README.md",
  "docs/README.md",
  "sdks/README.md",
  "scripts/README.md",
  "etc/README.md",
  "etc/sdkwork.deployment.config.json",
  "config/app/runtime-env.development.example.json",
  "lib/main.dart",
  "lib/app.dart",
  "lib/auth_gate.dart",
  "lib/bootstrap/environment.dart",
  "lib/bootstrap/runtime.dart",
  "lib/bootstrap/sdk_clients.dart",
  "lib/bootstrap/iam_runtime.dart",
  "lib/bootstrap/host_adapters.dart",
  "lib/bootstrap/routes.dart",
  "tests/flutter-surface-contract.test.mjs",
]) {
  mustExist(requiredPath);
}

// --- 2. Package family -----------------------------------------------------

for (const packageName of expectedPackages) {
  const packageRoot = `packages/${packageName}`;
  mustExist(`${packageRoot}/pubspec.yaml`);
  mustExist(`${packageRoot}/lib/${packageName}.dart`);
  const componentSpec = JSON.parse(
    mustExist(`${packageRoot}/specs/component.spec.json`),
  );
  assert.equal(
    componentSpec.component?.root,
    `sdkwork-webserver/apps/sdkwork-webserver-flutter-mobile/packages/${packageName}`,
    `${packageName} component spec root must use the canonical Flutter package path`,
  );
  assert.equal(
    componentSpec.component?.type,
    "dart-package",
    `${packageName} must declare the dart-package component type`,
  );
  assert.ok(
    componentSpec.component?.languages?.includes("dart"),
    `${packageName} must declare dart as its language`,
  );

  // Every declared path reference must resolve from the component root — the
  // relative depth differs between `specs/`, `lib/`, and the workspace, and a
  // wrong depth fails silently everywhere else.
  const resolveFromComponent = (reference) =>
    path
      .resolve(root, packageRoot, reference)
      .replaceAll("\\", "/");
  for (const spec of componentSpec.canonicalSpecs ?? []) {
    assert.ok(
      fs.existsSync(resolveFromComponent(spec.path)),
      `${packageName} canonicalSpecs ${spec.file} must resolve (${spec.path})`,
    );
  }
  for (const manifest of componentSpec.component?.manifests ?? []) {
    assert.ok(
      fs.existsSync(resolveFromComponent(manifest)),
      `${packageName} manifest ${manifest} must resolve`,
    );
  }

  // A capability package must not import a generated SDK; only `core` does.
  if (!packageName.endsWith("_core")) {
    assert.deepEqual(
      componentSpec.contracts?.sdkDependencies ?? [],
      [],
      `${packageName} must not declare SDK dependencies; core owns SDK construction`,
    );
  }
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

const expectedLayerRoles = {
  sdkwork_webserver_flutter_mobile_core: "frontend-core",
  sdkwork_webserver_flutter_mobile_commons: "frontend-commons",
  sdkwork_webserver_flutter_mobile_shell: "frontend-shell",
  sdkwork_webserver_flutter_mobile_applications: "frontend-feature",
};

for (const packageName of expectedPackages) {
  const componentSpec = JSON.parse(
    mustExist(`packages/${packageName}/specs/component.spec.json`),
  );
  const layerRole = componentSpec.contracts?.layerRole;
  assert.ok(
    allowedLayerRoles.has(layerRole),
    `${packageName} contracts.layerRole ${JSON.stringify(layerRole)} is not an allowed composable layer role`,
  );
  assert.equal(
    layerRole,
    expectedLayerRoles[packageName],
    `${packageName} must carry the layer role the Flutter taxonomy assigns it`,
  );
}

// --- 4. Root thinness ------------------------------------------------------

const rootLibFiles = listFiles("lib").map((filePath) =>
  filePath.replaceAll("\\", "/"),
);
const allowedRootLibFiles = new Set([
  "lib/main.dart",
  "lib/app.dart",
  "lib/auth_gate.dart",
  "lib/bootstrap/environment.dart",
  "lib/bootstrap/runtime.dart",
  "lib/bootstrap/sdk_clients.dart",
  "lib/bootstrap/iam_runtime.dart",
  "lib/bootstrap/host_adapters.dart",
  "lib/bootstrap/routes.dart",
]);
assert.deepEqual(
  rootLibFiles.filter((filePath) => !allowedRootLibFiles.has(filePath)),
  [],
  "root lib/ must stay thin: only the entry point, the shell widget, the route guard, and bootstrap belong there",
);
assert.ok(
  rootLibFiles.some((filePath) => filePath.endsWith(".dart")),
  "the root must own Dart source",
);

// A shared package that owns a screen would invert the layering the composition
// spec fixes; only capability packages contribute screens.
for (const packageName of expectedPackages) {
  const screens = listFiles(`packages/${packageName}/lib`)
    .map((filePath) => filePath.replaceAll("\\", "/"))
    .filter((filePath) => /\/(screens|pages|views)\//u.test(filePath));
  if (packageName.endsWith("_applications")) {
    assert.ok(
      screens.length > 0,
      "the capability package must own its screen",
    );
  } else {
    assert.deepEqual(
      screens,
      [],
      `${packageName} must not own screens: ${screens.join(", ")}`,
    );
  }
}

// --- 5. Runtime profile matrix ---------------------------------------------

const deploymentConfig = JSON.parse(
  mustExist("etc/sdkwork.deployment.config.json"),
);
const repositoryDeploymentIndex = readJson(
  path.join(root, "etc", deploymentConfig.parentDeploymentConfig),
);
assert.equal(deploymentConfig.kind, "sdkwork.component-deployment");
assert.equal(
  deploymentConfig.application,
  "sdkwork-webserver-flutter-mobile",
  "the component deployment config must name this root",
);
assert.equal(
  deploymentConfig.parentDeploymentConfig,
  "../../../etc/sdkwork.deployment.config.json",
  "the parent deployment pointer must be declared from the app root",
);
assert.equal(
  repositoryDeploymentIndex.kind,
  "sdkwork.deployment-index",
  "the parent pointer must resolve from etc/ (not from the app root) to the repository deployment index",
);

const materialization = deploymentConfig.materialization ?? {};
assert.equal(materialization.format, "dart-define-json");
assert.equal(
  materialization.command,
  "pnpm workflow:materialize-client-env",
  "the materialization command must name the shared harness",
);
assert.equal(
  materialization.outputPattern,
  "../env/sdkwork.{deploymentProfile}.{environment}.json",
  "Flutter materializes a dart-define-from-file JSON under the app root's env/ (ENVIRONMENT_SPEC.md)",
);
assert.deepEqual(
  [...(materialization.profiles ?? [])].sort(),
  [...profiles].sort(),
  "the component deployment config must mirror the repository profile matrix",
);
assert.deepEqual(
  Object.keys(repositoryDeploymentIndex.profiles ?? {}).sort(),
  [...profiles].sort(),
  "the repository deployment index must expose the same profile ids",
);
assert.equal(
  deploymentConfig.profiles,
  undefined,
  "Flutter roots pin profiles through materialization.profiles; a second profiles map must not appear",
);

for (const profileId of profiles) {
  const environment = profileId.split(".")[1];

  // The output pattern must expand to a file that actually exists and is the
  // one this contract reads — otherwise the pattern and the tree have drifted.
  const materializedPath = path
    .resolve(
      root,
      "etc",
      materialization.outputPattern
        .replace("{deploymentProfile}", "standalone")
        .replace("{environment}", environment),
    )
    .replaceAll("\\", "/");
  const committedPath = path
    .resolve(root, "env", `sdkwork.${profileId}.json`)
    .replaceAll("\\", "/");
  assert.equal(
    materializedPath,
    committedPath,
    `${profileId}: materialization.outputPattern must expand to the committed env file`,
  );

  const env = readJson(committedPath);
  assert.deepEqual(
    Object.keys(env).sort(),
    [...environmentKeys].sort(),
    `${profileId} must declare exactly the SDKWORK_* and SDKWORK_WEBSERVER_* keys`,
  );
  assert.equal(env.SDKWORK_DEPLOYMENT_PROFILE, "standalone");
  assert.equal(env.SDKWORK_ENVIRONMENT, environment);
  assert.equal(env.SDKWORK_PROFILE_ID, profileId);
  assert.equal(env.SDKWORK_RUNTIME_TARGET, "flutter-android");

  // The alias set must not drift from the canonical set. The two URL keys are
  // application-scoped by nature and exist only in the prefixed form; the four
  // profile keys exist in both, and must agree.
  for (const canonical of [
    "SDKWORK_DEPLOYMENT_PROFILE",
    "SDKWORK_ENVIRONMENT",
    "SDKWORK_PROFILE_ID",
    "SDKWORK_RUNTIME_TARGET",
  ]) {
    assert.equal(
      env[`SDKWORK_WEBSERVER_${canonical.replace("SDKWORK_", "")}`],
      env[canonical],
      `${profileId} SDKWORK_WEBSERVER_${canonical.replace("SDKWORK_", "")} must alias ${canonical}`,
    );
  }
  for (const unprefixedOnlyInApplicationScope of [
    "SDKWORK_APP_API_BASE_URL",
    "SDKWORK_APPLICATION_PUBLIC_HTTP_URL",
  ]) {
    assert.equal(
      Object.hasOwn(env, unprefixedOnlyInApplicationScope),
      false,
      `${profileId} must not carry an unprefixed ${unprefixedOnlyInApplicationScope}: the surface URL is application-scoped`,
    );
  }

  const expectedOrigin =
    repositoryDeploymentIndex.environments?.[environment]?.applicationOrigin;
  assert.ok(
    expectedOrigin,
    `the repository deployment index must define ${environment}`,
  );
  assert.equal(
    env.SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL,
    expectedOrigin,
    `${profileId} must use the repository application origin verbatim`,
  );
  assert.equal(
    env.SDKWORK_WEBSERVER_APP_API_BASE_URL,
    `${expectedOrigin}/app/v3/api`,
    `${profileId} app API base URL must be the application origin plus the surface prefix`,
  );
}

// The Kotlin/Dart define names the bootstrap reads must exist as declared.
const environmentSource = mustExist("lib/bootstrap/environment.dart");
for (const defineName of [
  "SDKWORK_WEBSERVER_APP_API_BASE_URL",
  "SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL",
  "SDKWORK_DEPLOYMENT_PROFILE",
  "SDKWORK_ENVIRONMENT",
  "SDKWORK_PROFILE_ID",
  "SDKWORK_RUNTIME_TARGET",
]) {
  assert.ok(
    environmentSource.includes(`String.fromEnvironment(\n  '${defineName}'`) ||
      environmentSource.includes(`'${defineName}'`),
    `lib/bootstrap/environment.dart must read ${defineName}`,
  );
}
assert.deepEqual(
  extractConstStringList(environmentSource, "webserverFlutterEnvironments"),
  profiles.map((profileId) => profileId.split(".")[1]),
  "the bootstrap's environment list must equal the deployment index's environments",
);
assert.match(
  environmentSource,
  /const String webserverFlutterRuntimeTarget = 'flutter-android';/u,
);
assert.match(
  environmentSource,
  /throw StateError\(/u,
  "an unconfigured build must throw instead of falling back to a default host",
);

// --- 6. Config must stay secret-free ---------------------------------------

const secretPattern =
  /(signingPrivateKey|privateKey|refreshToken|apiKey|databaseUrl|password)\s*[:=]\s*["'][^"'<]/iu;
const exampleConfigPath = "config/app/runtime-env.development.example.json";
assert.doesNotMatch(
  mustExist(exampleConfigPath),
  secretPattern,
  `${exampleConfigPath} must not contain secrets`,
);

const envExample = mustExist(".env.example");
assert.doesNotMatch(
  envExample,
  /^\s*[A-Z0-9_]*(?:TOKEN|SECRET|KEY|PASSWORD)[A-Z0-9_]*\s*=\s*\S+/mu,
  ".env.example must declare names only, never a value",
);

const gitignore = mustExist(".gitignore");
for (const ignored of [".dart_tool/", "build/", "env/*.local.json"]) {
  assert.ok(
    gitignore.includes(ignored),
    `.gitignore must ignore ${ignored}`,
  );
}

// Flutter's canonical host suffix is `null` — a Flutter root owns no native
// host package (check-client-host-packages.mjs CANONICAL_HOST). A `config/host/`
// tree here would be a half-wired platform, so it must be absent rather than
// present-but-empty.
assert.equal(
  fs.existsSync(path.join(root, "config", "host")),
  false,
  "a Flutter root ships no native host package, so it must not carry config/host/",
);

// --- 7. App manifest -------------------------------------------------------

const manifest = JSON.parse(mustExist("sdkwork.app.config.json"));
assert.equal(manifest.schemaVersion, 3, "the manifest must use App Standard v3");
assert.equal(manifest.kind, "sdkwork.app");
assert.equal(
  manifest.app?.appType,
  "APP_FLUTTER",
  "a Flutter root must declare appType APP_FLUTTER",
);
assert.equal(
  manifest.app?.versionSource,
  "pubspec.yaml",
  "a Flutter root sources its version from pubspec.yaml",
);
assert.equal(manifest.app?.key, "sdkwork-webserver-flutter-mobile");
assert.equal(manifest.runtime?.family, "mobile");
assert.equal(manifest.runtime?.framework, "flutter");
assert.deepEqual(manifest.runtime?.runtimes, ["APP", "APP_ANDROID", "APP_IOS"]);
assert.deepEqual(manifest.runtime?.deliveryModes, ["DIRECT_DOWNLOAD"]);
assert.deepEqual(manifest.runtime?.supportedDeploymentProfiles, ["standalone"]);
assert.equal(manifest.runtime?.defaultDeploymentProfile, "standalone");
assert.equal(manifest.runtime?.defaultPlatform, "APP_ANDROID");
assert.ok(manifest.publish?.platforms?.includes("APP_ANDROID"));
assert.ok(manifest.publish?.installPlatforms?.includes("APP_ANDROID"));
assert.ok(
  manifest.media?.icons?.primary?.url,
  "the primary icon must carry a real asset reference",
);
const currentReleases = (manifest.release?.notes ?? []).filter(
  (entry) => entry.current === true,
);
assert.equal(currentReleases.length, 1, "exactly one release note must be current");
assert.equal(
  manifest.metadata?.deploymentConfig,
  "etc/sdkwork.deployment.config.json",
);
assert.equal(manifest.metadata?.architectureSpec, "FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md");
assert.equal(manifest.metadata?.runtimeTarget, "flutter-android");

const appPubspec = mustExist("pubspec.yaml");
const pubspecName = /^name:\s*(\S+)/mu.exec(appPubspec)?.[1];
const pubspecVersion = /^version:\s*(\S+)/mu.exec(appPubspec)?.[1];
assert.equal(pubspecName, "sdkwork_webserver_flutter_mobile");
assert.equal(
  pubspecVersion,
  manifest.release?.currentVersion,
  "pubspec.yaml is the version source, so it must equal release.currentVersion",
);

// An unclaimed signature is a declared gap; it must not be dressed up as done.
assert.equal(manifest.security?.checksumRequired, true);
assert.equal(manifest.security?.sbomRequired, true);
assert.equal(
  manifest.security?.signatureRequired,
  false,
  "no signing profile exists yet, so the manifest must not claim one",
);

// --- 8. SDK boundary -------------------------------------------------------

/** Authored sources only: the generated SDK legitimately depends on `http`. */
const rawHttpPattern =
  /(?:package:http\b|dart:io\b|HttpClient\s*\(|\bDio\s*\(|["']Authorization["'])/u;
for (const { filePath, source } of authoredDartSources()) {
  const violation = rawHttpPattern.exec(source);
  assert.equal(
    violation,
    null,
    `${filePath} must not perform raw HTTP transport or hand-write auth (found ${JSON.stringify(violation?.[0])})`,
  );
}

// The `deploy_app` entity is owned by `sdkwork-deployments`, and this root reads
// it through the catalog port asserted below. No generated Dart artifact for
// that authority exists yet, so the root declares no `*_app_sdk` path dependency
// at all: naming one would point at a surface that is not generated, and the
// catalog port is what turns that gap into an explicit state instead of a build
// failure nobody sees until the toolchain lands.
const corePubspec = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_core/pubspec.yaml",
);
assert.ok(
  !/^[ \t]*\w+_app_sdk:[ \t]*$/mu.test(corePubspec),
  "core must not declare a generated app SDK path dependency while none is generated",
);

const catalogPortSource = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_core/lib/sdk/webserver_deploy_app_catalog_port.dart",
);
assert.match(
  catalogPortSource,
  /bool get available => _reader != null;/u,
  "the catalog port must derive availability from its binding",
);
assert.match(
  catalogPortSource,
  /static const String platform = 'flutter-android';/u,
);
assert.match(
  catalogPortSource,
  /static const String code = 'deploy-app-sdk-unavailable';/u,
  "an unbound port must report a distinct code, not an empty page",
);

// No local SDK fork: the only allowed override is the shared Flutter commons.
for (const pubspecPath of [
  "pubspec.yaml",
  ...expectedPackages.map((n) => `packages/${n}/pubspec.yaml`),
]) {
  const source = mustExist(pubspecPath);
  const overridesBlock = /^dependency_overrides:\n((?:[ \t]+.*\n?)*)/mu.exec(source)?.[1] ?? "";
  const overrideNames = [...overridesBlock.matchAll(/^[ \t]{2}([a-z0-9_]+):/gmu)].map(
    (m) => m[1],
  );
  for (const name of overrideNames) {
    assert.ok(
      !/_app_sdk$|_backend_sdk$/u.test(name),
      `${pubspecPath} must not fork an SDK through dependency_overrides: ${name}`,
    );
  }
}

const rootComponentSpec = JSON.parse(mustExist("specs/component.spec.json"));
assert.equal(rootComponentSpec.component?.type, "flutter-mobile-app-root");
assert.deepEqual(
  (rootComponentSpec.contracts?.sdkDependencies ?? [])
    .map((entry) => entry.workspace)
    .sort(),
  [
    "sdkwork-deployments-app-sdk",
    "sdkwork-drive-app-sdk",
  ],
  "the root must compose the deployments- and drive-owned app SDK families — the same set the PC, H5, mini program, and HarmonyOS roots compose",
);

// The declared Dart coverage must match the filesystem, and the unbound catalog
// port must be attributable to exactly one of those entries.
for (const entry of rootComponentSpec.metadata?.dartSdkCoverage ?? []) {
  assert.equal(
    entry.available,
    fs.existsSync(path.join(repoRoot, entry.variantPath)),
    `dartSdkCoverage for ${entry.workspace} must match the filesystem (${entry.variantPath})`,
  );
}
const availableDartSdks = (rootComponentSpec.metadata?.dartSdkCoverage ?? [])
  .filter((entry) => entry.available)
  .map((entry) => entry.workspace);
assert.deepEqual(
  availableDartSdks.sort(),
  [],
  "no declared app SDK family ships a Dart variant today, so the catalog port is unbound",
);
assert.ok(
  !availableDartSdks.includes("sdkwork-deployments-app-sdk"),
  "the deploy_app catalog port is unbound precisely because deployments ships no Dart artifact",
);

const coreSdkInventory = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_core/lib/composition/sdk_inventory.dart",
);
for (const entry of rootComponentSpec.metadata?.dartSdkCoverage ?? []) {
  const flag = entry.available ? "true" : "false";
  assert.ok(
    coreSdkInventory.includes(`packageName: '${entry.packageName}'`) &&
      coreSdkInventory.includes(`dartArtifactAvailable: ${flag}`),
    `core's SDK inventory must record ${entry.packageName} dartArtifactAvailable: ${flag}`,
  );
}

// --- 9. Dart import direction ---------------------------------------------

/**
 * Compensates a real gate blind spot: `check-frontend-composition.mjs` collects
 * `/\.(?:ts|tsx|js|jsx)$/` only, and `check-app-sdk-consumer-imports.mjs`
 * requires `\.(?:tsx?|jsx?|mjs|cjs|json)$`. Both are blind to `.dart`, so the
 * dependency direction they would otherwise enforce is asserted here on the
 * shipped sources.
 */
const siblingPackages = new Map(
  expectedPackages.map((packageName) => [
    packageName,
    packageName.replace("sdkwork_webserver_flutter_mobile_", ""),
  ]),
);

const allowedSiblingImports = new Map([
  // core owns generated-SDK construction; nothing else may import a transport.
  ["core", new Set()],
  ["commons", new Set()],
  ["shell", new Set()],
  // a capability consumes shared primitives, the shell route contract, and the
  // injected SDK port; it must never bind a transport itself.
  ["applications", new Set(["core", "commons", "shell"])],
]);

const generatedSdkImportPattern = /package:[a-z0-9_]*_(?:app|backend)_sdk\//u;

for (const packageName of expectedPackages) {
  const role = siblingPackages.get(packageName);
  const sources = listFiles(`packages/${packageName}/lib`)
    .filter((filePath) => filePath.endsWith(".dart"))
    .map((filePath) => ({ filePath, source: mustExist(filePath) }));

  for (const { filePath, source } of sources) {
    const normalized = filePath.replaceAll("\\", "/");

    for (const match of source.matchAll(/^\s*import\s+'package:([a-z0-9_]+)\//gmu)) {
      const imported = match[1];
      const targetRole = siblingPackages.get(imported);
      if (targetRole !== undefined) {
        assert.ok(
          allowedSiblingImports.get(role)?.has(targetRole),
          `${normalized} (${role}) must not depend on the ${targetRole} package; allowed: ${[...(allowedSiblingImports.get(role) ?? [])].join(", ") || "none"}`,
        );
        continue;
      }
      if (generatedSdkImportPattern.test(`package:${imported}/`)) {
        assert.equal(
          role,
          "core",
          `${normalized} (${role}) must not import the generated SDK; only core constructs a transport`,
        );
      }
    }

    // A relative specifier resolves on disk, so it can cross a package boundary
    // without ever naming a `package:` URI. Resolve each one and require it to
    // stay inside the owning package; otherwise the table above could be
    // satisfied while the real dependency direction is inverted.
    for (const match of source.matchAll(/^\s*(?:import|export)\s+'(\.\.?\/[^']+)'/gmu)) {
      const specifier = match[1];
      const resolved = path
        .resolve(root, path.dirname(filePath), specifier)
        .replaceAll("\\", "/");
      assert.ok(
        resolved.startsWith(
          `${root.replaceAll("\\", "/")}/packages/${packageName}/`,
        ),
        `${normalized} escapes its own package via '${specifier}' -> ${resolved}`,
      );
    }
  }
}

// The root composition layer may import every role, but it must not reach for
// the generated SDK directly — `lib/bootstrap/` goes through core.
for (const { filePath, source } of listFiles("lib")
  .filter((filePath) => filePath.endsWith(".dart"))
  .map((filePath) => ({ filePath, source: mustExist(filePath) }))) {
  assert.doesNotMatch(
    source,
    /^\s*import\s+'package:[a-z0-9_]*_(?:app|backend)_sdk\//gmu,
    `${filePath.replaceAll("\\", "/")} must construct the SDK through core, not directly`,
  );
}

// --- 10. Route identity + permission authority -----------------------------

const routeSource = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_applications/lib/src/routes/route_contributions.dart",
);
const routeIds = [...routeSource.matchAll(/\bid:\s*'([^']+)'/gu)].map((m) => m[1]);
const routeDomains = [...routeSource.matchAll(/\bdomain:\s*'([^']+)'/gu)].map((m) => m[1]);
const routeCapabilities = [...routeSource.matchAll(/\bcapability:\s*'([^']+)'/gu)].map((m) => m[1]);
const routeScreens = [...routeSource.matchAll(/\bscreen:\s*'([^']+)'/gu)].map((m) => m[1]);
const routeNames = [...routeSource.matchAll(/routeName:\s*'([^']+)'/gu)].map((m) => m[1]);
const titleKeys = [...routeSource.matchAll(/titleKey:\s*'([^']+)'/gu)].map((m) => m[1]);
const permissionHints = [...routeSource.matchAll(/permissionHint:\s*'([^']+)'/gu)].map((m) => m[1]);
const navigationPermissions = [...routeSource.matchAll(/\bpermission:\s*'([^']+)'/gu)].map((m) => m[1]);

assert.ok(routeIds.length > 0, "the applications capability must contribute a route");
// Every other field is pinned to `routeIds.length`, so two identical entries
// would keep all of those equalities intact and pass. Uniqueness is the one
// property the column-count checks cannot express, and a duplicated route id
// would let one screen silently shadow another.
assert.equal(
  new Set(routeIds).size,
  routeIds.length,
  "route ids must be unique across the contribution",
);
assert.equal(routeDomains.length, routeIds.length);
assert.equal(routeCapabilities.length, routeIds.length);
assert.equal(routeScreens.length, routeIds.length);
assert.equal(routeNames.length, routeIds.length);
assert.equal(titleKeys.length, routeIds.length);
assert.equal(permissionHints.length, routeIds.length);
// The route contribution declares `surface: 'app'` implicitly via the class
// default, so the id must be composed with the same literal.
const routes = routeIds.map((id, index) => ({
  id,
  surface: "app",
  domain: routeDomains[index],
  capability: routeCapabilities[index],
  screen: routeScreens[index],
  routeName: routeNames[index],
  titleKey: titleKeys[index],
  permissionHint: permissionHints[index],
}));

for (const route of routes) {
  assert.equal(
    route.id,
    `${route.surface}.${route.domain}.${route.capability}.${route.screen}`,
    "route ids must follow <surface>.<domain>.<capability>.<screen>",
  );
  assert.ok(route.permissionHint, `route ${route.id} must declare its permission hint`);
  assert.ok(route.routeName.startsWith("/"), `route ${route.id} must declare an absolute route name`);
  assert.match(route.titleKey, /^[a-z][a-zA-Z0-9]*(\.[a-zA-Z0-9-]+)+$/u);
}
assert.deepEqual(
  [...navigationPermissions].sort(),
  [...permissionHints].sort(),
  "a navigation entry and its route must not disagree about the permission",
);

// Cross-root parity. The mini program root is the anchor the HarmonyOS root is
// already pinned to; H5 is pinned here too, now that its registry composes the
// same four-segment id. PC is not included: the PC renderer does not declare an
// applications route at all, it bridges the deployments console package, so PC
// has no declaration to compare against on this capability.
const miniProgramRoutesPath = path.join(
  repoRoot,
  "apps",
  "sdkwork-webserver-mini-program",
  "packages",
  "sdkwork-webserver-mp-applications",
  "src",
  "routes",
  "routeContributions.ts",
);
assert.ok(
  fs.existsSync(miniProgramRoutesPath),
  "the mini program route contributions must exist to anchor cross-root parity",
);
const miniProgramRoutes = fs.readFileSync(miniProgramRoutesPath, "utf8");
for (const route of routes) {
  assert.ok(
    miniProgramRoutes.includes(`"${route.id}"`),
    `route ${route.id} must also be declared by the mini program root so the mobile clients cannot drift`,
  );
  assert.ok(
    miniProgramRoutes.includes(`"${route.permissionHint}"`),
    `route ${route.id} permission hint must match the mini program root's hint`,
  );
}

/**
 * The H5 registry composes its id from four exported constants rather than
 * spelling the id out, so a literal substring search would prove nothing about
 * it. Rebuilding the id from the same constants H5 exports is what actually
 * pins the two roots together: a drift in any single segment fails here.
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

/**
 * The screens gate on `deploy.apps.read`, so the code must be a real authority
 * code. `API_SPEC.md` derives a permission from the operationId
 * (`[resource, action] = operationId.split(".")`, `list|retrieve` => read), while
 * `iam.module.manifest.json#permissions.catalog` is the registered catalog. A
 * hint that resolves from neither is a dangling gate — the class of silent
 * front-end defect no repository gate currently catches.
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

if (
  fs.existsSync(deploymentsIamManifestPath) &&
  fs.existsSync(deploymentsAppApiPath)
) {
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
    derivedCodes.add(
      `deploy.${resource}.${/^(list|retrieve)/u.test(action) ? "read" : "write"}`,
    );
  }
  assert.ok(derivedCodes.size > 0, "the deployments app-api contract must declare operationIds");

  for (const route of routes) {
    assert.ok(
      derivedCodes.has(route.permissionHint),
      `route ${route.id} permission hint ${route.permissionHint} must correspond to a real deployments app-api operation`,
    );
  }

  const unregistered = [...derivedCodes].filter((code) => !catalogCodes.has(code));
  console.log(
    `[flutter-surface-contract] deployments IAM catalog registers ${catalogCodes.size} codes; ` +
      `${unregistered.length} of ${derivedCodes.size} app-api-derived codes are not registered ` +
      `(deploy.apps.read registered: ${catalogCodes.has("deploy.apps.read")})`,
  );
}

// The bootstrap permission scope must carry every route's hint, or the screen
// would be gated on a permission the build never requests.
const backendScope = new Set(
  manifest.backend?.accessTokenPermissionScope ?? [],
);
for (const route of routes) {
  assert.ok(
    backendScope.has(route.permissionHint),
    `sdkwork.app.config.json backend.accessTokenPermissionScope must include ${route.permissionHint}`,
  );
}

// --- 11. Behavioural: pagination -------------------------------------------

const paginationSource = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_core/lib/sdk/pagination.dart",
);

/**
 * Reference semantics for `toWebserverFlutterListPage`.
 *
 * This mirror exists so the *intended* narrowing rules are executable here. It
 * is NOT evidence that the Dart ran — §11 pins the shipped Dart expression by
 * source text, and §13 asserts the committed `flutter_test` suite names each
 * case.
 */
function referenceListPage(pageInfo) {
  const normalizeCount = (value, fallback, minimum) => {
    let parsed;
    if (typeof value === "number") {
      parsed = Number.isFinite(value) ? Math.trunc(value) : fallback;
    } else {
      parsed = Number.parseInt(String(value ?? ""), 10);
      if (!Number.isFinite(parsed)) parsed = fallback;
    }
    return parsed < minimum ? minimum : parsed;
  };
  const page = normalizeCount(pageInfo?.page, 1, 1);
  const pageSize = normalizeCount(pageInfo?.pageSize, 20, 1);
  const totalPages = normalizeCount(pageInfo?.totalPages, 0, 0);
  const totalItems = normalizeCount(pageInfo?.totalItems, 0, 0);
  return {
    hasMore: pageInfo?.hasMore === true || (totalPages > 0 && page < totalPages),
    page,
    pageSize,
    totalItems,
    totalPages,
  };
}

test("the shipped Dart narrows server page info with the same rule the mirror encodes", () => {
  assert.match(
    paginationSource,
    /hasMore:\s*pageInfo\?\.hasMore == true \|\| \(totalPages > 0 && page < totalPages\),/u,
    "the hasMore rule must be exactly 'server said so, or page < totalPages' — never invented from rendered rows",
  );
  assert.match(
    paginationSource,
    /return parsed < minimum \? minimum : parsed;/u,
    "counts must be clamped to the declared minimum",
  );
  assert.match(
    paginationSource,
    /const int defaultWebserverFlutterListPageSize = 20;/u,
    "the default page size must be the value the mirror uses",
  );
  assert.match(
    paginationSource,
    /final page = _normalizeCount\(pageInfo\?\.page, 1, 1\);/u,
    "a missing page must fall back to page 1",
  );
});

test("the reference mirror narrows page info without inventing totals", () => {
  const implicit = referenceListPage({
    page: 1,
    pageSize: 20,
    totalItems: "45",
    totalPages: 3,
  });
  assert.deepEqual(implicit, {
    hasMore: true,
    page: 1,
    pageSize: 20,
    totalItems: 45,
    totalPages: 3,
  });

  const last = referenceListPage({ page: 3, pageSize: 20, totalPages: 3, hasMore: false });
  assert.equal(last.hasMore, false);

  const empty = referenceListPage(null);
  assert.deepEqual(empty, {
    hasMore: false,
    page: 1,
    pageSize: 20,
    totalItems: 0,
    totalPages: 0,
  });

  // A server that omits totalPages entirely must not be read as "no more".
  const cursorMode = referenceListPage({ page: 1, pageSize: 20, hasMore: true });
  assert.equal(cursorMode.hasMore, true);
});

// --- 12. Behavioural: runtime surface URL contracts ------------------------

const clientSource = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_core/lib/sdk/app_api_surface_url.dart",
);

test("the runtime environment normalizes the surface URL", () => {
  assert.match(
    clientSource,
    /final normalized = value\.trim\(\)\.replaceFirst\(RegExp\(r'\/\+\$'\), ''\);/u,
    "the surface URL must be trimmed and de-slashed before validation",
  );
  assert.match(
    clientSource,
    /uri\.hasQuery \|\|\s*uri\.hasFragment/u,
    "a URL carrying a query or fragment is not a base URL",
  );
  assert.match(
    clientSource,
    /!uri\.path\.endsWith\(webserverAppApiPrefix\)/u,
    "the surface prefix must be required, not optional",
  );
  assert.match(
    clientSource,
    /must contain \$webserverAppApiPrefix exactly once/u,
    "a duplicated surface prefix must be rejected while the environment is built",
  );
});

test("an unbound catalog port refuses to look like an empty tenant", () => {
  assert.match(
    catalogPortSource,
    /throw const WebserverDeployAppCatalogUnavailableError\(/u,
    "reading an unbound port must throw, never resolve an empty page",
  );
  assert.match(
    catalogPortSource,
    /abstract interface class WebserverDeployAppCatalogReader \{/u,
    "the port must be an interface core can bind, not a concrete transport",
  );
  // Dart is nominal, not structural: the adapter obligation must stay stated.
  assert.match(
    catalogPortSource,
    /Dart is nominal, not structural/u,
    "the adapter seam must be documented, because a generated client cannot satisfy this interface structurally",
  );
});

// --- 13. Closed sets + locale parity ---------------------------------------

const messagesSource = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_applications/lib/src/copy/applications_messages.dart",
);
const kindLabels = extractEnumLabels(
  messagesSource,
  "webserverFlutterApplicationKindLabels",
);
const statusLabels = extractEnumLabels(
  messagesSource,
  "webserverFlutterApplicationStatusLabels",
);
const enMessages = extractDartMessageKeys(
  messagesSource,
  "webserverFlutterApplicationsMessagesEnUs",
);
const zhMessages = extractDartMessageKeys(
  messagesSource,
  "webserverFlutterApplicationsMessagesZhCn",
);

const deploymentsTypesDir = path.join(
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

function membersOf(filePath, typeName) {
  const source = fs.readFileSync(filePath, "utf8");
  const match = new RegExp(`export type ${typeName} =([^;]+);`, "u").exec(source);
  assert.ok(match, `${filePath} must declare ${typeName}`);
  return [...match[1].matchAll(/'([A-Z_]+)'/gu)].map((entry) => entry[1]).sort();
}

test("every closed set mirrors the generated deployments unions exactly", () => {
  const generatedKinds = membersOf(path.join(deploymentsTypesDir, "app-kind.ts"), "AppKind");
  const generatedStatuses = membersOf(path.join(deploymentsTypesDir, "app-status.ts"), "AppStatus");

  assert.deepEqual(
    kindLabels.map((entry) => entry.value).sort(),
    generatedKinds,
    "every generated AppKind member must have exactly one label entry",
  );
  assert.deepEqual(
    statusLabels.map((entry) => entry.value).sort(),
    generatedStatuses,
    "every generated AppStatus member must have exactly one label entry",
  );

  // The port's own closed sets must agree with the label tables, or the screen
  // could render a token the port says cannot exist.
  const portSource = mustExist(
    "packages/sdkwork_webserver_flutter_mobile_core/lib/sdk/webserver_deploy_app_catalog_port.dart",
  );
  assert.deepEqual(
    extractConstStringList(portSource, "webserverDeployAppKinds").sort(),
    generatedKinds,
    "core's AppKind mirror must equal the generated union",
  );
  assert.deepEqual(
    extractConstStringList(portSource, "webserverDeployAppStatuses").sort(),
    generatedStatuses,
    "core's AppStatus mirror must equal the generated union",
  );

  // The webserver-owned application surface is deliberately not consulted: the
  // `deploy_app` entity has one owner (`sdkwork-deployments`), so pinning this
  // root's mirror to a second, retiring authority would keep the duplicate alive
  // (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
});

test("both locale fragments cover the same keys and every label resolves", () => {
  assert.deepEqual(
    [...enMessages].sort(),
    [...zhMessages].sort(),
    "the two locale fragments must cover exactly the same keys",
  );
  assert.ok(enMessages.size > 0);

  for (const label of [...kindLabels, ...statusLabels]) {
    assert.ok(enMessages.has(label.messageKey), `en-US must define ${label.messageKey}`);
    assert.ok(zhMessages.has(label.messageKey), `zh-CN must define ${label.messageKey}`);
  }
  assert.ok(enMessages.has("applications.list.row.unknown"), "the unknown-token fallback must exist");
  assert.ok(enMessages.has("applications.list.unavailable"), "the unbound-transport state must exist");

  // The navigation label a route contributes must resolve too.
  for (const key of ["navigation.applications"]) {
    assert.ok(enMessages.has(key), `en-US must define ${key}`);
    assert.ok(zhMessages.has(key), `zh-CN must define ${key}`);
  }
});

// --- 14. Behavioural: record mapping ---------------------------------------

const mappingSource = mustExist(
  "packages/sdkwork_webserver_flutter_mobile_applications/lib/src/models/application_record_mapping.dart",
);

test("the shipped Dart mapping drops unrenderable rows and never blanks a cell", () => {
  assert.match(
    mappingSource,
    /if \(id\.isEmpty\) \{\s*return null;\s*\}/u,
    "a record with no identity must be dropped rather than rendered blank",
  );
  assert.match(
    mappingSource,
    /final name = record\.name\.isNotEmpty \? record\.name : id;/u,
    "a record with no display name must fall back to its id",
  );
  assert.match(
    mappingSource,
    /platformTargetCount: count > 0 \? count : 0,/u,
    "a negative or non-numeric count must not reach the UI",
  );
  assert.match(
    mappingSource,
    /return fallbackKey;\s*\}/u,
    "a token outside the closed set must resolve to the fallback key, not the raw token",
  );
  // The mapping must stay free of I/O so it is testable without a binding.
  assert.doesNotMatch(
    mappingSource,
    /^\s*import\s+'package:flutter\//gmu,
    "the mapping must not depend on the widget layer",
  );
});

// --- 15. Verification is scheduled, not assumed ----------------------------

/**
 * The behavioural suites cannot run without the Flutter toolchain. This section
 * asserts they exist and still name each behaviour, so a behaviour can never be
 * quietly dropped from the suite that will run once the toolchain lands.
 */
const behaviouralSuites = [
  {
    path: "packages/sdkwork_webserver_flutter_mobile_core/test/app_api_surface_url_test.dart",
    behaviours: [
      "accepts one prefixed surface URL and normalizes trailing slashes",
      "rejects a bare origin that carries no surface prefix",
      "rejects a URL carrying a duplicated surface prefix",
      "rejects a URL carrying a query or a fragment",
      "rejects a non-HTTP(S) scheme",
    ],
  },
  {
    path: "packages/sdkwork_webserver_flutter_mobile_applications/test/applications_mapping_test.dart",
    behaviours: [
      "drops unrenderable records and never leaves a cell blank",
      "projects a row whose labels come from the injected label tables",
      "a token outside the closed set renders the unknown label, not the raw token",
      "both locales cover exactly the same keys",
      "every closed-set label resolves to copy that exists in both locales",
    ],
  },
  {
    path: "packages/sdkwork_webserver_flutter_mobile_shell/test/route_contract_test.dart",
    behaviours: [
      "reports every violation of the route contract",
      "decides route access before any page renders",
      "mounts only named routes and picks the lowest navigation order as home",
    ],
  },
];

test("every behavioural case is committed as a flutter_test and still named", () => {
  const suitePaths = new Set();
  for (const suite of behaviouralSuites) {
    const source = mustExist(suite.path);
    suitePaths.add(suite.path);
    assert.match(
      source,
      /^import 'package:flutter_test\/flutter_test\.dart';/mu,
      `${suite.path} must be a real flutter_test suite`,
    );
    for (const behaviour of suite.behaviours) {
      assert.ok(
        source.includes(`test('${behaviour}'`) ||
          source.includes(`test(\n      '${behaviour}'`),
        `${suite.path} must still declare the behaviour "${behaviour}"`,
      );
    }
  }

  // Every package that ships a test directory must be listed above, so a new
  // suite cannot be added without this gate noticing.
  const discovered = expectedPackages
    .flatMap((name) => listFiles(`packages/${name}/test`))
    .map((filePath) => filePath.replaceAll("\\", "/"))
    .filter((filePath) => filePath.endsWith(".dart"));
  assert.deepEqual(
    discovered.filter((filePath) => !suitePaths.has(filePath)),
    [],
    "every committed Dart test suite must be listed in behaviouralSuites",
  );
});

console.log(
  "flutter surface contract passed (static + source-text + scheduling; Dart is NOT executed by this gate).",
);
