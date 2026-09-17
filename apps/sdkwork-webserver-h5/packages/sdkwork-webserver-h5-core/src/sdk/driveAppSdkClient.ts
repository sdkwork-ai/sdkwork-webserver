import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient,
  type SdkworkDriveAppClient as GeneratedDriveAppClient,
} from "@sdkwork/drive-app-sdk";

import { getWebserverH5GlobalTokenManager } from "../session/session";

/**
 * Drive app SDK construction. The H5 console composes the drive-owned admin
 * storage plane instead of forking it (same ownership rule the PC console
 * follows through `@sdkwork/webserver-pc-admin-storage`), so the drive client
 * is built here and injected downward like every other generated client.
 */
export type WebserverH5DriveAppClient = GeneratedDriveAppClient;

export interface CreateWebserverH5DriveAppSdkClientOptions {
  baseUrl: string;
  tokenManager?: AuthTokenManager;
  timeoutMs?: number;
}

export function createWebserverH5DriveAppSdkClient(
  options: CreateWebserverH5DriveAppSdkClientOptions,
): WebserverH5DriveAppClient {
  return createClient({
    baseUrl: options.baseUrl,
    authMode: "dual-token",
    platform: "h5",
    tokenManager: options.tokenManager ?? getWebserverH5GlobalTokenManager(),
    ...(options.timeoutMs ? { timeout: options.timeoutMs } : {}),
  });
}
