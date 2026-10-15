import type { DomainWechatVerificationCheckResponse } from './domain-wechat-verification-check-response';

export interface RootDomainsWechatVerificationCheckResponse {
  code: 0;
  data: unknown & { item: DomainWechatVerificationCheckResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
