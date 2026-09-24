import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

/**
 * The VM Instances page is served from this edge, but the routes and the
 * permission codes they demand belong to `sdkwork-sandbox`. Two artifacts have
 * to agree across the repo boundary, and nothing else reads them together:
 *
 *  - the route manifest in `sdkwork-sandbox` fixes which code each route
 *    requires, and `sdkwork-web-core` enforces it on every request;
 *  - `specs/iam.module.manifest.json` here is the `web` domain's catalog, and a
 *    code that is not materialized from a catalog is not an assignable
 *    permission at all (`IAM_SPEC` section 543: runtime authorization uses
 *    permission catalog materialization, not the bootstrap token scope).
 *
 * So a code that drifts out of the catalog does not fail a build, a type check,
 * or a render — it turns every console operator's read into a 403, which the
 * page then honestly paints as "unavailable". Hence the cross-repo assertions.
 */

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const WORKSPACE_ROOT = path.resolve(REPO_ROOT, '..');
const SANDBOX_ROUTE_SRC = path.join(
  WORKSPACE_ROOT,
  'sdkwork-sandbox/crates/sdkwork-routes-sandbox-app-api/src',
);

function read(file) {
  return readFileSync(file, 'utf8');
}

function readJson(file) {
  return JSON.parse(read(file));
}

/** `pub const NAME: &str = "code";` → `{ NAME: "code" }`. */
function declaredCodes(source) {
  const codes = {};
  for (const match of source.matchAll(/pub const ([A-Z0-9_]+): &str = "([^"]+)";/gu)) {
    codes[match[1]] = match[2];
  }
  return codes;
}

/**
 * Every permission constant the route table actually demands.
 *
 * The helper `sandbox_route(..)` takes the permission as its parameter and its
 * body only ever names `permission`, so the constants that matter are the ones
 * each *call site* passes. Scanning the call sites (rather than the helper) is
 * what makes this read the real table.
 */
function demandedConstantNames(source) {
  const table = source.match(/const HTTP_ROUTES: &\[HttpRoute\] = &\[([\s\S]*?)\n\];/u);
  assert.ok(table, 'HTTP_ROUTES must be a static route table');

  const calls = [...table[1].matchAll(/sandbox_route\(([\s\S]*?)\),/gu)];
  assert.ok(calls.length > 0, 'the route table must declare routes through sandbox_route');

  const names = [];
  for (const call of calls) {
    const named = [...call[1].matchAll(/\bPERM_[A-Z0-9_]+\b/gu)].map((match) => match[0]);
    assert.equal(
      named.length,
      1,
      `each sandbox_route call must name exactly one PERM_ constant, found ${named.length}`,
    );
    names.push(named[0]);
  }
  return [...new Set(names)];
}

const constants = declaredCodes(read(path.join(SANDBOX_ROUTE_SRC, 'permissions.rs')));
const demanded = demandedConstantNames(read(path.join(SANDBOX_ROUTE_SRC, 'http_route_manifest.rs')));
const manifest = readJson(path.join(REPO_ROOT, 'specs/iam.module.manifest.json'));
const appConfig = readJson(path.join(REPO_ROOT, 'sdkwork.app.config.json'));
const catalog = new Map(manifest.permissions.catalog.map((entry) => [entry.code, entry]));

/** The code behind a demanded constant, with the extraction itself checked. */
function codeOf(constantName) {
  const code = constants[constantName];
  assert.ok(code, `${constantName} must be declared in the sandbox route permissions.rs`);
  return code;
}

test('every permission the sandbox routes demand is materialized in the web catalog', () => {
  assert.ok(demanded.length > 0, 'the sandbox route manifest must guard its routes');
  // The control for the two regexes above: if either stopped matching, `demanded`
  // would be empty and every loop below would pass without examining anything.
  assert.ok(Object.keys(constants).length > 0, 'permissions.rs must declare permission constants');

  for (const constantName of demanded) {
    const code = codeOf(constantName);
    assert.ok(
      catalog.has(code),
      `route permission ${code} is demanded by sdkwork-sandbox but not in the web catalog, ` +
        'so no role can ever be granted it',
    );
  }

  // The other direction: a constant no route names is a permission this module
  // publishes but never enforces, which reads as a working guard that is not one.
  for (const constantName of Object.keys(constants)) {
    assert.ok(
      demanded.includes(constantName),
      `${constantName} is declared but no sandbox route demands it`,
    );
  }
});

test('the catalog a sandbox route resolves against is the web domain catalog', () => {
  // `IAM_MODULE_MANIFEST_SPEC` section 1: `domain` must match the permission code
  // prefix for every permission in the module. Resolving `web.sandbox.read` only
  // works while this file is the catalog that owns the `web` prefix.
  assert.equal(manifest.moduleId, 'web');
  assert.equal(manifest.domain, 'web');
  assert.equal(manifest.owner, 'sdkwork-webserver');
  for (const entry of manifest.permissions.catalog) {
    assert.ok(
      entry.code.startsWith(`${manifest.domain}.`),
      `${entry.code} is outside the module's own domain`,
    );
  }

  for (const constantName of demanded) {
    const entry = catalog.get(codeOf(constantName));
    assert.equal(entry.status, 'active', `${entry.code} must be an active permission`);
    assert.ok(entry.name.length > 0, `${entry.code} must carry a display name`);
    assert.equal(entry.replacementCode, null, `${entry.code} is active, so it replaces nothing`);
  }
});

test('a plain console user is granted the sandbox permissions, not told to ask', () => {
  const appUser = manifest.roles.roleGrantExtensions.find((entry) => entry.roleCode === 'app_user');
  assert.ok(appUser, 'app_user must carry a role grant extension');

  // The product statement is that every account may provision its own sandbox, and
  // ownership is decided server-side from the verified principal — so the console
  // role is the one that needs the grant. Without it the page is reachable but
  // every request is refused, which is indistinguishable from an outage.
  const grants = (code) =>
    appUser.patterns.some(
      (pattern) =>
        pattern === code ||
        pattern === `${manifest.domain}.*` ||
        (pattern.endsWith('.*') && code.startsWith(pattern.slice(0, -1))),
    );

  for (const constantName of demanded) {
    const code = codeOf(constantName);
    assert.ok(grants(code), `${code} must be granted to app_user`);
  }
});

test('the bootstrap access token scope is not used as the sandbox RBAC surface', () => {
  const scope = appConfig.backend?.accessTokenPermissionScope ?? [];

  // `IAM_SPEC` section 543 and `IAM_APPLICATION_BOOTSTRAP_SPEC` section 167: the
  // bootstrap scope gates credential entry and pre-login transport, must stay
  // minimal, and must not duplicate module catalogs. Registering the sandbox codes
  // there instead of in the catalog looks like it works in a hand-driven session
  // while leaving the permission unassignable through IAM — and it is the obvious
  // wrong turn for whoever next hits a 403 on this page.
  for (const constantName of demanded) {
    const code = codeOf(constantName);
    assert.ok(
      !scope.includes(code),
      `${code} belongs in the permission catalog, not backend.accessTokenPermissionScope`,
    );
  }
});

test('this contract runs on the workspace gate chain', () => {
  const packageJson = readJson(path.join(REPO_ROOT, 'package.json'));

  assert.match(packageJson.scripts['test:contracts'], /tests\/contract\/\*\.contract\.test\.mjs/u);
  assert.match(packageJson.scripts['_sdkwork:test'], /pnpm test:contracts/u);
});
