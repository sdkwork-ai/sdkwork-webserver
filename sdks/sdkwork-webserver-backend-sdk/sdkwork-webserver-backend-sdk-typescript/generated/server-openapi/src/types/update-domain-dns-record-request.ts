export interface UpdateDomainDnsRecordRequest {
  recordType: 'A' | 'AAAA' | 'CNAME' | 'TXT' | 'MX' | 'NS' | 'CAA';
  host: string;
  recordValue: string;
  ttlSeconds?: number;
  priority?: number;
  recordLine?: string;
}
