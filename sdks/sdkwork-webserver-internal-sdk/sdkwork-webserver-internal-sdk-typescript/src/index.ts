import {
  createClient as createGeneratedInternalClient,
  SdkworkCustomClient,
} from '../generated/server-openapi/src/index';
import type { SdkworkCustomConfig } from '../generated/server-openapi/src/types/common';

export {
  SdkworkCustomClient,
  SdkworkCustomClient as SdkworkInternalClient,
  createGeneratedInternalClient,
};
export type { SdkworkCustomConfig };
export type SdkworkInternalConfig = SdkworkCustomConfig;
export * from '../generated/server-openapi/src/types';
export * from '../generated/server-openapi/src/api';
export * from '../generated/server-openapi/src/http';
export * from '../generated/server-openapi/src/auth';

export function createClient(config: SdkworkCustomConfig): SdkworkCustomClient {
  return createGeneratedInternalClient(config);
}
