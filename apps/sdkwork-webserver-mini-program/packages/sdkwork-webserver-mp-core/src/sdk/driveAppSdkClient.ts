import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient,
  type SdkworkDriveAppClient as GeneratedDriveAppClient,
} from "@sdkwork/drive-app-sdk";

import { getWebserverMpGlobalTokenManager } from "../session/session";

/**
 * Drive app SDK construction. The console composes the drive-owned admin storage
 * plane instead of forking it (the same ownership rule the PC console follows
 * through `@sdkwork/webserver-pc-admin-storage`), so the drive client is built
 * here and injected downward like every other generated client.
 */
export type WebserverMpDriveAppClient = GeneratedDriveAppClient;

export interface CreateWebserverMpDriveAppSdkClientOptions {
  baseUrl: string;
  tokenManager?: AuthTokenManager;
  timeoutMs?: number;
}

export function createWebserverMpDriveAppSdkClient(
  options: CreateWebserverMpDriveAppSdkClientOptions,
): WebserverMpDriveAppClient {
  return createClient({
    baseUrl: options.baseUrl,
    authMode: "dual-token",
    platform: "mini-program",
    tokenManager: options.tokenManager ?? getWebserverMpGlobalTokenManager(),
    ...(options.timeoutMs ? { timeout: options.timeoutMs } : {}),
  });
}
