import type { WebserverConfigWriteResult } from './webserver-config-write-result';

export interface WebserverConfigsUpdateResponse {
  code: 0;
  data: unknown & WebserverConfigWriteResult;
  /** Server-owned request correlation id. */
  traceId: string;
}
