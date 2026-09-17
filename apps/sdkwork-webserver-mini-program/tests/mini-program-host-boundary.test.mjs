/**
 * Host boundary contract.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8/§12: platform globals live behind the
 * `mp-host` adapters and nowhere else, capability pages do not build transports,
 * and a page reaches the packages only through the runtime bundle. These are
 * static scans because the failure mode is an import that *compiles* — a page that
 * calls `wx.request` directly would work on a device and silently bypass the
 * adapter, the token manager, and the error vocabulary.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PACKAGES_DIR = path.join(APP_ROOT, "packages");
const HOST_PACKAGE_DIR = "sdkwork-webserver-mp-host";
const SOURCE_EXTENSIONS = new Set([".ts", ".tsx", ".js", ".mjs"]);
/**
 * A platform *call* (`wx.getStorageSync(`), not any occurrence of the two letters:
 * matching bare `wx` would trip over unrelated identifiers and make the guard
 * noise instead of a signal.
 */
const PLATFORM_GLOBAL_PATTERN = /\b(?:wx|my|dd|tt)\s*\.\s*[a-zA-Z_$][a-zA-Z0-9_$]*\s*\(|\b(?:getApp|Page|App|Component|Behavior)\s*\(/u;
const RAW_HTTP_PATTERN = /\bfetch\s*\(|\baxios\s*\.|\bky\s*\.|\bgot\s*\(|new\s+XMLHttpRequest\s*\(/u;
const ABSOLUTE_HOST_PATTERN = /https?:\/\/[a-z0-9.-]+/iu;

function walk(directory) {
  const files = [];
  if (!fs.existsSync(directory)) return files;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "dist") continue;
      files.push(...walk(absolute));
    } else if (entry.isFile() && SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
      files.push(absolute);
    }
  }
  return files;
}

function relative(absolute) {
  return path.relative(APP_ROOT, absolute).replaceAll("\\", "/");
}

test("capability and core packages never call a platform global", () => {
  const offenders = [];
  for (const file of walk(PACKAGES_DIR)) {
    const relativePath = relative(file);
    // The host package is the one place a platform global is allowed to appear.
    if (relativePath.includes(`/packages/${HOST_PACKAGE_DIR}/`)) continue;
    const source = fs.readFileSync(file, "utf8");
    if (PLATFORM_GLOBAL_PATTERN.test(source)) {
      offenders.push(relativePath);
    }
  }
  assert.deepEqual(offenders, [], `platform globals must stay inside ${HOST_PACKAGE_DIR}`);
});

test("capability packages build no transport of their own", () => {
  const offenders = [];
  for (const file of walk(path.join(PACKAGES_DIR, "sdkwork-webserver-mp-applications"))) {
    const source = fs.readFileSync(file, "utf8");
    if (RAW_HTTP_PATTERN.test(source)) {
      offenders.push(relative(file));
    }
    assert.doesNotMatch(
      source,
      /from\s+["']@sdkwork\/[a-z0-9-]+-(?:app|backend)-sdk["']/u,
      `${relative(file)} must consume the injected client, not a generated SDK package`,
    );
  }
  assert.deepEqual(offenders, [], "the capability package must not build a transport");
});

test("native pages bind the runtime bundle and nothing else", () => {
  for (const file of walk(path.join(APP_ROOT, "src", "pages"))) {
    const source = fs.readFileSync(file, "utf8");
    const relativePath = relative(file);
    assert.doesNotMatch(source, RAW_HTTP_PATTERN, `${relativePath} must not call raw HTTP`);
    assert.doesNotMatch(source, ABSOLUTE_HOST_PATTERN, `${relativePath} must not hardcode an endpoint`);
    const requires = [...source.matchAll(/require\(\s*["']([^"']+)["']\s*\)/gu)].map((match) => match[1]);
    for (const specifier of requires) {
      assert.match(
        specifier,
        /^\.\.\/\.\.\/runtime\/[a-z-]+$/u,
        `${relativePath} may only require the runtime bundle, found ${specifier}`,
      );
    }
  }
});

test("the bundle never embeds a selected profile's origin", () => {
  const bundlePath = path.join(APP_ROOT, "src", "runtime", "webserver-app.js");
  assert.ok(fs.existsSync(bundlePath), "run `pnpm build:mini-program` before this test");
  const bundle = fs.readFileSync(bundlePath, "utf8");
  const profile = JSON.parse(
    fs.readFileSync(
      path.join(APP_ROOT, "config/mini-program/runtime-env.standalone.development.json"),
      "utf8",
    ),
  );
  for (const key of [
    "SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL",
    "SDKWORK_WEBSERVER_APP_API_BASE_URL",
    "SDKWORK_WEBSERVER_DEPLOY_APP_API_BASE_URL",
    "SDKWORK_WEBSERVER_DRIVE_APP_API_BASE_URL",
  ]) {
    assert.ok(
      !bundle.includes(profile[key]),
      `the bundle must read ${key} from the runtime profile, not embed it`,
    );
  }
});
