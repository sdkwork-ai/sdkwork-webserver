export interface DomainVerificationFileResponse {
  id: string;
  rootDomainId: string;
  /** Issuing platform. Grouping/audit only; the edge matches by file name. */
  platform: 'WECHAT_OFFICIAL_ACCOUNT' | 'WECHAT_MINI_PROGRAM' | 'ALIPAY' | 'BYTEDANCE' | 'APPLE' | 'GOOGLE' | 'CUSTOM';
  /** File name issued by the platform (for example MP_verify_xxx.txt, google123.html); served by exact name. */
  fileName: string;
  /** Verbatim file content, stored and returned byte for byte. */
  content: string;
  /** Time the file was first registered. */
  createdAt: string;
  /** Time of the most recent upload or replacement. */
  updatedAt: string;
}
