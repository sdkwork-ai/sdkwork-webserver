import type { DnsAccountResponse } from './dns-account-response';
import type { PageInfo } from './page-info';

export interface DnsAccountsListResponse {
  code: 0;
  data: unknown & { items: DnsAccountResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
