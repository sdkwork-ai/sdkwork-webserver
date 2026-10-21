import type { NginxConfigRevisionSummary } from './nginx-config-revision-summary';
import type { PageInfo } from './page-info';

export interface ConfigsRevisionsListResponse {
  code: 0;
  data: unknown & { items: NginxConfigRevisionSummary[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
