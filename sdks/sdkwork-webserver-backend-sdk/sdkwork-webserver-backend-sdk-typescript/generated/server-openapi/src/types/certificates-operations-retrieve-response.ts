import type { CertificateOperationResponse } from './certificate-operation-response';

export interface CertificatesOperationsRetrieveResponse {
  code: 0;
  data: unknown & { item: CertificateOperationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
