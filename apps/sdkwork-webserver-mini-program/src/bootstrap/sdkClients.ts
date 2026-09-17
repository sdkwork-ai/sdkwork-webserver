import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverMpAppSdkClient,
  createWebserverMpDeployAppSdkClient,
  createWebserverMpDriveAppSdkClient,
  type WebserverMpAppSdkClient,
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
 */
export interface WebserverMiniProgramSdkClients {
  readonly app: WebserverMpAppSdkClient;
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
    app: createWebserverMpAppSdkClient({
      baseUrl: config.appApiBaseUrl,
      tokenManager,
    }),
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
