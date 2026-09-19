import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverMpDeployAppSdkClient,
  createWebserverMpDriveAppSdkClient,
  type WebserverMpDeployAppClient,
  type WebserverMpDriveAppClient,
  type WebserverMpRuntimeConfig,
} from "@sdkwork/webserver-mp-core/sdk";
import type { ApplicationsListReader } from "@sdkwork/webserver-mp-applications";

/**
 * The generated clients the runtime hands to capability packages.
 *
 * `APP_SDK_INTEGRATION_SPEC.md` §2: every client is constructed in core and
 * injected downward. This module is the runtime's composition point — it decides
 * which clients exist and what a screen is allowed to see of them.
 *
 * The `deploy_app` entity has a single owner (`sdkwork-deployments`), so the
 * applications screen reads it through the deployments App SDK rather than
 * through a second, webserver-owned application surface
 * (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
 */
export interface WebserverMiniProgramSdkClients {
  readonly deploy: WebserverMpDeployAppClient;
  readonly drive: WebserverMpDriveAppClient;
}

/**
 * The production-boundary view of those clients. A capability package receives
 * exactly this, so a screen can only reach the operations it declared.
 */
export interface WebserverMiniProgramScreenClients {
  readonly deploy: ApplicationsListReader;
}

export function createWebserverMiniProgramSdkClients(
  config: WebserverMpRuntimeConfig,
  tokenManager: AuthTokenManager,
): WebserverMiniProgramSdkClients {
  return {
    deploy: createWebserverMpDeployAppSdkClient({
      baseUrl: config.deployAppApiBaseUrl,
      tokenManager,
    }),
    drive: createWebserverMpDriveAppSdkClient({
      baseUrl: config.driveAppApiBaseUrl,
      tokenManager,
    }),
  };
}

export function toWebserverMiniProgramScreenClients(
  clients: WebserverMiniProgramSdkClients,
): WebserverMiniProgramScreenClients {
  return { deploy: clients.deploy };
}
