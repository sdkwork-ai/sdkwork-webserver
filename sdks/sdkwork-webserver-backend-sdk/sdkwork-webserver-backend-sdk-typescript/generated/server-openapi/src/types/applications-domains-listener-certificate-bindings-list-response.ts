import type { ListenerCertificateBindingResponse } from './listener-certificate-binding-response';
import type { PageInfo } from './page-info';

export interface ApplicationsDomainsListenerCertificateBindingsListResponse {
  code: 0;
  data: unknown & { items: ListenerCertificateBindingResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
