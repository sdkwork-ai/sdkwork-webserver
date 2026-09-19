import {
  createSdkworkIamRuntimeAuthController,
  type SdkworkIamRuntimeAuthRuntimeLike,
} from "@sdkwork/auth-pc-react";
import {
  createSdkworkAppbasePcAuthRuntime,
  createSdkworkSessionAuthUnauthorizedIntegration,
} from "@sdkwork/auth-runtime-pc-react";
import { readBootstrapAccessTokenFromProcessEnv } from "@sdkwork/iam-credential-entry";
import { createClient as createIamAppClient } from "@sdkwork/iam-app-sdk";
import { createPersistentIamTokenStore, resetTokenManagerToBootstrapAccessToken } from "@sdkwork/iam-runtime";
import { createTokenManager } from "@sdkwork/sdk-common";
import type { WebserverConsoleSdkClients } from "@sdkwork/webserver-pc-console-core";
import { loadWebserverPcRuntimeConfig, type WebserverLocale } from "@sdkwork/webserver-pc-core";
import { createWebserverAuthRuntimeConfigLoader } from "../auth/authRuntimeConfig.ts";
import { dedupeAuthControllerBootstrap } from "./authBootstrapDedupe.ts";
import { resolveBrowserInitialLocale } from "./locale.ts";

const WEBSERVER_PC_APP_ID = "sdkwork-webserver-pc";

export async function bootstrapWebserverPcRuntime() {
  const config = await loadWebserverPcRuntimeConfig();
  const locale = resolveBrowserInitialLocale(config);
  // SDK transports call the locale provider once per request, so the runtime
  // holds the active locale in a mutable slot: switching language in the shell
  // changes the Accept-Language of the next call instead of only the visible
  // copy (I18N_SPEC.md section 10).
  const localeState = { current: locale };
  const tokenManager = createTokenManager();
  // The shared IAM store owns authToken/accessToken persistence; app code never reads credentials.
  const tokenStore = createPersistentIamTokenStore({
    appId: WEBSERVER_PC_APP_ID,
    storage: window.localStorage,
  });
  const auth = createSdkworkAppbasePcAuthRuntime({
    app: { appId: WEBSERVER_PC_APP_ID, deploymentMode: config.deploymentProfile === "cloud" ? "saas" : "local", environment: config.environment === "development" ? "dev" : config.environment === "test" ? "test" : "prod", platform: "pc" },
    baseUrls: { appbaseAppApiBaseUrl: config.appbaseAppApiBaseUrl },
    createAppbaseAppClient: (clientConfig) => createIamAppClient({ ...clientConfig, timeout: config.environment === "production" || config.environment === "staging" ? 10_000 : 5_000 }),
    localeProvider: () => localeState.current,
    sessionAuth: true,
    tokenManager,
    tokenStore,
  });
  const sessionAuth = createSdkworkSessionAuthUnauthorizedIntegration({
    clearSession: () => { void auth.runtime.clearSession(); },
  });
  const attachSdkClientBoundaries = sessionAuth.attachSdkClientBoundaries;
  let consoleClientsPromise: Promise<WebserverConsoleSdkClients> | undefined;
  const loadConsoleClients = () => {
    if (!consoleClientsPromise) {
      consoleClientsPromise = import("@sdkwork/webserver-pc-console-core")
        .then(({ createWebserverConsoleSdkClients }) => {
          const clients = createWebserverConsoleSdkClients({
            deployAppApiBaseUrl: config.deployAppApiBaseUrl,
            driveAppApiBaseUrl: config.driveAppApiBaseUrl,
          }, tokenManager);
          attachSdkClientBoundaries([clients.deploy, clients.drive]);
          return clients;
        })
        .catch((cause: unknown) => {
          consoleClientsPromise = undefined;
          throw cause;
        });
    }
    return consoleClientsPromise;
  };
  await auth.runtime.hydrateTokenManager();
  if (!tokenManager.hasAuthToken()) {
    resetTokenManagerToBootstrapAccessToken(
      tokenManager,
      readBootstrapAccessTokenFromProcessEnv(),
    );
  }
  const getAuthRuntime = () => auth.getRuntime() as unknown as SdkworkIamRuntimeAuthRuntimeLike;
  const authController = dedupeAuthControllerBootstrap(
    createSdkworkIamRuntimeAuthController({ getRuntime: getAuthRuntime }),
  );
  const loadAuthRuntimeConfig = createWebserverAuthRuntimeConfigLoader(auth.appbaseApp, tokenManager);
  return {
    attachSdkClientBoundaries,
    auth,
    authController,
    config,
    loadAuthRuntimeConfig,
    loadConsoleClients,
    locale,
    setLocale(next: WebserverLocale) {
      localeState.current = next;
    },
    tokenManager,
  } as const;
}

export type BootstrappedWebserverPcRuntime = Awaited<ReturnType<typeof bootstrapWebserverPcRuntime>>;
