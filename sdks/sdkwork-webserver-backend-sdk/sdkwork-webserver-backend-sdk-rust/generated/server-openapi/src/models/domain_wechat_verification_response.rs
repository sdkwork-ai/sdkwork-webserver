use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DomainWechatVerificationResponse {
    #[serde(rename = "rootDomainId")]
    pub root_domain_id: String,

    /// 平台发放的文件名(如 MP_verify_xxx.txt),按精确名称服务
    #[serde(rename = "fileName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,

    /// 文件原文,逐字节保存与返回
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// 最近一次上传/替换的时间
    #[serde(rename = "updatedAt")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
