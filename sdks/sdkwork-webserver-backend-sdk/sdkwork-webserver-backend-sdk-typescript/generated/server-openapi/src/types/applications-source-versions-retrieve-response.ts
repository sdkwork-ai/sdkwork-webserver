import type { ApplicationSourceVersionResponse } from './application-source-version-response';

export interface ApplicationsSourceVersionsRetrieveResponse {
  code: 0;
  data: unknown & { item: ApplicationSourceVersionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
