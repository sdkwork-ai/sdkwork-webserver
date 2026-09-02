import type { DomainResponse } from './domain-response';

export interface ApplicationsDomainsRetrieveResponse {
  code: 0;
  data: unknown & { item: DomainResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
