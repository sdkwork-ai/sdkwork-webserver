export interface DomainWechatVerificationResponse {
  rootDomainId: string;
  /** File name issued by the platform (for example MP_verify_xxx.txt); served by exact name. */
  fileName?: string;
  /** Verbatim file content, stored and returned byte for byte. */
  content?: string;
  /** Time of the most recent upload or replacement. */
  updatedAt?: string;
}
