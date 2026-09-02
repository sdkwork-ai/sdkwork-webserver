import type { DomainVerifyResponse } from './domain-verify-response';

export interface ApplicationsDomainsVerifyResponse {
  code: 0;
  data: unknown & { item: DomainVerifyResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
