import type { AuthTokenManager } from "@sdkwork/sdk-common";
import type { ApplicationsListReader } from "@sdkwork/webserver-h5-applications";
import {
  createWebserverH5DeployAppSdkClient,
  createWebserverH5DriveAppSdkClient,
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
 *
 * The `deploy_app` entity has a single owner (`sdkwork-deployments`), so the
 * applications screen reads it through the deployments App SDK rather than
 * through a second, webserver-owned application surface
 * (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
 */
export interface WebserverH5SdkClients {
  readonly deploy: WebserverH5DeployAppClient;
  readonly drive: WebserverH5DriveAppClient;
  /**
   * The single TokenManager every composed client shares. Bootstrap seeds it with
   * the credential-entry bootstrap credential before any screen can dispatch;
   * handing it back out keeps the seeding step from constructing a second,
   * disconnected manager (`IAM_CREDENTIAL_ENTRY_SPEC.md` section 5).
   */
  readonly tokenManager: AuthTokenManager;
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
    deploy: createWebserverH5DeployAppSdkClient({
      baseUrl: config.deployAppApiBaseUrl,
      tokenManager,
    }),
    drive: createWebserverH5DriveAppSdkClient({
      baseUrl: config.driveAppApiBaseUrl,
      tokenManager,
    }),
    tokenManager,
  };
}
