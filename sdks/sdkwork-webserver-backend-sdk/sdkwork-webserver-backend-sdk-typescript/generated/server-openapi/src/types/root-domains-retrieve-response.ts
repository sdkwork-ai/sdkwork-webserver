import type { RootDomainResponse } from './root-domain-response';

export interface RootDomainsRetrieveResponse {
  code: 0;
  data: unknown & { item: RootDomainResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
