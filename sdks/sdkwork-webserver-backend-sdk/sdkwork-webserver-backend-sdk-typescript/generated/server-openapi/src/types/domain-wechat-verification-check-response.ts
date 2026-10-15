export interface DomainWechatVerificationCheckResponse {
  /** 是否有任一协议取回了 2xx 响应 */
  reachable: boolean;
  /** 取回内容是否与保存的文件逐字节一致 */
  matched: boolean;
  /** 命中方案的 HTTP 状态码 */
  statusCode?: number;
  /** 命中方案(https 或 http) */
  scheme?: string;
  /** 未通过时的一句话事实 */
  detail?: string;
  /** 自检时间 */
  checkedAt: string;
}
