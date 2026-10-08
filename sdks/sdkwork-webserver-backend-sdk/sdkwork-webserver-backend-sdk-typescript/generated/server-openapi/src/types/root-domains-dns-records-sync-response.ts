import type { DomainDnsSyncResponse } from './domain-dns-sync-response';

export interface RootDomainsDnsRecordsSyncResponse {
  code: 0;
  data: unknown & { item: DomainDnsSyncResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
