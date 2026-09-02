import type { ApplicationResponse } from './application-response';
import type { PageInfo } from './page-info';

export interface ApplicationsListResponse {
  code: 0;
  data: unknown & { items: ApplicationResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
