export interface CertificateOperationResponse {
  id: string;
  certificateId: string;
  operationType: 'ISSUE' | 'RENEW';
  status: 'PENDING' | 'RUNNING' | 'SUCCEEDED' | 'FAILED';
  attemptCount: number;
  maxAttempts: number;
  nextAttemptAt: string;
  failureCode?: string;
  createdAt: string;
  updatedAt: string;
  completedAt?: string;
}
