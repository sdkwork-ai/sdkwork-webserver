import type { DomainVerificationFileResponse } from './domain-verification-file-response';

export interface RootDomainsVerificationFilesCreateResponse201 {
  code: 0;
  data: unknown & { item: DomainVerificationFileResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
