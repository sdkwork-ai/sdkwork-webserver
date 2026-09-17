/**
 * Runtime bundle contract.
 *
 * Asserts the *shipped artifact* — `src/runtime/webserver-app.js` plus the frozen
 * `runtime-env.js` — composes and behaves. Everything below drives the same
 * bundle the device loads, with the in-memory host adapter the architecture
 * standard requires for exactly this purpose
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8/§12).
 */
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const RUNTIME_DIR = path.join(APP_ROOT, "src", "runtime");
const require = createRequire(import.meta.url);
const runtime = require(path.join(RUNTIME_DIR, "webserver-app.js"));
const runtimeEnv = require(path.join(RUNTIME_DIR, "runtime-env.js"));
const buildManifest = JSON.parse(fs.readFileSync(path.join(RUNTIME_DIR, "build-manifest.json"), "utf8"));

const DEVELOPMENT_PROFILE = JSON.parse(
  fs.readFileSync(
    path.join(APP_ROOT, "config/mini-program/runtime-env.standalone.development.json"),
    "utf8",
  ),
);

function flush() {
  return new Promise((resolve) => {
    setTimeout(resolve, 0);
  });
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolveFn, rejectFn) => {
    resolve = resolveFn;
    reject = rejectFn;
  });
  return { promise, resolve, reject };
}

function bootstrap(overrides = {}) {
  return runtime.bootstrapWebserverMiniProgram({
    runtimeConfig: DEVELOPMENT_PROFILE,
    host: runtime.createMemoryWebserverMpHostAdapter({ language: "zh_CN" }),
    authenticated: true,
    hasPermission: () => true,
    ...overrides,
  });
}

function record(patch) {
  return {
    id: "app-1",
    name: "Console",
    slug: "console",
    appKind: "SPA_WEB",
    appStatus: "ACTIVE",
    description: "demo",
    defaultEnvironment: "development",
    latestReleaseTag: "0.1.0",
    platformTargetCount: 2,
    createdAt: "2026-09-17T00:00:00Z",
    updatedAt: "2026-09-17T00:00:00Z",
    version: "1",
    ...patch,
  };
}

function pageInfo(patch) {
  return { mode: "offset", page: 1, pageSize: 20, totalItems: 1, totalPages: 1, hasMore: false, ...patch };
}

test("the emitted runtime artifacts describe the selected profile", () => {
  assert.equal(buildManifest.runtimeTarget, "mini-program");
  assert.equal(buildManifest.platform, "MP_WEIXIN");
  assert.equal(buildManifest.profileId, `${buildManifest.deploymentProfile}.${buildManifest.environment}`);
  assert.deepEqual(JSON.parse(fs.readFileSync(path.join(RUNTIME_DIR, "route-projection.json"), "utf8")).pages, buildManifest.pages);
  assert.match(fs.readFileSync(path.join(RUNTIME_DIR, "runtime-env.js"), "utf8"), /SDKWORK_PROFILE_ID/u);
  assert.equal(runtimeEnv.SDKWORK_PROFILE_ID, buildManifest.profileId);
});

test("the bundle exposes the composition and page-model surface the pages require", () => {
  for (const marker of [
    "bootstrapWebserverMiniProgram",
    "createApplicationsListPageBinding",
    "createWebserverMpApplicationListPageModel",
    "createMemoryWebserverMpHostAdapter",
    "currentLocale",
    "stopPullDownRefresh",
  ]) {
    assert.equal(typeof runtime[marker], "function", `runtime bundle must export ${marker}`);
  }
});

test("bootstrap negotiates the locale from the host and resolves copy", () => {
  const composed = bootstrap();
  assert.equal(composed.locale, "zh-CN");
  assert.equal(composed.resolveMessage("applications.list.empty"), "当前租户还没有应用。");
  assert.equal(composed.resolveMessage("chrome.brand"), "SDKWork Web Server");
  // An unknown key degrades to the key itself; a blank string would render an
  // invisible screen instead of a diagnosable one.
  assert.equal(composed.resolveMessage("applications.list.missing"), "applications.list.missing");
});

test("bootstrap falls back to the declared default when the device locale is unshipped", () => {
  const composed = bootstrap({
    host: runtime.createMemoryWebserverMpHostAdapter({ language: "fr_FR" }),
  });
  assert.equal(composed.locale, "zh-CN");
  assert.equal(composed.resolveMessage("applications.list.title"), "应用");
});

test("navigation is filtered by the runtime permission predicate", () => {
  const granted = bootstrap();
  assert.equal(granted.navigation.length, 1);
  assert.equal(granted.resolveMessage(granted.navigation[0].labelKey), "应用");
  assert.equal(granted.navigation[0].path, "/pages/applications/index");

  const denied = bootstrap({ hasPermission: () => false });
  assert.equal(denied.declaredNavigation.length, 1, "the console still declares the entry");
  assert.deepEqual(denied.navigation, [], "an ungranted entry must not render");
});

test("the auth gate blocks an unauthenticated session before any load", async () => {
  const composed = bootstrap({ authenticated: false });
  const binding = runtime.createApplicationsListPageBinding({
    onDataChange: () => {},
  });
  assert.equal(binding.enterable, false, "the binding must inherit the launched session");
  assert.equal(binding.blockedReason, "unauthenticated");
  assert.ok(composed.declaredNavigation.length > 0, "the console still declares the entry");

  // A blocked page must not reach the transport: an attempted request would
  // surface as an error once settled, and nothing may be appended.
  binding.load();
  binding.loadMore();
  await flush();
  const data = binding.getData();
  assert.equal(data.items.length, 0);
  assert.equal(data.errorMessage, "", "a blocked binding must issue no request");
  assert.equal(data.errorDetail, "");
});

test("route access decisions use the shell vocabulary", () => {
  const route = { auth: "required", permissionHint: "deploy.apps.read" };
  assert.deepEqual(
    runtime.resolveWebserverMpRouteAccess(route, { authenticated: false, hasPermission: () => true }),
    { allowed: false, reason: "unauthenticated" },
  );
  assert.deepEqual(
    runtime.resolveWebserverMpRouteAccess(route, { authenticated: true, hasPermission: () => false }),
    { allowed: false, reason: "forbidden" },
  );
  assert.deepEqual(
    runtime.resolveWebserverMpRouteAccess(route, { authenticated: true, hasPermission: () => true }),
    { allowed: true },
  );
});

test("the page model loads one page, appends the next, and stops at the last", async () => {
  bootstrap();
  const calls = [];
  const service = runtime.createWebserverMpApplicationsService({
    app: {
      list: async (params) => {
        calls.push(params);
        if (params.page === 1) {
          return {
            items: [record({ id: "app-1", name: "Alpha" }), record({ id: "app-2", name: "Beta" })],
            pageInfo: pageInfo({ page: 1, totalItems: 3, totalPages: 2, hasMore: true }),
          };
        }
        return {
          items: [record({ id: "app-3", name: "Gamma" })],
          pageInfo: pageInfo({ page: 2, totalItems: 3, totalPages: 2, hasMore: false }),
        };
      },
    },
  });
  const states = [];
  const model = runtime.createWebserverMpApplicationListPageModel({
    service,
    resolveMessage: runtime.getWebserverMiniProgramRuntime().resolveMessage,
    onDataChange: (data) => states.push(data),
  });

  model.load();
  await flush();
  assert.deepEqual(calls, [{ page: 1, pageSize: 20 }], "PAGINATION_SPEC forbids a full-set load");
  assert.deepEqual(model.getData().items.map((item) => item.id), ["app-1", "app-2"]);
  assert.equal(model.getData().hasMore, true);
  assert.equal(model.getData().loading, false);
  assert.equal(model.getData().items[0].kindLabel, "单页应用");
  assert.equal(model.getData().items[0].statusLabel, "运行中");

  model.loadMore();
  await flush();
  assert.deepEqual(calls, [{ page: 1, pageSize: 20 }, { page: 2, pageSize: 20 }]);
  assert.deepEqual(model.getData().items.map((item) => item.id), ["app-1", "app-2", "app-3"]);
  assert.equal(model.getData().hasMore, false);

  model.loadMore();
  await flush();
  assert.equal(calls.length, 2, "a drained list must not keep requesting pages");
  assert.ok(states.length >= 3);
});

test("a failed load reports localized copy plus technical detail", async () => {
  bootstrap();
  const service = {
    loadPage: async () => {
      throw new Error("HTTP 401");
    },
  };
  const model = runtime.createWebserverMpApplicationListPageModel({
    service,
    resolveMessage: runtime.getWebserverMiniProgramRuntime().resolveMessage,
    onDataChange: () => {},
  });
  model.load();
  await flush();
  assert.equal(model.getData().errorMessage, "应用列表加载失败。");
  assert.equal(model.getData().errorDetail, "HTTP 401");
  assert.equal(model.getData().loading, false);
  assert.equal(model.status(), "error");
});

test("a stale response cannot overwrite a newer one", async () => {
  bootstrap();
  const first = deferred();
  const second = deferred();
  let call = 0;
  const service = {
    loadPage: async () => {
      call += 1;
      return call === 1 ? first.promise : second.promise;
    },
  };
  const model = runtime.createWebserverMpApplicationListPageModel({
    service,
    resolveMessage: runtime.getWebserverMiniProgramRuntime().resolveMessage,
    onDataChange: () => {},
  });

  model.load();
  model.load();
  second.resolve({ items: [], page: pageInfo({ totalItems: 1, totalPages: 1 }) });
  await flush();
  first.resolve({ items: [record({ id: "stale" })], page: pageInfo({ totalItems: 9, totalPages: 9 }) });
  await flush();

  assert.deepEqual(model.getData().items, [], "the superseded response must be dropped");
  assert.equal(model.getData().totalItems, 1);
});

test("the token manager persists through the injected storage and clearing drops it", () => {
  const storage = runtime.createWebserverMpMemoryStorage();
  const manager = runtime.createWebserverMpTokenManager(storage);
  manager.setTokens({ accessToken: "access-1", authToken: "auth-1" });
  assert.equal(manager.getAccessToken(), "access-1");
  assert.match(storage.get("sdkwork-webserver-mp:session:v1") ?? "", /access-1/u);

  manager.clearTokens();
  assert.equal(manager.getAccessToken(), undefined);
  assert.equal(storage.get("sdkwork-webserver-mp:session:v1"), null);
});

test("logout clears every registered capability cache", () => {
  const storage = runtime.createWebserverMpMemoryStorage();
  const manager = runtime.createWebserverMpTokenManager(storage);
  let cleared = 0;
  const unregister = runtime.registerWebserverMpSensitiveStateClearer(() => {
    cleared += 1;
  });
  try {
    manager.setTokens({ accessToken: "access-1", authToken: "auth-1" });
    manager.clearTokens();
    assert.equal(cleared, 1, "clearing the session must drop tenant-scoped caches too");
    unregister();
    manager.setTokens({ accessToken: "access-2", authToken: "auth-2" });
    manager.clearTokens();
    assert.equal(cleared, 1, "an unregistered cache must stop being cleared");
  } finally {
    unregister();
  }
});

test("the runtime rejects a profile whose identity does not parse", () => {
  assert.throws(
    () => runtime.bootstrapWebserverMiniProgram({
      runtimeConfig: { ...DEVELOPMENT_PROFILE, SDKWORK_PROFILE_ID: "standalone.production" },
    }),
    /SDKWORK_PROFILE_ID must equal standalone\.development/u,
  );
  assert.throws(
    () => runtime.bootstrapWebserverMiniProgram({
      runtimeConfig: { ...DEVELOPMENT_PROFILE, SDKWORK_RUNTIME_TARGET: "browser" },
    }),
    /SDKWORK_RUNTIME_TARGET must equal mini-program/u,
  );
});
