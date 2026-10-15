import type { DomainWechatVerificationResponse } from './domain-wechat-verification-response';

export interface RootDomainsWechatVerificationUpsertResponse {
  code: 0;
  data: unknown & { item: DomainWechatVerificationResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
