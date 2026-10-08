export interface DomainDnsRecordStatusRequest {
  /** `true` resumes the record; `false` pauses it (暂停解析). */
  enabled: boolean;
}
