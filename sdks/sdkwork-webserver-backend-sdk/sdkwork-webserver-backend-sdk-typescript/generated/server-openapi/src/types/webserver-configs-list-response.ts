import type { WebserverConfigCatalog } from './webserver-config-catalog';

export interface WebserverConfigsListResponse {
  code: 0;
  data: unknown & WebserverConfigCatalog;
  /** Server-owned request correlation id. */
  traceId: string;
}
