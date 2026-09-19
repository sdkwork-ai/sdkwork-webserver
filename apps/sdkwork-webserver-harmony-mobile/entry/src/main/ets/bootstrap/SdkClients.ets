import {
  configureWebserverDeployAppSdkBaseUrl,
  createWebserverHarmonySdkPort,
  getWebserverHarmonySdkPort,
  resetWebserverDeployAppSdkBaseUrl,
  type WebserverDeployAppSdkPort,
  type WebserverHarmonySdkPortConfig,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-core';

/**
 * SDK client construction for the HarmonyOS root.
 *
 * Authority: `APP_SDK_INTEGRATION_SPEC.md` and
 * `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6. Concrete clients are constructed
 * here (bootstrap only) and injected into feature packages; capability packages
 * must never construct one.
 *
 * The `deploy_app` entity is owned by `sdkwork-deployments`, so this root binds
 * the deployments-owned port and not a second, webserver-owned application
 * surface (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7).
 */
export class WebserverHarmonySdkClients {
  private deployAppPort: WebserverDeployAppSdkPort | null = null;

  initialize(appApiBaseUrl: string, accessToken?: string, authToken?: string): void {
    configureWebserverDeployAppSdkBaseUrl(appApiBaseUrl);
    const config: WebserverHarmonySdkPortConfig = {
      baseUrl: appApiBaseUrl,
      accessToken,
      authToken,
    };
    this.deployAppPort = createWebserverHarmonySdkPort(config);
  }

  get deployApp(): WebserverDeployAppSdkPort {
    if (this.deployAppPort === null) {
      throw new Error('the deploy_app SDK port must be initialized by bootstrap before use');
    }
    return this.deployAppPort;
  }

  reset(): void {
    resetWebserverDeployAppSdkBaseUrl();
    this.deployAppPort = null;
  }
}

let sdkClients: WebserverHarmonySdkClients | null = null;

export function createWebserverHarmonySdkClients(): WebserverHarmonySdkClients {
  sdkClients = new WebserverHarmonySdkClients();
  return sdkClients;
}

export function getWebserverHarmonySdkClients(): WebserverHarmonySdkClients {
  if (sdkClients === null) {
    return createWebserverHarmonySdkClients();
  }
  return sdkClients;
}

export function resetWebserverHarmonySdkClients(): void {
  sdkClients = null;
}

/** Re-export so capability packages can name the injected port type. */
export type { WebserverDeployAppSdkPort };
export { getWebserverHarmonySdkPort };
