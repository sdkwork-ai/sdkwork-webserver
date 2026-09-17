import {
  configureWebserverAppSdkBaseUrl,
  createWebserverHarmonySdkPort,
  getWebserverHarmonySdkPort,
  resetWebserverAppSdkBaseUrl,
  type WebserverAppSdkPort,
  type WebserverHarmonySdkPortConfig,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-core';

/**
 * SDK client construction for the HarmonyOS root.
 *
 * Authority: `APP_SDK_INTEGRATION_SPEC.md` and
 * `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6. Concrete clients are constructed
 * here (bootstrap only) and injected into feature packages; capability packages
 * must never construct one.
 */
export class WebserverHarmonySdkClients {
  private webserverPort: WebserverAppSdkPort | null = null;

  initialize(appApiBaseUrl: string, accessToken?: string, authToken?: string): void {
    configureWebserverAppSdkBaseUrl(appApiBaseUrl);
    const config: WebserverHarmonySdkPortConfig = {
      baseUrl: appApiBaseUrl,
      accessToken,
      authToken,
    };
    this.webserverPort = createWebserverHarmonySdkPort(config);
  }

  get webserver(): WebserverAppSdkPort {
    if (this.webserverPort === null) {
      throw new Error('Webserver app SDK port must be initialized by bootstrap before use');
    }
    return this.webserverPort;
  }

  reset(): void {
    resetWebserverAppSdkBaseUrl();
    this.webserverPort = null;
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
export type { WebserverAppSdkPort };
export { getWebserverHarmonySdkPort };
