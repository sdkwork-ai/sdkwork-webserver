import type { ApplicationResponse } from './application-response';

export interface ApplicationsRetrieveResponse {
  code: 0;
  data: unknown & { item: ApplicationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
