import { createSdkworkIamRuntimeAuthController, type SdkworkIamRuntimeAuthRuntimeLike } from "@sdkwork/auth-pc-react";
import {
  createSdkworkAppbasePcAuthRuntime,
  createSdkworkSessionAuthUnauthorizedIntegration,
} from "@sdkwork/auth-runtime-pc-react";
import { initializeCredentialEntryTokenManager } from "@sdkwork/iam-credential-entry";
import { createClient as createIamAppClient } from "@sdkwork/iam-app-sdk";
import { createPersistentIamTokenStore } from "@sdkwork/iam-runtime";
import { createTokenManager } from "@sdkwork/sdk-common";
import type { WebserverConsoleSdkClients } from "@sdkwork/webserver-pc-console-core";
import { loadWebserverPcRuntimeConfig, resolveWebserverLocale } from "@sdkwork/webserver-pc-core";
import { createWebserverAuthRuntimeConfigLoader } from "../auth/authRuntimeConfig.ts";

const WEBSERVER_PC_APP_ID = "sdkwork-webserver-pc";

export async function bootstrapWebserverPcRuntime() {
  const config = await loadWebserverPcRuntimeConfig();
  const locale = resolveWebserverLocale(config, navigator.languages);
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
    localeProvider: () => locale,
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
            driveAppApiBaseUrl: config.driveAppApiBaseUrl,
            webAppApiBaseUrl: config.appApiBaseUrl,
          }, tokenManager);
          attachSdkClientBoundaries([clients.web, clients.drive]);
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
  // hydrateTokenManager may restore a persisted session that has Auth-Token but
  // no Access-Token; credential-entry metadata routes require the bootstrap
  // Access-Token injected into index.html by the standalone gateway.
  initializeCredentialEntryTokenManager(tokenManager);
  const getAuthRuntime = () => auth.getRuntime() as unknown as SdkworkIamRuntimeAuthRuntimeLike;
  const authController = createSdkworkIamRuntimeAuthController({ getRuntime: getAuthRuntime });
  const loadAuthRuntimeConfig = createWebserverAuthRuntimeConfigLoader(auth.appbaseApp);
  return {
    attachSdkClientBoundaries,
    auth,
    authController,
    config,
    loadAuthRuntimeConfig,
    loadConsoleClients,
    locale,
    tokenManager,
  } as const;
}

export type BootstrappedWebserverPcRuntime = Awaited<ReturnType<typeof bootstrapWebserverPcRuntime>>;
