import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  filterWebserverMpNavigation,
  createWebserverMpNavigation,
  validateWebserverMpRouteContributions,
  resolveWebserverMpRouteAccess,
  type WebserverMpAuthContext,
  type WebserverMpNavigationEntry,
  type WebserverMpRouteContribution,
} from "@sdkwork/webserver-mp-shell";
import {
  WEBSERVER_MP_LOCALE_TAGS,
  type WebserverMpMessageCatalog,
} from "@sdkwork/webserver-mp-commons";
import {
  parseWebserverMpRuntimeConfig,
  createWebserverMpPlatformStorage,
  createWebserverMpTokenManager,
  type WebserverMpDeploymentProfile,
  type WebserverMpLocale,
  type WebserverMpRuntimeConfig,
} from "@sdkwork/webserver-mp-core";
import type {
  WebserverMpHostAdapter,
  WebserverMpHostStorage,
} from "@sdkwork/webserver-mp-host";

import { createWebserverMiniProgramHostAdapter } from "./hostAdapters";
import { resolveWebserverMiniProgramLocale } from "./locale";
import { createWebserverMiniProgramRoutes } from "./routes";
import {
  createWebserverMiniProgramSdkClients,
  toWebserverMiniProgramScreenClients,
  type WebserverMiniProgramScreenClients,
  type WebserverMiniProgramSdkClients,
} from "./sdkClients";
import {
  createWebserverMiniProgramMessageCatalog,
  createWebserverMiniProgramMessageResolver,
  type WebserverMiniProgramMessageResolver,
} from "../i18n/index";

/**
 * Root bootstrap.
 *
 * One pass, in this order: parse the frozen runtime profile, negotiate the locale
 * through the host adapter, build the message catalog, bind the host's storage
 * into the core token manager, then construct every generated SDK client. The
 * result is the single object the pages consume — nothing below re-reads
 * configuration or re-creates a client.
 *
 * Every dependency is injectable so the Node-side contract tests drive this exact
 * composition with an in-memory host instead of a device
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §8).
 */
export interface BootstrapWebserverMiniProgramOptions {
  /** `require("./runtime/runtime-env")` output, i.e. the selected profile. */
  readonly runtimeConfig: unknown;
  /** The profile the build selected; a mismatch is a build error, not a fallback. */
  readonly deploymentProfile?: WebserverMpDeploymentProfile;
  readonly host?: WebserverMpHostAdapter;
  readonly tokenManager?: AuthTokenManager;
  /** Permission predicate used by the shell auth gate. */
  readonly hasPermission?: (permission: string) => boolean;
  readonly authenticated?: boolean;
}

export interface WebserverMiniProgramRuntime {
  readonly runtimeConfig: WebserverMpRuntimeConfig;
  readonly host: WebserverMpHostAdapter;
  readonly locale: WebserverMpLocale;
  readonly messageCatalog: WebserverMpMessageCatalog;
  readonly resolveMessage: WebserverMiniProgramMessageResolver;
  readonly tokenManager: AuthTokenManager;
  readonly sdkClients: WebserverMiniProgramSdkClients;
  readonly screenClients: WebserverMiniProgramScreenClients;
  readonly routeContributions: readonly WebserverMpRouteContribution[];
  readonly navigation: readonly WebserverMpNavigationEntry[];
  /** Full navigation regardless of permission — what the console could show. */
  readonly declaredNavigation: readonly WebserverMpNavigationEntry[];
  /**
   * The session decision made once at launch. Pages read it from here instead of
   * being handed credentials of their own
   * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §7).
   */
  readonly authContext: WebserverMpAuthContext;
}

let activeRuntime: WebserverMiniProgramRuntime | null = null;

export function bootstrapWebserverMiniProgram(
  options: BootstrapWebserverMiniProgramOptions,
): WebserverMiniProgramRuntime {
  const runtimeConfig = parseWebserverMpRuntimeConfig(
    options.runtimeConfig,
    options.deploymentProfile,
  );
  const host = createWebserverMiniProgramHostAdapter(options.host);

  const routeContributions = createWebserverMiniProgramRoutes();
  const routeIssues = validateWebserverMpRouteContributions(routeContributions);
  if (routeIssues.length > 0) {
    throw new Error(`mini program route contributions are invalid: ${routeIssues.join("; ")}`);
  }

  const declaredNavigation = createWebserverMpNavigation(routeContributions);
  const authContext: WebserverMpAuthContext = {
    authenticated: options.authenticated ?? false,
    hasPermission: options.hasPermission ?? (() => false),
  };
  const navigation = filterWebserverMpNavigation(
    declaredNavigation,
    authContext.hasPermission,
  );

  const localeResolution = resolveWebserverMiniProgramLocale({
    host,
    supportedLocales: runtimeConfig.supportedLocales,
    defaultLocale: runtimeConfig.defaultLocale,
    fallbackLocale: runtimeConfig.fallbackLocale,
  });
  const messageCatalog = createWebserverMiniProgramMessageCatalog({
    supportedLocales: runtimeConfig.supportedLocales.length > 0
      ? runtimeConfig.supportedLocales
      : WEBSERVER_MP_LOCALE_TAGS,
    defaultLocale: runtimeConfig.defaultLocale,
    fallbackLocale: runtimeConfig.fallbackLocale,
  });

  const tokenManager = options.tokenManager ?? createWebserverMpTokenManager(
    toCoreStorage(host.createStorage()),
  );
  const sdkClients = createWebserverMiniProgramSdkClients(runtimeConfig, tokenManager);

  activeRuntime = {
    runtimeConfig,
    host,
    locale: localeResolution.locale,
    messageCatalog,
    resolveMessage: createWebserverMiniProgramMessageResolver(
      messageCatalog,
      localeResolution.locale,
    ),
    tokenManager,
    sdkClients,
    screenClients: toWebserverMiniProgramScreenClients(sdkClients),
    routeContributions,
    navigation,
    declaredNavigation,
    authContext,
  };
  return activeRuntime;
}

export function getWebserverMiniProgramRuntime(): WebserverMiniProgramRuntime {
  if (!activeRuntime) {
    throw new Error("bootstrapWebserverMiniProgram must run before the runtime is read");
  }
  return activeRuntime;
}

export function hasWebserverMiniProgramRuntime(): boolean {
  return activeRuntime !== null;
}

/**
 * Drop the composed runtime. Called on logout/account switch after the core has
 * cleared the session, so the next launch rebuilds clients against the new
 * identity instead of reusing a client bound to the previous tenant.
 */
export function resetWebserverMiniProgramRuntime(): void {
  activeRuntime = null;
}

export function resolveRouteAccess(
  routeId: string,
  context: WebserverMpAuthContext,
): ReturnType<typeof resolveWebserverMpRouteAccess> {
  const route = getWebserverMiniProgramRuntime().routeContributions.find(
    (entry) => entry.id === routeId,
  );
  if (!route) {
    return { allowed: false, reason: "forbidden" };
  }
  return resolveWebserverMpRouteAccess(route, context);
}

/**
 * The core's session module and the host adapter each declare the same three
 * storage methods. They are intentionally separate types — core must not depend
 * on the host package — so the bridge is an explicit, structurally-checked
 * adaptation rather than a shared import.
 */
function toCoreStorage(storage: WebserverMpHostStorage): {
  get(key: string): string | null;
  set(key: string, value: string): void;
  remove(key: string): void;
} {
  return {
    get: (key) => storage.get(key),
    set: (key, value) => storage.set(key, value),
    remove: (key) => storage.remove(key),
  };
}

/** Re-exported so pages and tests can build the platform storage explicitly. */
export { createWebserverMpPlatformStorage };
