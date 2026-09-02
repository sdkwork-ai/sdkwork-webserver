import type { CertificateResponse } from './certificate-response';
import type { PageInfo } from './page-info';

export interface CertificatesListResponse {
  code: 0;
  data: unknown & { items: CertificateResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
