use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainWechatVerificationCheckResponse {
    /// 是否有任一协议取回了 2xx 响应
    pub reachable: bool,

    /// 取回内容是否与保存的文件逐字节一致
    pub matched: bool,

    /// 命中方案的 HTTP 状态码
    #[serde(rename = "statusCode")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i64>,

    /// 命中方案(https 或 http)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,

    /// 未通过时的一句话事实
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// 自检时间
    #[serde(rename = "checkedAt")]
    pub checked_at: String,
}
