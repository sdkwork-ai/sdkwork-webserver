/**
 * Config boundary contract.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §10/§12: every runtime profile declares
 * its own identity, a build contains no credential, and the host templates carry
 * platform ids and package settings only. These assertions run on the committed
 * files, so a profile that drifts (or accidentally gains a credential) fails in CI
 * rather than at upload time.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEPLOYMENT_CONFIG_PATH = path.join(APP_ROOT, "etc", "sdkwork.deployment.config.json");
const PROFILES = ["standalone.development", "standalone.test", "standalone.staging", "standalone.demo", "standalone.production"];
const HOST_ENVIRONMENTS = ["development", "test", "staging", "production"];
const BASE_URL_KEYS = [
  "SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL",
  "SDKWORK_WEBSERVER_APP_API_BASE_URL",
  "SDKWORK_WEBSERVER_DEPLOY_APP_API_BASE_URL",
  "SDKWORK_WEBSERVER_DRIVE_APP_API_BASE_URL",
];
const FORBIDDEN_KEY = /(?:password|private[_-]?key|signing[_-]?secret|access[_-]?token|refresh[_-]?token|api[_-]?key|secret)$/iu;
const SAFE_KEY_REFERENCE = /(?:file|path|ref|reference)$/iu;

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(APP_ROOT, relativePath), "utf8"));
}

function profilePath(profileId) {
  return `config/mini-program/runtime-env.${profileId}.json`;
}

test("every declared runtime profile exists with the canonical identity", () => {
  for (const profileId of PROFILES) {
    const source = readJson(profilePath(profileId));
    const [deploymentProfile, environment] = profileId.split(".");
    assert.equal(source.SDKWORK_DEPLOYMENT_PROFILE, deploymentProfile, `${profileId} profile`);
    assert.equal(source.SDKWORK_ENVIRONMENT, environment, `${profileId} environment`);
    assert.equal(source.SDKWORK_PROFILE_ID, profileId, `${profileId} id`);
    assert.equal(source.SDKWORK_RUNTIME_TARGET, "mini-program", `${profileId} runtime target`);
    // The scoped identity is what a future topology-driven materializer writes;
    // keeping it present means switching drivers does not change the bundle shape.
    assert.equal(source.SDKWORK_WEBSERVER_PROFILE_ID, profileId, `${profileId} scoped id`);
    assert.equal(source.SDKWORK_WEBSERVER_RUNTIME_TARGET, "mini-program", `${profileId} scoped target`);
  }
});

test("every runtime profile declares bare absolute origins", () => {
  for (const profileId of PROFILES) {
    const source = readJson(profilePath(profileId));
    for (const key of BASE_URL_KEYS) {
      const value = source[key];
      assert.equal(typeof value, "string", `${profileId}.${key} must be a string`);
      const url = new URL(value);
      assert.ok(["http:", "https:"].includes(url.protocol), `${profileId}.${key} scheme`);
      assert.equal(url.origin, value.replace(/\/$/u, ""), `${profileId}.${key} must be a bare origin`);
      assert.equal(url.username, "", `${profileId}.${key} must not carry credentials`);
    }
  }
});

test("production profiles never point at a loopback host", () => {
  const source = readJson(profilePath("standalone.production"));
  for (const key of BASE_URL_KEYS) {
    const { hostname } = new URL(source[key]);
    assert.ok(!["localhost", "127.0.0.1", "::1"].includes(hostname), `${key} uses ${hostname}`);
  }
});

test("no runtime profile or host template carries a secret-shaped key", () => {
  const files = [
    ...PROFILES.map(profilePath),
    ...HOST_ENVIRONMENTS.map((environment) => `config/host/mp-weixin.${environment}.example.json`),
  ];
  for (const relativePath of files) {
    assert.ok(fs.existsSync(path.join(APP_ROOT, relativePath)), `${relativePath} must exist`);
    for (const key of Object.keys(readJson(relativePath))) {
      assert.ok(
        !FORBIDDEN_KEY.test(key) || SAFE_KEY_REFERENCE.test(key),
        `${relativePath} must not carry secret key ${key}`,
      );
    }
  }
});

test("host templates declare platform identity without an endpoint", () => {
  for (const environment of HOST_ENVIRONMENTS) {
    const source = readJson(`config/host/mp-weixin.${environment}.example.json`);
    assert.equal(source.platform, "mp-weixin");
    assert.equal(source.environment, environment);
    assert.equal(source.deploymentProfile, "standalone");
    assert.equal(typeof source.appid, "string");
    assert.ok(source.appid.length > 0, "host template must declare an appid");
    // No business endpoint, private host, or SDK package fact belongs here.
    const serialized = JSON.stringify(source);
    assert.doesNotMatch(serialized, /https?:\/\//u, "host template must not hardcode an endpoint");
    assert.doesNotMatch(serialized, /\/app\/v3\/api/u, "host template must not hardcode an API path");
  }
});

test("the WeChat project publishes src/ as the mini program root", () => {
  const project = readJson("project.config.json");
  assert.equal(project.miniprogramRoot, "src/");
  assert.equal(project.compileType, "miniprogram");
  assert.equal(project.setting.urlCheck, false, "devtools must be able to reach a local origin");
});

test("src/app.json publishes the projected pages and declares window chrome", () => {
  const appJson = readJson("src/app.json");
  assert.ok(Array.isArray(appJson.pages) && appJson.pages.length > 0);
  for (const page of appJson.pages) {
    assert.ok(
      fs.existsSync(path.join(APP_ROOT, "src", `${page}.js`)),
      `declared page ${page} must have a native implementation`,
    );
  }
  assert.equal(typeof appJson.window.navigationBarTitleText, "string");
});

test("the component deployment config delegates to the parent and mirrors its profile matrix", () => {
  const deployment = readJson("etc/sdkwork.deployment.config.json");
  assert.equal(deployment.kind, "sdkwork.component-deployment");
  assert.equal(deployment.runtimeTarget, "mini-program");
  assert.ok(
    !fs.existsSync(path.join(APP_ROOT, "specs", "topology.spec.json")),
    "runtime topology is parent-owned",
  );
  // The lifecycle framework resolves both parent pointers against `etc/`, i.e.
  // `path.resolve(root, "etc", parentDeploymentConfig)`.
  assert.ok(
    fs.existsSync(path.resolve(APP_ROOT, "etc", deployment.parentDeploymentConfig)),
    "the parent deployment config must resolve from etc/",
  );
  assert.ok(
    fs.existsSync(path.resolve(APP_ROOT, "etc", deployment.parentTopologySpec)),
    "the parent topology spec must resolve from etc/",
  );
  assert.equal(deployment.materialization?.format, "mini-program-json");
  assert.equal(deployment.materialization?.checkMode, "--check");
  assert.deepEqual(
    [...(deployment.materialization?.profiles ?? [])].sort(),
    [...PROFILES].sort(),
    "the declared profile matrix must equal the materialized set",
  );
  for (const profileId of PROFILES) {
    const [deploymentProfile, environment] = profileId.split(".");
    const pattern = deployment.materialization.inputPattern
      .replace("{deploymentProfile}", deploymentProfile)
      .replace("{environment}", environment);
    assert.ok(
      fs.existsSync(path.join(APP_ROOT, pattern)),
      `materialization.inputPattern must resolve for ${profileId} (${pattern})`,
    );
  }
});

/**
 * The profiles are authored in this app while their authority is the repository
 * deployment index. Without a shared materializer the values are copied, so the
 * copy is checked here instead: an origin that drifts in either file fails CI.
 */
test("every runtime profile agrees with the repository deployment index", () => {
  const deployment = readJson("etc/sdkwork.deployment.config.json");
  const index = JSON.parse(
    fs.readFileSync(path.resolve(APP_ROOT, "etc", deployment.parentDeploymentConfig), "utf8"),
  );
  assert.equal(index.kind, "sdkwork.deployment-index");
  assert.deepEqual(
    [...Object.keys(index.profiles)].sort(),
    [...PROFILES].sort(),
    "this app must cover exactly the repository profile matrix",
  );
  for (const profileId of PROFILES) {
    const environment = profileId.split(".")[1];
    const origin = index.environments[environment]?.applicationOrigin;
    assert.equal(typeof origin, "string", `the index must declare ${environment}.applicationOrigin`);
    const source = readJson(profilePath(profileId));
    assert.equal(
      source.SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL,
      origin,
      `${profileId} public URL must equal the deployment index origin`,
    );
    // Standalone is same-origin (SDKWORK_WEBSERVER_SPEC.md §17.4): the browser
    // app, the API, and the imported surfaces are all served by one edge.
    for (const key of BASE_URL_KEYS) {
      assert.equal(source[key], origin, `${profileId}.${key} must be the same origin`);
    }
  }
});
