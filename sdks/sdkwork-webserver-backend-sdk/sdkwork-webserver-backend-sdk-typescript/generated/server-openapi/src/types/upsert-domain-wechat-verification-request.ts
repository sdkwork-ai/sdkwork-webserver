export interface UpsertDomainWechatVerificationRequest {
  /** File name issued by the platform; must end with .txt. */
  fileName: string;
  /** Verbatim file content, stored and returned byte for byte. */
  content: string;
}
