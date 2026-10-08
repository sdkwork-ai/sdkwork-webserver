export interface CreateDomainDnsRecordRequest {
  recordType: 'A' | 'AAAA' | 'CNAME' | 'TXT' | 'MX' | 'NS' | 'CAA';
  /** Zone-relative 主机记录 (`@` for the apex, `www`, `api.eu`, `*`). */
  host: string;
  recordValue: string;
  ttlSeconds?: number;
  /** MX priority; rejected on types that carry none. */
  priority?: number;
  /** Provider resolution line; omitted = the provider default. */
  recordLine?: string;
}
