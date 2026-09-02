import type { ApplicationDomainResponse } from './application-domain-response';
import type { PageInfo } from './page-info';

export interface ApplicationsDomainsListResponse {
  code: 0;
  data: unknown & { items: ApplicationDomainResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
