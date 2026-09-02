import type { NginxConfigResponse } from './nginx-config-response';
import type { PageInfo } from './page-info';

export interface ConfigsListResponse {
  code: 0;
  data: unknown & { items: NginxConfigResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
