import {
  createWebserverMpApplicationsService,
  createWebserverMpApplicationListPageModel,
  type WebserverMpApplicationListPageData,
  type WebserverMpApplicationListPageModel,
} from "@sdkwork/webserver-mp-applications";

import {
  bootstrapWebserverMiniProgram,
  getWebserverMiniProgramRuntime,
  hasWebserverMiniProgramRuntime,
  resolveRouteAccess,
  resetWebserverMiniProgramRuntime,
  type BootstrapWebserverMiniProgramOptions,
} from "./runtime";

/**
 * Runtime bundle surface.
 *
 * This module is the esbuild entry: `scripts/build-runtime.mjs` bundles it into
 * `src/runtime/webserver-app.js`, and the native pages `require` that bundle. It
 * is therefore the *only* bridge between the TypeScript packages and the `.js`
 * mini program pages — pages get composition and page models, never a package
 * internals import and never an SDK client they could call directly.
 */
export interface CreateApplicationsListPageBindingOptions {
  /** Receives the complete next data object; the page forwards it to `setData`. */
  readonly onDataChange: (data: WebserverMpApplicationListPageData) => void;
  /** Called when a load completes, so a pull-to-refresh gesture can be released. */
  readonly onSettled?: () => void;
  readonly authenticated?: boolean;
  readonly hasPermission?: (permission: string) => boolean;
}

export interface ApplicationsListPageBinding {
  readonly enterable: boolean;
  readonly blockedReason: "unauthenticated" | "forbidden" | "allowed";
  load(): void;
  loadMore(): void;
  getData(): WebserverMpApplicationListPageData;
  message(key: string): string;
  stopPullDownRefresh(): void;
}

export function createApplicationsListPageBinding(
  options: CreateApplicationsListPageBindingOptions,
): ApplicationsListPageBinding {
  const runtime = getWebserverMiniProgramRuntime();
  const access = resolveRouteAccess("app.webserver.applications.list", {
    authenticated: options.authenticated ?? runtime.authContext.authenticated,
    hasPermission: options.hasPermission ?? runtime.authContext.hasPermission,
  });
  const service = createWebserverMpApplicationsService(runtime.screenClients.deploy);
  const model: WebserverMpApplicationListPageModel = createWebserverMpApplicationListPageModel({
    service,
    resolveMessage: runtime.resolveMessage,
    onDataChange: options.onDataChange,
    ...(options.onSettled ? { onSettled: options.onSettled } : {}),
  });
  /**
   * A blocked page must not issue an authenticated request, so the guard sits in
   * front of the model rather than only in front of the first render.
   */
  const guard = (action: () => void): void => {
    if (access.allowed) {
      action();
    }
  };

  return {
    enterable: access.allowed,
    blockedReason: access.allowed ? "allowed" : access.reason,
    load: () => guard(() => model.load()),
    loadMore: () => guard(() => model.loadMore()),
    getData: () => model.getData(),
    message: (key: string) => runtime.resolveMessage(key),
    stopPullDownRefresh: () => runtime.host.stopPullDownRefresh(),
  };
}

/**
 * Composition exports the mini program `App()` lifecycle calls. Kept as thin
 * named functions so `src/app.js` stays declarative.
 */
export { bootstrapWebserverMiniProgram, getWebserverMiniProgramRuntime, hasWebserverMiniProgramRuntime };

export function stopPullDownRefresh(): void {
  getWebserverMiniProgramRuntime().host.stopPullDownRefresh();
}

export function showToast(message: string): void {
  getWebserverMiniProgramRuntime().host.showToast(message);
}

/** Locale negotiated at launch, so the page can report it to the platform. */
export function currentLocale(): string {
  return getWebserverMiniProgramRuntime().locale;
}

export function navigationLabels(): string[] {
  const runtime = getWebserverMiniProgramRuntime();
  return runtime.navigation.map((entry) => runtime.resolveMessage(entry.labelKey));
}

export function routePagePaths(): string[] {
  return getWebserverMiniProgramRuntime()
    .routeContributions
    .filter((route) => route.miniProgram.rootPackage === true)
    .map((route) => route.miniProgram.pagePath);
}

/**
 * Logout/account-switch teardown. The root calls this instead of clearing storage
 * itself, so the platform record, the token manager, and every registered
 * capability cache drop together (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7).
 */
export function clearSensitiveState(): void {
  getWebserverMiniProgramRuntime().tokenManager.clearTokens?.();
  resetWebserverMiniProgramRuntime();
}

export type BootstrapOptions = BootstrapWebserverMiniProgramOptions;

/**
 * The bundle's public surface.
 *
 * A mini program page can only `require` the bundle — it cannot import a package
 * — so anything a page legitimately needs is re-exported here rather than the page
 * reaching into package internals. The contract tests consume the same surface,
 * which is what makes them exercise the shipped artifact instead of a parallel
 * copy of it.
 */
export {
  createWebserverMpMessageCatalog,
  normalizeWebserverMpLocale,
  resolveWebserverMpScreenStatus,
  webserverMpTokens,
} from "@sdkwork/webserver-mp-commons";
export {
  createWebserverMpNavigation,
  filterWebserverMpNavigation,
  resolveWebserverMpRouteAccess,
  validateWebserverMpRouteContributions,
} from "@sdkwork/webserver-mp-shell";
export {
  createWebserverMpApplicationListPageModel,
  createWebserverMpApplicationsService,
  clearWebserverMpApplicationListState,
  mapWebserverMpApplicationItem,
  registerWebserverMpApplicationListStateClearing,
  toWebserverMpApplicationRow,
} from "@sdkwork/webserver-mp-applications";
export { createMemoryWebserverMpHostAdapter } from "@sdkwork/webserver-mp-host/testing";
export {
  createWebserverMpMemoryStorage,
  createWebserverMpTokenManager,
  registerWebserverMpSensitiveStateClearer,
} from "@sdkwork/webserver-mp-core/session";
export { parseWebserverMpRuntimeConfig } from "@sdkwork/webserver-mp-core/sdk";

