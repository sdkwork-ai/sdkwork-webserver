import type { ListenerCertificateBindingResponse } from './listener-certificate-binding-response';

export interface ApplicationsDomainsListenerCertificateBindingsCreateResponse201 {
  code: 0;
  data: unknown & { item: ListenerCertificateBindingResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
