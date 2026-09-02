export interface DomainVerifyResponse {
  verified: boolean;
  status: 'PENDING' | 'VERIFIED' | 'FAILED' | 'EXPIRED';
  method: 'DNS_TXT';
  recordName: string;
  recordValue: string;
  attemptCount: number;
  expiresAt: string;
  nextAttemptAt?: string;
  checkedAt?: string;
  failureCode?: string;
}
