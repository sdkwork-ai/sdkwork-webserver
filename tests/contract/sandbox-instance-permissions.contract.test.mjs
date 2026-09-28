import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

/**
 * The VM Instances console page is hosted by this edge but served by
 * `sdkwork-sandbox`'s own product API (`appApiBaseUrl`), and the permission
 * codes it cares about cross the repo boundary. The 2026-09 sandbox
 * restructuring retired the sandbox app-api crate entirely: the only route
 * plane the sibling publishes is the machine-only internal API, whose routes
 * are ingress-token gated and declare no per-route IAM permission codes at
 * all. Three artifacts therefore have to agree, and nothing else reads them
 * together:
 *
 *  - the sibling's route manifests must keep demanding no `web.*` permission
 *    code. A sandbox route that starts enforcing a permission code again
 *    silently reintroduces the old 403-on-every-request failure this test
 *    exists for, because nothing on this side of the boundary would know to
 *    re-materialize the catalog;
 *  - `specs/iam.module.manifest.json` here is the `web` domain's catalog, and
 *    a code that is not materialized from a catalog is not an assignable
 *    permission at all (`IAM_SPEC` section 543: runtime authorization uses
 *    permission catalog materialization, not the bootstrap token scope). The
 *    console menu's visibility gate (`web.sandbox.read`) is the code's one
 *    remaining consumer, so it must stay active and grantable;
 *  - `sdkwork.app.config.json`'s bootstrap access token scope must stay
 *    minimal and must not duplicate module catalogs.
 */

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const WORKSPACE_ROOT = path.resolve(REPO_ROOT, '..');
const SANDBOX_CRATES = path.join(WORKSPACE_ROOT, 'sdkwork-sandbox/crates');
const SANDBOX_MENU_CODE_PREFIX = 'web.sandbox.';

function read(file) {
  return readFileSync(file, 'utf8');
}

function readJson(file) {
  return JSON.parse(read(file));
}

function listFiles(root, segment, suffix) {
  // Fixed-depth walk (`<root>/<crate|package>/<segment>/<file>`): a recursive
  // readdir would descend into node_modules and cargo target trees.
  const files = [];
  let entries = [];
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch {
    // The sibling checkout (or package tree) is absent from this checkout: the
    // cross-repo half of the contract cannot run and must not fail the gate.
    return files;
  }
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const candidate = path.join(root, entry.name, segment, suffix);
    try {
      readFileSync(candidate);
      files.push(candidate);
    } catch {
      // No such file in this crate/package — fine.
    }
  }
  return files;
}

/** Every route-manifest source the sibling publishes, across all its crates. */
function sandboxRouteManifestSources() {
  return listFiles(SANDBOX_CRATES, 'src', 'http_route_manifest.rs');
}

/** `permission: "web.sandbox.read"` declarations in this repo's console modules. */
function consoleMenuSandboxCodes() {
  const packages = path.join(REPO_ROOT, 'apps/sdkwork-webserver-pc/packages');
  const codes = [];
  for (const file of listFiles(packages, 'src', 'module.ts')) {
    for (const match of read(file).matchAll(/permission:\s*"([^"]+)"/gu)) {
      if (match[1].startsWith(SANDBOX_MENU_CODE_PREFIX)) {
        codes.push(match[1]);
      }
    }
  }
  return codes;
}

const manifest = readJson(path.join(REPO_ROOT, 'specs/iam.module.manifest.json'));
const appConfig = readJson(path.join(REPO_ROOT, 'sdkwork.app.config.json'));
const catalog = new Map(manifest.permissions.catalog.map((entry) => [entry.code, entry]));

test('the sibling sandbox route planes demand no web-domain permission code', () => {
  const manifests = sandboxRouteManifestSources();
  // The control for this whole test: without the sibling checkout there is
  // nothing to assert. The skip is deliberate and visible here rather than
  // silent inside a loop that never runs.
  if (manifests.length === 0) {
    test.skip('sdkwork-sandbox checkout is absent from this workspace', () => {});
    return;
  }
  for (const file of manifests) {
    const source = read(file);
    assert.ok(
      !/with_required_permission\s*\(/u.test(source),
      `${path.relative(WORKSPACE_ROOT, file)} declares a per-route permission; the sandbox ` +
        'surface is machine-gated, so a web.* enforcement point cannot return without ' +
        're-materializing this module catalog first',
    );
    assert.ok(
      !/\bweb\.[a-z]+\.[a-z_]+\b/u.test(source),
      `${path.relative(WORKSPACE_ROOT, file)} names a web.* permission code literally`,
    );
  }
});

test('every sandbox menu code the console declares is an active, grantable catalog entry', () => {
  const menuCodes = consoleMenuSandboxCodes();
  assert.ok(
    menuCodes.includes('web.sandbox.read'),
    'the VM Instances menu must keep its visibility code; dropping it hides the page',
  );

  for (const code of menuCodes) {
    const entry = catalog.get(code);
    assert.ok(
      entry,
      `menu permission ${code} is demanded by the console but not in the web catalog, ` +
        'so no role can ever be granted it',
    );
    assert.equal(entry.status, 'active', `${entry.code} must be an active permission`);
    assert.ok(entry.name.length > 0, `${entry.code} must carry a display name`);
    assert.equal(entry.replacementCode, null, `${entry.code} is active, so it replaces nothing`);
  }
});

test('a plain console user is granted the sandbox menu codes, not told to ask', () => {
  const appUser = manifest.roles.roleGrantExtensions.find((entry) => entry.roleCode === 'app_user');
  assert.ok(appUser, 'app_user must carry a role grant extension');

  // The product statement is that every account may provision its own sandbox, and
  // ownership is decided server-side from the verified principal — so the console
  // role is the one that needs the grant. Without it the menu is invisible (or the
  // page reachable but refused), which is indistinguishable from an outage.
  const grants = (code) =>
    appUser.patterns.some(
      (pattern) =>
        pattern === code ||
        pattern === `${manifest.domain}.*` ||
        (pattern.endsWith('.*') && code.startsWith(pattern.slice(0, -1))),
    );

  for (const code of consoleMenuSandboxCodes()) {
    assert.ok(grants(code), `${code} must be granted to app_user`);
  }
});

test('the catalog a sandbox menu code resolves against is the web domain catalog', () => {
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
});

test('the bootstrap access token scope is not used as the sandbox RBAC surface', () => {
  const scope = appConfig.backend?.accessTokenPermissionScope ?? [];

  // `IAM_SPEC` section 543 and `IAM_APPLICATION_BOOTSTRAP_SPEC` section 167: the
  // bootstrap scope gates credential entry and pre-login transport, must stay
  // minimal, and must not duplicate module catalogs. Registering the sandbox codes
  // there instead of in the catalog looks like it works in a hand-driven session
  // while leaving the permission unassignable through IAM — and it is the obvious
  // wrong turn for whoever next hits a 403 on this page.
  for (const code of consoleMenuSandboxCodes()) {
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
