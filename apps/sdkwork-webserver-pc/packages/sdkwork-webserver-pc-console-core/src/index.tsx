import {
  createClient as createDeployAppClient,
  type SdkworkDeployAppClient,
} from "@sdkwork/deployments-app-sdk";
import { createDriveAppClient, type SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createContext, useContext, type ReactNode } from "react";

/**
 * Console SDK wiring. This package owns exactly one thing: turning the host's
 * base URLs plus token manager into the two generated clients the console
 * surfaces consume, and publishing them through a typed host port.
 *
 * Both clients are dependency-owned. The application lifecycle used to live here
 * as a resource registry over `webserver_application` / `webserver_source_version` /
 * `webserver_deployment`; that entity is owned by sdkwork-deployments (`deploy_app`)
 * and every Applications page is now the canonical deployments page bridged by
 * `@sdkwork/webserver-pc-console-delivery`. The client this host constructs is
 * the deployments generated client as well, so no console surface reaches the
 * legacy webserver app-api face any more.
 */

export type { SdkworkDeployAppClient, SdkworkDriveAppClient };
export { createDeployAppClient, createDriveAppClient };
export {
  WEBSERVER_PC_PLUGIN_PACKAGE_UPLOAD,
  WEBSERVER_PC_UPLOAD_DECLARATIONS,
} from "./sdk/uploadDeclaration";
export type {
  WebserverPcUploadDeclarationEntry,
} from "./sdk/uploadDeclaration";

export type WebserverConsoleSdkClient = SdkworkDeployAppClient;

export interface WebserverConsoleSdkClients {
  deploy: SdkworkDeployAppClient;
  drive: SdkworkDriveAppClient;
}

const Context = createContext<WebserverConsoleSdkClients | null>(null);

export function createWebserverConsoleSdkClient(baseUrl: string, tokenManager: AuthTokenManager): WebserverConsoleSdkClient { return createDeployAppClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager }); }
export function createWebserverConsoleSdkClients(baseUrls: { deployAppApiBaseUrl: string; driveAppApiBaseUrl: string }, tokenManager: AuthTokenManager): WebserverConsoleSdkClients { return { deploy: createWebserverConsoleSdkClient(baseUrls.deployAppApiBaseUrl, tokenManager), drive: createDriveAppClient({ baseUrl: baseUrls.driveAppApiBaseUrl, authMode: "dual-token", platform: "pc", tokenManager }) }; }
export function WebserverConsoleSdkProvider({ children, clients }: { children: ReactNode; clients: WebserverConsoleSdkClients }) { return <Context.Provider value={clients}>{children}</Context.Provider>; }
export function useWebserverConsoleSdk(): WebserverConsoleSdkClients { const clients = useContext(Context); if (!clients) throw new Error("WebserverConsoleSdkProvider is required"); return clients; }
