export interface DomainDnsRecordStatusRequest {
  /** `true` resumes the record; `false` pauses it (paused resolution). */
  enabled: boolean;
}
