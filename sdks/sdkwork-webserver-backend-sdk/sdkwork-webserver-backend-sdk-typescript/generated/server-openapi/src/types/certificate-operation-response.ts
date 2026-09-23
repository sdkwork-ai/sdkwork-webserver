export interface CertificateOperationResponse {
  id: string;
  certificateId: string;
  operationType: 'ISSUE' | 'RENEW';
  status: 'PENDING' | 'RUNNING' | 'SUCCEEDED' | 'FAILED';
  attemptCount: number;
  maxAttempts: number;
  nextAttemptAt: string;
  failureCode?: string;
  /** What the failure said, as the named provider worded it, redacted and bounded. The code classifies a failure; this is the part an operator acts on. */
  failureDetail?: string;
  createdAt: string;
  updatedAt: string;
  completedAt?: string;
}
