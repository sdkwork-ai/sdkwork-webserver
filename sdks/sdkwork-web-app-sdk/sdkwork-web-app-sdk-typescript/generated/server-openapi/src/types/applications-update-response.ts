import type { ApplicationResponse } from './application-response';

export interface ApplicationsUpdateResponse {
  code: 0;
  data: unknown & { item: ApplicationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
