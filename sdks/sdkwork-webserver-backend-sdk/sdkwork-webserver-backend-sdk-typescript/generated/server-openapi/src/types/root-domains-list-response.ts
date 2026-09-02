import type { PageInfo } from './page-info';
import type { RootDomainResponse } from './root-domain-response';

export interface RootDomainsListResponse {
  code: 0;
  data: unknown & { items: RootDomainResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
