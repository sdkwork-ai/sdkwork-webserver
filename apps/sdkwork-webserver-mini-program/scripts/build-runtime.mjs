/**
 * Mini program runtime build.
 *
 * Two jobs, in this order:
 *
 * 1. **Select one runtime profile.** `config/mini-program/runtime-env.<profileId>.json`
 *    is read, its identity keys are checked against the profile the caller asked
 *    for, and it is rejected if it carries anything secret-shaped. A native mini
 *    program has no environment at device run time, so a profile that does not
 *    match must fail here — at build time — rather than at launch
 *    (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §10).
 * 2. **Emit the runtime.** The route projection is checked against `src/app.json`,
 *    then `src/bootstrap/runtimeBundle.ts` is bundled to `src/runtime/webserver-app.js`
 *    together with the frozen `runtime-env.js` and a build manifest.
 *
 * `--watch` keeps the bundle current for the WeChat devtools preview; `--check`
 * re-renders in memory and fails when the working tree is stale.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

import { projectWebserverMiniProgramRoutes } from "./route-projection.mjs";

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const RUNTIME_DIR = path.join(APP_ROOT, "src", "runtime");
const RUNTIME_BUNDLE_PATH = path.join(RUNTIME_DIR, "webserver-app.js");
const RUNTIME_ENV_PATH = path.join(RUNTIME_DIR, "runtime-env.js");
const BUILD_MANIFEST_PATH = path.join(RUNTIME_DIR, "build-manifest.json");
const ROUTE_PROJECTION_PATH = path.join(RUNTIME_DIR, "route-projection.json");
/**
 * The emitted bundle is CommonJS because the platform `require`s it. The app
 * package is `"type": "module"`, so Node — which is what the contract tests run
 * on — would otherwise refuse to load the very artifact the device loads. This
 * generated marker scopes that directory back to CommonJS and is emitted
 * alongside the bundle so it can never drift.
 */
const RUNTIME_PACKAGE_PATH = path.join(RUNTIME_DIR, "package.json");
const APP_JSON_PATH = path.join(APP_ROOT, "src", "app.json");
/** The bundle is much larger than this once the SDK clients are linked in. */
const MIN_BUNDLE_BYTES = 10_000;

/**
 * This repository is standalone-only (`SDKWORK_WEBSERVER_SPEC.md` §17.4), and the
 * app manifest declares `runtime.supportedDeploymentProfiles = ["standalone"]`, so
 * the vocabulary is closed here too — a `cloud` flag fails on the vocabulary
 * instead of on a missing file.
 */
const DEPLOYMENT_PROFILES = ["standalone"];
const ENVIRONMENTS = ["development", "test", "staging", "demo", "production"];
export const RUNTIME_TARGET = "mini-program";
export const PLATFORM = "MP_WEIXIN";

const IDENTITY_KEYS = {
  SDKWORK_DEPLOYMENT_PROFILE: "deploymentProfile",
  SDKWORK_ENVIRONMENT: "environment",
  SDKWORK_PROFILE_ID: "profileId",
  SDKWORK_RUNTIME_TARGET: "runtimeTarget",
};

const BASE_URL_KEYS = [
  "SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL",
  "SDKWORK_WEBSERVER_APP_API_BASE_URL",
  "SDKWORK_WEBSERVER_DEPLOY_APP_API_BASE_URL",
  "SDKWORK_WEBSERVER_DRIVE_APP_API_BASE_URL",
];

/**
 * A mini program bundle is uploaded to a third-party platform, so the profile is
 * scanned for credential-shaped keys before it is embedded. Reference-style keys
 * (`...Reference`, `...Path`, `...Ref`) are allowed on purpose — that is how a
 * profile points at a secret without containing one.
 */
const FORBIDDEN_KEY = /(?:password|private[_-]?key|signing[_-]?secret|access[_-]?token|refresh[_-]?token|api[_-]?key|secret)$/iu;
const SAFE_KEY_REFERENCE = /(?:file|path|ref|reference)$/iu;

function fail(message) {
  console.error(`[sdkwork-webserver-mini-program] ${message}`);
  process.exitCode = 1;
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

export function resolveRuntimeProfilePath(deploymentProfile, environment) {
  return path.join(
    APP_ROOT,
    "config",
    "mini-program",
    `runtime-env.${deploymentProfile}.${environment}.json`,
  );
}

export function validateRuntimeProfile(profileId) {
  const [deploymentProfile, environment] = profileId.split(".");
  if (!DEPLOYMENT_PROFILES.includes(deploymentProfile)) {
    throw new Error(`--deployment-profile must be one of ${DEPLOYMENT_PROFILES.join(", ")}`);
  }
  if (!ENVIRONMENTS.includes(environment)) {
    throw new Error(`--environment must be one of ${ENVIRONMENTS.join(", ")}`);
  }
  const profilePath = resolveRuntimeProfilePath(deploymentProfile, environment);
  if (!existsSync(profilePath)) {
    throw new Error(`mini program runtime profile does not exist: ${relativeToApp(profilePath)}`);
  }
  const source = readJson(profilePath);
  const expected = {
    SDKWORK_DEPLOYMENT_PROFILE: deploymentProfile,
    SDKWORK_ENVIRONMENT: environment,
    SDKWORK_PROFILE_ID: profileId,
    SDKWORK_RUNTIME_TARGET: RUNTIME_TARGET,
  };
  for (const [key, value] of Object.entries(expected)) {
    if (source[key] !== value) {
      throw new Error(`${profileId} must declare ${key}=${value}`);
    }
  }
  for (const key of Object.keys(source)) {
    if (FORBIDDEN_KEY.test(key) && !SAFE_KEY_REFERENCE.test(key)) {
      throw new Error(`${profileId} must not carry a secret value (${key})`);
    }
  }
  for (const key of BASE_URL_KEYS) {
    const value = source[key];
    if (typeof value !== "string" || value.length === 0) {
      throw new Error(`${profileId} must declare ${key}`);
    }
    const url = new URL(value);
    if (url.origin !== value.replace(/\/$/u, "")) {
      throw new Error(`${profileId}.${key} must be a bare origin with no path: ${value}`);
    }
    if (environment === "production" && ["localhost", "127.0.0.1", "::1"].includes(url.hostname)) {
      throw new Error(`${profileId}.${key} cannot use a loopback host in production`);
    }
  }
  return { deploymentProfile, environment, profileId, profilePath, source };
}

/**
 * The authored `src/app.json` carries the window chrome; the pages list is
 * derived. Comparing them here is what makes "declared but not published" and
 * "published but not declared" both hard build failures
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §12 route projection).
 */
function assertAppJsonMatchesProjection(projection) {
  const appJson = readJson(APP_JSON_PATH);
  if (projection.issues.length > 0) {
    throw new Error(`route contributions are invalid: ${projection.issues.join("; ")}`);
  }
  const declared = Array.isArray(appJson.pages) ? appJson.pages : [];
  const projected = projection.pages;
  if (JSON.stringify(declared) !== JSON.stringify(projected)) {
    throw new Error(
      `src/app.json#pages must equal the projected root pages (${projected.join(", ")}); found ${declared.join(", ")}`,
    );
  }
  if (projection.subPackages.length > 0) {
    const subPackages = Array.isArray(appJson.subPackages) ? appJson.subPackages : [];
    if (JSON.stringify(subPackages.map((entry) => entry.root)) !== JSON.stringify(
      projection.subPackages.map((entry) => entry.root),
    )) {
      throw new Error("src/app.json#subPackages must equal the projected subpackages");
    }
  }
}

export function relativeToApp(target) {
  return path.relative(APP_ROOT, target).replaceAll("\\", "/");
}

/** One definition of the bundle, shared by the build, the check, and the watcher. */
function esbuildOptions(options = {}) {
  return {
    entryPoints: [path.join(APP_ROOT, "src", "bootstrap", "runtimeBundle.ts")],
    outfile: RUNTIME_BUNDLE_PATH,
    bundle: true,
    platform: "browser",
    format: "cjs",
    target: "es2019",
    minifySyntax: true,
    minifyWhitespace: true,
    minifyIdentifiers: false,
    legalComments: "none",
    logLevel: options.logLevel ?? "warning",
    ...options.extra,
  };
}

/** Same build, kept in memory — this is what `--check` compares against. */
async function renderBundle(options) {
  const result = await esbuild.build({ ...esbuildOptions(options), write: false });
  return result.outputFiles[0].text;
}

function assertBundleIsComplete(source) {
  if (source.length < MIN_BUNDLE_BYTES) {
    throw new Error(
      `runtime bundle looks truncated (${source.length} bytes); expected at least ${MIN_BUNDLE_BYTES}`,
    );
  }
  if (!source.includes("module.exports")) {
    throw new Error(
      "runtime bundle must be CommonJS: the platform require()s it and the contract tests load it in Node",
    );
  }
  return source.length;
}

function readIfExists(filePath) {
  return existsSync(filePath) ? readFileSync(filePath, "utf8") : null;
}

/**
 * Everything except the bundle is a pure function of the profile and the route
 * projection, so `--check` can render it all and diff against the working tree.
 */
function renderStaticArtifacts(profile, manifest, projection) {
  return new Map([
    [RUNTIME_PACKAGE_PATH, `${JSON.stringify({ type: "commonjs" }, null, 2)}\n`],
    [RUNTIME_ENV_PATH, `module.exports = ${JSON.stringify(profile.source, null, 2)};\n`],
    [BUILD_MANIFEST_PATH, `${JSON.stringify(manifest, null, 2)}\n`],
    [ROUTE_PROJECTION_PATH, `${JSON.stringify(projection, null, 2)}\n`],
  ]);
}

async function main() {
  const { values } = parseArgs({
    args: process.argv.slice(2),
    options: {
      "deployment-profile": { type: "string", default: "standalone" },
      environment: { type: "string", default: "development" },
      check: { type: "boolean", default: false },
      watch: { type: "boolean", default: false },
      "print-host-open-hint": { type: "boolean", default: false },
    },
    strict: true,
  });

  const deploymentProfile = values["deployment-profile"];
  const environment = values.environment;
  const profileId = `${deploymentProfile}.${environment}`;

  const profile = validateRuntimeProfile(profileId);
  const projection = await projectWebserverMiniProgramRoutes(APP_ROOT, {
    logLevel: values.watch ? "info" : "silent",
  });
  assertAppJsonMatchesProjection(projection);

  const manifest = {
    deploymentProfile,
    environment,
    profileId,
    runtimeTarget: RUNTIME_TARGET,
    platform: PLATFORM,
    bundle: relativeToApp(RUNTIME_BUNDLE_PATH),
    pages: projection.pages,
    routeIds: projection.routeIds,
  };

  if (values.watch) {
    const context = await esbuild.context(
      esbuildOptions({ logLevel: "info", extra: { write: true } }),
    );
    mkdirSync(RUNTIME_DIR, { recursive: true });
    await context.watch();
    writeRuntimeArtifacts(profile, manifest, projection);
    console.log(
      `[sdkwork-webserver-mini-program] watching ${profileId}; open ${relativeToApp(APP_ROOT)} in WeChat devtools (miniprogramRoot = src/)`,
    );
    return;
  }

  const staticArtifacts = renderStaticArtifacts(profile, manifest, projection);
  const bundleSource = await renderBundle({});

  if (values.check) {
    const stale = [];
    if (readIfExists(RUNTIME_BUNDLE_PATH) !== bundleSource) {
      stale.push(relativeToApp(RUNTIME_BUNDLE_PATH));
    }
    for (const [filePath, expected] of staticArtifacts) {
      if (readIfExists(filePath) !== expected) {
        stale.push(relativeToApp(filePath));
      }
    }
    if (stale.length > 0) {
      throw new Error(
        `runtime build is stale (${stale.join(", ")}); run pnpm run build:mini-program --deployment-profile ${deploymentProfile} --environment ${environment}`,
      );
    }
    assertBundleIsComplete(bundleSource);
    console.log(`[sdkwork-webserver-mini-program] runtime build current: ${profileId}`);
    return;
  }

  const bundleBytes = assertBundleIsComplete(bundleSource);
  writeRuntimeArtifacts(profile, manifest, projection);
  writeFileSync(RUNTIME_BUNDLE_PATH, bundleSource, "utf8");
  if (values["print-host-open-hint"]) {
    console.log(
      `[sdkwork-webserver-mini-program] open ${relativeToApp(APP_ROOT)} in WeChat devtools; miniprogramRoot = src/`,
    );
  }
  console.log(
    `[sdkwork-webserver-mini-program] built ${profileId} -> ${relativeToApp(RUNTIME_BUNDLE_PATH)} (${bundleBytes} bytes, ${projection.pages.length} page(s))`,
  );
}

function writeRuntimeArtifacts(profile, manifest, projection) {
  mkdirSync(RUNTIME_DIR, { recursive: true });
  for (const [filePath, content] of renderStaticArtifacts(profile, manifest, projection)) {
    writeFileSync(filePath, content, "utf8");
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
}
