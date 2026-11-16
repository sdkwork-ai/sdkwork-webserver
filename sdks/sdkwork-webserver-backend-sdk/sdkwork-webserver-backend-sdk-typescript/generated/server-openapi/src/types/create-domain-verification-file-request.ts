export interface CreateDomainVerificationFileRequest {
  /** Issuing platform. */
  platform: 'WECHAT_OFFICIAL_ACCOUNT' | 'WECHAT_MINI_PROGRAM' | 'ALIPAY' | 'BYTEDANCE' | 'APPLE' | 'GOOGLE' | 'CUSTOM';
  /** Single-segment ASCII file name ending with .txt, .html or .xml. */
  fileName: string;
  /** Verbatim file content, stored and returned byte for byte. */
  content: string;
}
