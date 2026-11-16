import type { DomainVerificationFileCheckResponse } from './domain-verification-file-check-response';

export interface RootDomainsVerificationFilesCheckResponse {
  code: 0;
  data: unknown & { item: DomainVerificationFileCheckResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
