import type { CertificateResponse } from './certificate-response';

export interface CertificatesRevokeResponse {
  code: 0;
  data: unknown & CertificateResponse;
  /** Server-owned request correlation id. */
  traceId: string;
}
