use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct UpsertDomainWechatVerificationRequest {
    /// 平台发放的文件名,必须是 .txt 结尾
    #[serde(rename = "fileName")]
    pub file_name: String,

    /// 文件原文,逐字节保存与返回
    pub content: String,
}
