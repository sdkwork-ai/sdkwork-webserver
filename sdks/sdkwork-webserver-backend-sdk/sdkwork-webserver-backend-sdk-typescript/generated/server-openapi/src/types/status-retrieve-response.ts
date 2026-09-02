import type { NginxStatusResponse } from './nginx-status-response';

export interface StatusRetrieveResponse {
  code: 0;
  data: unknown & { item: NginxStatusResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
