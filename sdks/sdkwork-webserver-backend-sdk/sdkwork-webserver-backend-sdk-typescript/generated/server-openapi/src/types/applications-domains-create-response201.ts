import type { ApplicationDomainResponse } from './application-domain-response';

export interface ApplicationsDomainsCreateResponse201 {
  code: 0;
  data: unknown & { item: ApplicationDomainResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
