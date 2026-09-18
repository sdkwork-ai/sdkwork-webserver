/**
 * Route projection contract.
 *
 * `MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §9/§12: route ids are canonical,
 * placement metadata is complete, and the platform page list is a projection of
 * the SDKWork route contributions rather than a second hand-maintained source.
 *
 * The projection is computed by the same module the build uses, so a passing test
 * means the shipped `src/app.json` is the projected one — not a copy that happens
 * to look right today.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { projectWebserverMiniProgramRoutes } from "../scripts/route-projection.mjs";

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ROUTE_ID_PATTERN = /^app\.[a-z0-9-]+\.[a-z0-9-]+\.[a-z0-9-]+$/u;

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(APP_ROOT, relativePath), "utf8"));
}

test("route contributions are valid and project onto src/app.json pages", async () => {
  const projection = await projectWebserverMiniProgramRoutes(APP_ROOT);

  assert.deepEqual(projection.issues, [], "route contributions must validate");
  assert.ok(projection.pages.length > 0, "at least one root page is required");
  assert.deepEqual(
    projection.pages,
    readJson("src/app.json").pages,
    "src/app.json#pages must equal the projected root pages",
  );
});

test("every route id follows <surface>.<domain>.<capability>.<screen>", async () => {
  const projection = await projectWebserverMiniProgramRoutes(APP_ROOT);
  assert.ok(projection.routeIds.length > 0);
  for (const routeId of projection.routeIds) {
    assert.match(routeId, ROUTE_ID_PATTERN, `route id ${routeId} must use the canonical shape`);
    assert.equal(new Set(projection.routeIds).size, projection.routeIds.length, "route ids must be unique");
  }
});

test("the console launches from the root package with nothing preloaded from a subpackage", async () => {
  const projection = await projectWebserverMiniProgramRoutes(APP_ROOT);
  const rootEntries = projection.entries.filter((entry) => entry.rootPackage);
  assert.equal(rootEntries.length, 1, "exactly one first-screen page belongs to the root package");
  assert.equal(rootEntries[0].pagePath, projection.pages[0], "the projected order starts with the root page");
  assert.deepEqual(projection.subPackages, [], "the current single-capability console needs no subpackage");
});

test("navigation is ordered, permission-gated, and addressed by route id", async () => {
  const projection = await projectWebserverMiniProgramRoutes(APP_ROOT);
  assert.ok(projection.navigation.length > 0, "the console must contribute at least one navigation entry");
  const orders = projection.navigation.map((entry) => entry.order);
  assert.deepEqual(orders, [...orders].sort((left, right) => left - right), "navigation must be order-sorted");
  for (const entry of projection.navigation) {
    assert.ok(projection.routeIds.includes(entry.id), `navigation ${entry.id} must resolve to a route`);
    assert.match(entry.path, /^\/pages\/[a-z0-9-]+\/index$/u, `navigation path ${entry.path}`);
    assert.ok(entry.permission.length > 0, `navigation ${entry.id} must declare a permission`);
    assert.match(entry.labelKey, /^[a-z][a-zA-Z0-9.]*$/u, `navigation ${entry.id} label key`);
  }
});

test("route metadata declares no transport detail", () => {
  const source = fs.readFileSync(
    path.join(
      APP_ROOT,
      "packages/sdkwork-webserver-mp-applications/src/routes/routeContributions.ts",
    ),
    "utf8",
  );
  assert.doesNotMatch(source, /https?:\/\//u, "route metadata must not declare an API URL");
  assert.doesNotMatch(source, /\/app\/v3\/api/u, "route metadata must not declare an API path");
  assert.doesNotMatch(source, /\bclient\./u, "route metadata must not declare an SDK method");
});

/**
 * H5 parity.
 *
 * The H5 registry composes its id from four exported constants instead of
 * spelling the id out, so a literal substring search would prove nothing about
 * it; the id is rebuilt from the same constants H5 exports, which means a drift
 * in any one segment fails this test. PC is not compared: the PC renderer does
 * not declare an applications route, it bridges the deployments console package.
 */
test("the H5 root agrees on the canonical route id and permission hint", async () => {
  const projection = await projectWebserverMiniProgramRoutes(APP_ROOT);
  const repoRoot = path.resolve(APP_ROOT, "..", "..");
  const h5RouteRegistryPath = path.join(
    repoRoot,
    "apps/sdkwork-webserver-h5/packages/sdkwork-webserver-h5-shell",
    "src/navigation/routeRegistry.ts",
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

  for (const routeId of projection.routeIds) {
    assert.equal(
      h5RouteId,
      routeId,
      "the H5 root and the mini program root must agree on the canonical route id",
    );
  }
  for (const entry of projection.navigation) {
    assert.ok(
      h5PermissionHints.includes(entry.permission),
      `the H5 root must gate ${entry.id} on ${entry.permission}`,
    );
  }
});
