import type { ApplicationSourceVersionResponse } from './application-source-version-response';
import type { PageInfo } from './page-info';

export interface ApplicationsSourceVersionsListResponse {
  code: 0;
  data: unknown & { items: ApplicationSourceVersionResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
