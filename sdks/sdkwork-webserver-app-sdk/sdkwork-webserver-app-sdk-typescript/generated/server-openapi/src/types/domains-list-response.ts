import type { DomainResponse } from './domain-response';
import type { PageInfo } from './page-info';

export interface DomainsListResponse {
  code: 0;
  data: unknown & { items: DomainResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
