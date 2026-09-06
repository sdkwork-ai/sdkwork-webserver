import type { WebserverConfigFile } from './webserver-config-file';

export interface WebserverConfigsRetrieveResponse {
  code: 0;
  data: unknown & WebserverConfigFile;
  /** Server-owned request correlation id. */
  traceId: string;
}
