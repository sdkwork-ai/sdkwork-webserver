import type { CertificateDistributionResponse } from './certificate-distribution-response';
import type { PageInfo } from './page-info';

export interface CertificatesDistributionListResponse {
  code: 0;
  data: unknown & { items: CertificateDistributionResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
