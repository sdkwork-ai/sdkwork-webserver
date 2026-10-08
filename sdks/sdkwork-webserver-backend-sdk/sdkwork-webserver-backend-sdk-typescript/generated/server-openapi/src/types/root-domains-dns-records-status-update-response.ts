import type { DomainDnsRecordResponse } from './domain-dns-record-response';

export interface RootDomainsDnsRecordsStatusUpdateResponse {
  code: 0;
  data: unknown & { item: DomainDnsRecordResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
