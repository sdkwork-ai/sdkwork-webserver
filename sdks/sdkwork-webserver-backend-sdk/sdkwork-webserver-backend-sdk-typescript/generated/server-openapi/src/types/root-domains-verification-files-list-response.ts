import type { DomainVerificationFileResponse } from './domain-verification-file-response';
import type { PageInfo } from './page-info';

export interface RootDomainsVerificationFilesListResponse {
  code: 0;
  data: unknown & { items: DomainVerificationFileResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
