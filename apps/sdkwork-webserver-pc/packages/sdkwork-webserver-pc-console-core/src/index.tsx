import {
  createClient as createDeployAppClient,
  type SdkworkDeployAppClient,
} from "@sdkwork/deployments-app-sdk";
import { createDriveAppClient, type SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import { createClient as createIamAppClient, type SdkworkAppClient } from "@sdkwork/iam-app-sdk";
import {
  createClient as createIamBackendClient,
  type SdkworkBackendClient,
} from "@sdkwork/iam-backend-sdk";
import { createIamSdkAdapters } from "@sdkwork/iam-sdk-adapter";
import { createSdkworkIamService, type SdkworkIamService } from "@sdkwork/iam-service";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createContext, useContext, type ReactNode } from "react";

/**
 * Console SDK wiring. This package owns exactly one thing: turning the host's
 * base URLs plus token manager into the generated clients the console
 * surfaces consume, and publishing them through a typed host port.
 *
 * Both clients are dependency-owned. The application lifecycle used to live here
 * as a resource registry over `webserver_application` / `webserver_source_version` /
 * `webserver_deployment`; that entity is owned by sdkwork-deployments (`deploy_app`)
 * and every Applications page is now the canonical deployments page bridged by
 * `@sdkwork/webserver-pc-console-delivery`. The client this host constructs is
 * the deployments generated client as well, so no console surface reaches the
 * legacy webserver app-api face any more.
 *
 * The IAM cloud account center mounts on the same terms. `iam_provider_account`
 * is an IAM-owned resource served by the IAM backend API, and IAM's generated
 * clients may only be composed by a host *core* package: a capability package
 * consumes SDK/service ports through core public exports and never imports a
 * generated SDK (`verify-repo` enforces that boundary). This is that export —
 * the IAM app and backend clients, their port adapters, and the
 * `SdkworkIamService` facade built from them — so the cloud account capability
 * package is handed a service instead of a transport.
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

/**
 * Base URLs the console clients are built against.
 *
 * Each URL is the origin the hosting page is served from. On the standalone Web
 * Server edge the gateway serves the SPAs and every API on one origin, so the
 * host passes its own runtime configuration straight through.
 */
export interface WebserverConsoleSdkBaseUrls {
  /** IAM app base URL; the IAM service port requires an app client even when a page only calls the backend face. */
  appbaseAppApiBaseUrl: string;
  /** Base URL of the IAM backend API that owns `iam_provider_account` and its credentials. */
  backendApiBaseUrl: string;
  deployAppApiBaseUrl: string;
  driveAppApiBaseUrl: string;
}

/**
 * IAM transport plus the service facade the console hands to IAM-owned pages.
 *
 * The two clients stay exposed because the host registers every generated client
 * with the session auth boundary, so a 401 from the IAM backend API clears the
 * session exactly like one from the deployments or drive clients.
 */
export interface WebserverConsoleIamClients {
  app: SdkworkAppClient;
  backend: SdkworkBackendClient;
  service: SdkworkIamService;
}

export interface WebserverConsoleSdkClients {
  deploy: SdkworkDeployAppClient;
  drive: SdkworkDriveAppClient;
  iam: WebserverConsoleIamClients;
}

const Context = createContext<WebserverConsoleSdkClients | null>(null);

export function createWebserverConsoleSdkClient(baseUrl: string, tokenManager: AuthTokenManager): WebserverConsoleSdkClient { return createDeployAppClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager }); }

/**
 * Composes the IAM app and backend clients against the host base URLs and adapts
 * them to the IAM SDK ports.
 *
 * The generated clients are structurally wider than the ports — they carry their
 * own query builders and transports — so `createIamSdkAdapters` narrows them
 * instead of a cast: the adapter is IAM's published boundary, and the service
 * facade only ever sees the port surface.
 */
export function createWebserverConsoleIamClients(baseUrls: WebserverConsoleSdkBaseUrls, tokenManager: AuthTokenManager): WebserverConsoleIamClients {
  const app = createIamAppClient({ authMode: "dual-token", baseUrl: baseUrls.appbaseAppApiBaseUrl, platform: "pc", tokenManager });
  const backend = createIamBackendClient({ authMode: "dual-token", baseUrl: baseUrls.backendApiBaseUrl, platform: "pc", tokenManager });
  const adapters = createIamSdkAdapters({ appbaseApp: app, appbaseBackend: backend });
  if (!adapters.appbaseBackend) {
    throw new Error("The IAM backend SDK adapter is required to manage IAM-owned resources.");
  }
  return {
    app,
    backend,
    service: createSdkworkIamService({
      appbaseAppClient: adapters.appbaseApp,
      appbaseBackendClient: adapters.appbaseBackend,
    }),
  };
}

export function createWebserverConsoleSdkClients(baseUrls: WebserverConsoleSdkBaseUrls, tokenManager: AuthTokenManager): WebserverConsoleSdkClients {
  let iamClients: WebserverConsoleIamClients | undefined;
  return {
    deploy: createWebserverConsoleSdkClient(baseUrls.deployAppApiBaseUrl, tokenManager),
    drive: createDriveAppClient({ baseUrl: baseUrls.driveAppApiBaseUrl, authMode: "dual-token", platform: "pc", tokenManager }),
    // The IAM face is composed on first access rather than eagerly. It validates
    // the generated IAM clients against IAM's own standard registry, so it fails
    // for as long as that registry declares an operation the generated SDK does
    // not carry yet. The console shell, its menu, and every deployments/drive
    // page are independent of it, and a throw here would abort the whole console
    // bootstrap — which, through the `use()` call in the workspace surface,
    // surfaces as a loading screen that never resolves instead of an error.
    get iam() {
      if (!iamClients) {
        iamClients = createWebserverConsoleIamClients(baseUrls, tokenManager);
      }
      return iamClients;
    },
  };
}
export function WebserverConsoleSdkProvider({ children, clients }: { children: ReactNode; clients: WebserverConsoleSdkClients }) { return <Context.Provider value={clients}>{children}</Context.Provider>; }
export function useWebserverConsoleSdk(): WebserverConsoleSdkClients { const clients = useContext(Context); if (!clients) throw new Error("WebserverConsoleSdkProvider is required"); return clients; }
