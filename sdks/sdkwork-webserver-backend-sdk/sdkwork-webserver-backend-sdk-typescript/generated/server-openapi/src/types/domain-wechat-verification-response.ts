export interface DomainWechatVerificationResponse {
  rootDomainId: string;
  /** 平台发放的文件名(如 MP_verify_xxx.txt),按精确名称服务 */
  fileName?: string;
  /** 文件原文,逐字节保存与返回 */
  content?: string;
  /** 最近一次上传/替换的时间 */
  updatedAt?: string;
}
