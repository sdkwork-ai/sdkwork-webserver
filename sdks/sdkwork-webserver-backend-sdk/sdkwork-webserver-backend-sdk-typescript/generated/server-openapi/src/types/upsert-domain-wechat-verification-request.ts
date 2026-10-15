export interface UpsertDomainWechatVerificationRequest {
  /** 平台发放的文件名,必须是 .txt 结尾 */
  fileName: string;
  /** 文件原文,逐字节保存与返回 */
  content: string;
}
