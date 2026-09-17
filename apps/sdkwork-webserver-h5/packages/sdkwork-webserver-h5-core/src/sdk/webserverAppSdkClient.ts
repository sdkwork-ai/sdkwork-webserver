import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient,
  type SdkworkAppClient as GeneratedWebserverAppClient,
  type SdkworkAppConfig,
} from "@sdkwork/webserver-app-sdk";

import { getWebserverH5GlobalTokenManager } from "../session/session";

/**
 * The single construction point for the webserver app SDK client in this root.
 * `APP_SDK_INTEGRATION_SPEC.md` §2 requires generated clients to be created in
 * core and injected into feature packages — feature code never calls
 * `createClient` itself and never builds a raw transport.
 */
export type WebserverH5AppSdkClient = GeneratedWebserverAppClient;

export interface CreateWebserverH5AppSdkClientOptions {
  /** Absolute API origin resolved from `runtime-env.json`. */
  baseUrl: string;
  /** Defaults to the process-wide H5 token manager. */
  tokenManager?: AuthTokenManager;
  /** Request timeout override; deployment profiles pick the default. */
  timeoutMs?: number;
}

/** The webserver app API is the same-origin `/` root in standalone mode. */
export function createWebserverH5AppSdkClient(
  options: CreateWebserverH5AppSdkClientOptions,
): WebserverH5AppSdkClient {
  const config: SdkworkAppConfig = {
    baseUrl: options.baseUrl,
    authMode: "dual-token",
    platform: "h5",
    tokenManager: options.tokenManager ?? getWebserverH5GlobalTokenManager(),
    ...(options.timeoutMs ? { timeout: options.timeoutMs } : {}),
  };
  return createClient(config);
}

export type { SdkworkAppConfig };

/**
 * Re-exported so feature packages can name the response types they render
 * without deep-importing generated SDK transport modules.
 */
export type {
  ApplicationResponse,
  CreateApplicationRequest,
  PageInfo,
  UpdateApplicationRequest,
} from "@sdkwork/webserver-app-sdk";
