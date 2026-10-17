//! 微信公众号/小程序域名归属验证文件：控制面的存储、归一化与自检。
//!
//! 平台要求域名根路径可访问一个形如 `MP_verify_xxxxx.txt` 的校验文件后才能
//! 完成归属验证（JS 接口安全域名 / 网页授权域名 / 业务域名均如此）。控制台
//! 上传文件原文，边缘数据面对该根域名及其子域名按请求主机名原样提供；自检
//! 按微信爬虫的视角取回比对。内容逐字节存储与返回——微信按精确内容比对，
//! 任何改写都会把一次本可通过的验证变成"内容不匹配"。

use chrono::Utc;
use sdkwork_webserver_acme_service::DnsApiClient;
use sdkwork_webserver_contract::{
    DomainWechatVerificationCheckResponse, DomainWechatVerificationResponse,
    UpsertDomainWechatVerificationRequest, WebServiceError, WebServiceResult,
};

use super::WebService;

/// 文件名上限（含 `.txt` 后缀）。微信生成的文件名远短于此；上限只为封住滥用。
pub(crate) const WECHAT_VERIFICATION_FILE_NAME_MAX: usize = 64;
/// 内容上限。微信校验文件是单行 token，2048 字节已远超任何平台发放的值。
pub(crate) const WECHAT_VERIFICATION_CONTENT_MAX: usize = 2048;

fn normalize_file_name(value: &str) -> WebServiceResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > WECHAT_VERIFICATION_FILE_NAME_MAX {
        return Err(WebServiceError::validation("file name must be 1..64 bytes"));
    }
    let lowered = trimmed.to_ascii_lowercase();
    if !lowered.ends_with(".txt") {
        return Err(WebServiceError::validation("file name must end with .txt"));
    }
    // 微信历史上发放过带下划线与连字符的名字；语义上只要求"名字以 .txt
    // 结尾且不含路径分隔符"，逐字符白名单封住路径逃逸。
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(WebServiceError::validation(
            "file name must be ASCII letters, digits, dot, hyphen or underscore",
        ));
    }
    // 保存平台发放的原名（大小写保留）：边缘按精确名称服务，微信的名字
    // 不做大小写归一。
    Ok(trimmed.to_owned())
}

fn normalize_content(value: &str) -> WebServiceResult<String> {
    if value.is_empty() {
        return Err(WebServiceError::validation("content must not be empty"));
    }
    if value.len() > WECHAT_VERIFICATION_CONTENT_MAX {
        return Err(WebServiceError::validation(
            "content must not exceed 2048 bytes",
        ));
    }
    // 控制字符（换行/回车/制表除外）会在响应体里产生不可预期字节；直接
    // 拒绝而不是悄悄剥离，剥离本身就是对"逐字节比对"承诺的破坏。
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(WebServiceError::validation(
            "content must not contain control characters",
        ));
    }
    Ok(value.to_owned())
}

impl WebService {
    /// 读取根域名的微信验证文件；未配置时返回缺省响应。
    pub(crate) async fn retrieve_wechat_verification(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<DomainWechatVerificationResponse> {
        match self
            .repository
            .root_domain_wechat_verification(tenant_id, root_domain_id)
            .await?
        {
            Some((file_name, content, updated_at)) => Ok(DomainWechatVerificationResponse {
                root_domain_id: root_domain_id.to_owned(),
                file_name: Some(file_name),
                content: Some(content),
                updated_at: Some(updated_at),
            }),
            None => Ok(DomainWechatVerificationResponse {
                root_domain_id: root_domain_id.to_owned(),
                ..Default::default()
            }),
        }
    }

    /// 上传（或替换）微信验证文件。替换即整体覆盖：微信重新生成校验文件时，
    /// 旧的文件名与内容同时作废。
    pub(crate) async fn upsert_wechat_verification(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &UpsertDomainWechatVerificationRequest,
    ) -> WebServiceResult<DomainWechatVerificationResponse> {
        let file_name = normalize_file_name(&request.file_name)?;
        let content = normalize_content(&request.content)?;
        let updated_at = self
            .repository
            .upsert_root_domain_wechat_verification(tenant_id, root_domain_id, &file_name, &content)
            .await?;
        Ok(DomainWechatVerificationResponse {
            root_domain_id: root_domain_id.to_owned(),
            file_name: Some(file_name),
            content: Some(content),
            updated_at: Some(updated_at),
        })
    }

    /// 删除微信验证文件；未配置时 404。
    pub(crate) async fn delete_wechat_verification(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<()> {
        if !self
            .repository
            .delete_root_domain_wechat_verification(tenant_id, root_domain_id)
            .await?
        {
            return Err(WebServiceError::not_found(
                "no WeChat verification file is configured for this root domain",
            ));
        }
        Ok(())
    }

    /// 自检：按微信爬虫的视角取回 `https://<apex>/<fileName>`（失败再试
    /// http），与保存内容逐字节比对。https 命中即采信；内容不匹配时两个
    /// 方案都试完，操作员拿到的是两种协议各自的事实。自检从不抛错。
    pub(crate) async fn check_wechat_verification(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<DomainWechatVerificationCheckResponse> {
        let (file_name, content, _updated_at) = self
            .repository
            .root_domain_wechat_verification(tenant_id, root_domain_id)
            .await?
            .ok_or_else(|| {
                WebServiceError::not_found(
                    "no WeChat verification file is configured for this root domain",
                )
            })?;
        let apex = self
            .repository
            .retrieve_root_domain(tenant_id, root_domain_id)
            .await?
            .hostname;
        let checked_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let https = DnsApiClient::new().map_err(|error| {
            WebServiceError::Internal(format!("probe client unavailable: {error}"))
        })?;
        let http = DnsApiClient::new_permitting_plaintext().map_err(|error| {
            WebServiceError::Internal(format!("probe client unavailable: {error}"))
        })?;

        let mut reachable: Option<(i32, &'static str)> = None;
        let mut details: Vec<String> = Vec::new();
        for (scheme, client) in [("https", &https), ("http", &http)] {
            let url = format!("{scheme}://{apex}/{file_name}");
            let outcome = client.get_text(&url).await;
            match outcome {
                Ok((status, body)) if (200..300).contains(&status) => {
                    if body == content {
                        return Ok(DomainWechatVerificationCheckResponse {
                            reachable: true,
                            matched: true,
                            status_code: Some(i32::try_from(status).unwrap_or_default()),
                            scheme: Some(scheme.to_owned()),
                            detail: None,
                            checked_at,
                        });
                    }
                    if reachable.is_none() {
                        reachable = Some((i32::try_from(status).unwrap_or_default(), scheme));
                    }
                    details.push(format!(
                        "{scheme}: reachable (HTTP {status}) but content differs from the stored file"
                    ));
                }
                Ok((status, _)) => {
                    details.push(format!("{scheme}: HTTP {status}"));
                }
                Err(error) => {
                    details.push(format!("{scheme}: {error}"));
                }
            }
        }
        let (status_code, scheme) = match reachable {
            Some((code, scheme)) => (Some(code), Some(scheme.to_owned())),
            None => (None, None),
        };
        Ok(DomainWechatVerificationCheckResponse {
            reachable: reachable.is_some(),
            matched: false,
            status_code,
            scheme,
            detail: Some(details.join("; ")),
            checked_at,
        })
    }
}
