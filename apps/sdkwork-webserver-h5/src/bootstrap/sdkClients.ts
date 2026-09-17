import type { AuthTokenManager } from "@sdkwork/sdk-common";
import type { ApplicationsListReader } from "@sdkwork/webserver-h5-applications";
import {
  createWebserverH5AppSdkClient,
  createWebserverH5DeployAppSdkClient,
  createWebserverH5DriveAppSdkClient,
  type WebserverH5AppSdkClient,
  type WebserverH5DeployAppClient,
  type WebserverH5DriveAppClient,
  type WebserverH5RuntimeConfig,
} from "@sdkwork/webserver-h5-core/sdk";
import { createWebserverH5TokenManager } from "@sdkwork/webserver-h5-core/session";

/**
 * Every generated client this root composes, built once at bootstrap.
 *
 * `APP_SDK_INTEGRATION_SPEC.md` §2 keeps construction here: the clients are
 * handed down to feature packages as props, so no feature module ever imports a
 * generated transport or calls `createClient` itself.
 */
export interface WebserverH5SdkClients {
  readonly app: WebserverH5AppSdkClient;
  readonly deploy: WebserverH5DeployAppClient;
  readonly drive: WebserverH5DriveAppClient;
}

/**
 * The slice of the composed clients the root injects into screens. Naming it
 * separately states exactly what the mounted screens depend on, and it is
 * structurally satisfied by `WebserverH5SdkClients`, so bootstrap passes the
 * whole set through without an adapter.
 */
export interface WebserverH5ScreenClients {
  readonly deploy: ApplicationsListReader;
}

export function createWebserverH5SdkClients(
  config: WebserverH5RuntimeConfig,
  tokenManager: AuthTokenManager = createWebserverH5TokenManager(),
): WebserverH5SdkClients {
  return {
    app: createWebserverH5AppSdkClient({
      baseUrl: config.appApiBaseUrl,
      tokenManager,
    }),
    deploy: createWebserverH5DeployAppSdkClient({
      baseUrl: config.deployAppApiBaseUrl,
      tokenManager,
    }),
    drive: createWebserverH5DriveAppSdkClient({
      baseUrl: config.driveAppApiBaseUrl,
      tokenManager,
    }),
  };
}
