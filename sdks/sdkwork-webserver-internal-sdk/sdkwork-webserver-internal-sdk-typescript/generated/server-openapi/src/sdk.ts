import { HttpClient, createHttpClient } from './http/client';
import type { SdkworkCustomConfig } from './types/common';

import { RuntimeApi, createRuntimeApi } from './api/runtime';
import { ClusterApi, createClusterApi } from './api/cluster';

export class SdkworkCustomClient {
  private httpClient: HttpClient;

  public readonly runtime: RuntimeApi;
  public readonly cluster: ClusterApi;

  constructor(config: SdkworkCustomConfig) {
    this.httpClient = createHttpClient(config);
    this.runtime = createRuntimeApi(this.httpClient);

    this.cluster = createClusterApi(this.httpClient);
  }

  setApiKey(apiKey: string): this {
    this.httpClient.setApiKey(apiKey);
    return this;
  }
  get http(): HttpClient {
    return this.httpClient;
  }
}

export function createClient(config: SdkworkCustomConfig): SdkworkCustomClient {
  return new SdkworkCustomClient(config);
}

export default SdkworkCustomClient;
