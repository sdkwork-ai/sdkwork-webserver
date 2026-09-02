import type { PageInfo } from './page-info';
import type { ServerResponse } from './server-response';

export interface ServersListResponse {
  code: 0;
  data: unknown & { items: ServerResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
