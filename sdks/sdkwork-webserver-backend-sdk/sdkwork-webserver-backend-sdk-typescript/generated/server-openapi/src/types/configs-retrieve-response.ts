import type { NginxConfigResponse } from './nginx-config-response';

export interface ConfigsRetrieveResponse {
  code: 0;
  data: unknown & { item: NginxConfigResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
