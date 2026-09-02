import type { RootDomainResponse } from './root-domain-response';

export interface RootDomainsCreateResponse201 {
  code: 0;
  data: unknown & { item: RootDomainResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
