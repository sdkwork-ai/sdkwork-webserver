//! 域名归属校验文件：控制面的存储、归一化与自检。
//!
//! 各平台都要求域名根路径可公开取回一个自己签发的文件，然后凭这份文件确认
//! 域名归属：微信公众号 / 小程序（JS 接口安全域名、网页授权域名、业务域名）、
//! 支付宝与抖音小程序、Google Search Console（HTML 文件）、Apple 的
//! `apple-developer-domain-association.txt`、Bing 一类站点验证（XML 文件）。
//! 它们的机制相同、只有文件形态不同,所以这里是一个通用能力而不是"微信专用
//! 上传口"：一个根域名可以同时登记多份。
//!
//! 控制台上传文件原文,边缘数据面对该根域名及其子域名按请求主机名原样提供;
//! 自检按平台爬虫的视角取回比对。内容逐字节存储与返回——平台按精确内容比对,
//! 任何改写都会把一次本可通过的验证变成"内容不匹配"。

use chrono::Utc;
use sdkwork_webserver_acme_service::DnsApiClient;
use sdkwork_webserver_contract::{
    CreateDomainVerificationFileRequest, DomainVerificationFileCheckResponse,
    DomainVerificationFilePage, DomainVerificationFileResponse, WebServiceError, WebServiceResult,
};

use super::WebService;

/// 文件名上限（含扩展名）。平台签发的名字远短于此；上限只为封住滥用。
pub(crate) const VERIFICATION_FILE_NAME_MAX: usize = 128;
/// 内容上限（字节）。平台签发的校验文件只有几行；4096 覆盖任何已知形态。
pub(crate) const VERIFICATION_FILE_CONTENT_MAX: usize = 4096;

/// 平台封闭集合，与 DDL 的 `CHECK`、契约枚举、控制台选择项同源。
pub const VERIFICATION_PLATFORMS: &[&str] = &[
    "WECHAT_OFFICIAL_ACCOUNT",
    "WECHAT_MINI_PROGRAM",
    "ALIPAY",
    "BYTEDANCE",
    "APPLE",
    "GOOGLE",
    "CUSTOM",
];

/// 校验文件实际使用的扩展名；边缘按扩展名推断 `Content-Type`。
pub const VERIFICATION_FILE_EXTENSIONS: [&str; 3] = ["txt", "html", "xml"];

/// 一个根域名最多登记多少份校验文件。
///
/// 边缘把命中主机名的整组文件缓存在进程内，所以这份上限首先是内存保护：平台
/// 数量有界（七种），同一平台换发的新旧文件也只有两份，32 足够任何真实用法。
pub const MAX_VERIFICATION_FILES_PER_ZONE: usize = 32;

/// 一份已登记的校验文件，按仓储层交回的形态。
#[derive(Clone, Debug)]
pub struct VerificationFileRecord {
    /// 文件自身的 uuid：删除与自检都按它寻址。
    pub id: String,
    pub platform: String,
    pub file_name: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

fn normalize_platform(value: &str) -> WebServiceResult<String> {
    let value = value.trim().to_ascii_uppercase();
    if !VERIFICATION_PLATFORMS.contains(&value.as_str()) {
        return Err(WebServiceError::validation(format!(
            "platform must be one of {}",
            VERIFICATION_PLATFORMS.join(", ")
        )));
    }
    Ok(value)
}

fn normalize_file_name(value: &str) -> WebServiceResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > VERIFICATION_FILE_NAME_MAX {
        return Err(WebServiceError::validation(
            "file name must be 1..128 bytes",
        ));
    }
    let (stem, extension) = match trimmed.rsplit_once('.') {
        Some(parts) => parts,
        None => ("", ""),
    };
    if !VERIFICATION_FILE_EXTENSIONS.contains(&extension) {
        return Err(WebServiceError::validation(
            "file name must end with .txt, .html or .xml",
        ));
    }
    // 名字必须以字母数字开头：裸 ".txt" 意味着名为空，任何平台都不签发。
    if !stem
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return Err(WebServiceError::validation(
            "file name must start with an ASCII letter or digit",
        ));
    }
    // 逐字符白名单封住路径逃逸；平台只签发单段文件名。
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(WebServiceError::validation(
            "file name must be ASCII letters, digits, dot, hyphen or underscore",
        ));
    }
    // 保存平台发放的原名（大小写保留）：边缘按精确名称服务，名字不做大小写
    // 归一。
    Ok(trimmed.to_owned())
}

fn normalize_content(value: &str) -> WebServiceResult<String> {
    if value.is_empty() {
        return Err(WebServiceError::validation("content must not be empty"));
    }
    if value.len() > VERIFICATION_FILE_CONTENT_MAX {
        return Err(WebServiceError::validation(
            "content must not exceed 4096 bytes",
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

fn to_response(
    root_domain_id: &str,
    record: VerificationFileRecord,
) -> DomainVerificationFileResponse {
    DomainVerificationFileResponse {
        id: record.id,
        root_domain_id: root_domain_id.to_owned(),
        platform: record.platform,
        file_name: record.file_name,
        content: record.content,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// Refuses a *new* file name once the root domain holds
/// [`MAX_VERIFICATION_FILES_PER_ZONE`]; replacing an existing name is always
/// allowed, because a platform reissuing its file must not be blocked by the cap.
pub(crate) fn ensure_verification_file_capacity(
    existing_file_names: &[String],
    file_name: &str,
) -> WebServiceResult<()> {
    if existing_file_names.len() >= MAX_VERIFICATION_FILES_PER_ZONE
        && !existing_file_names.iter().any(|name| name == file_name)
    {
        return Err(WebServiceError::conflict(format!(
            "a root domain may publish at most {MAX_VERIFICATION_FILES_PER_ZONE} verification files; \
             delete one before adding another"
        )));
    }
    Ok(())
}

impl WebService {
    /// 列出根域名已登记的全部校验文件。
    pub(crate) async fn list_verification_files(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
    ) -> WebServiceResult<DomainVerificationFilePage> {
        let records = self
            .repository
            .list_root_domain_verification_files(tenant_id, root_domain_id)
            .await?;
        let items: Vec<DomainVerificationFileResponse> = records
            .into_iter()
            .map(|record| to_response(root_domain_id, record))
            .collect();
        let total = items.len() as i64;
        let page_size = items.len().max(1) as i32;
        Ok(DomainVerificationFilePage {
            items,
            total,
            page: 1,
            page_size,
        })
    }

    /// 登记（或替换）一份校验文件。同名文件整体替换：平台重新生成校验文件时，
    /// 旧的文件名与内容同时作废。
    pub(crate) async fn create_verification_file(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        request: &CreateDomainVerificationFileRequest,
    ) -> WebServiceResult<DomainVerificationFileResponse> {
        let platform = normalize_platform(&request.platform)?;
        let file_name = normalize_file_name(&request.file_name)?;
        let content = normalize_content(&request.content)?;
        let existing: Vec<String> = self
            .repository
            .list_root_domain_verification_files(tenant_id, root_domain_id)
            .await?
            .into_iter()
            .map(|record| record.file_name)
            .collect();
        ensure_verification_file_capacity(&existing, &file_name)?;
        let record = self
            .repository
            .upsert_root_domain_verification_file(
                tenant_id,
                root_domain_id,
                &platform,
                &file_name,
                &content,
            )
            .await?;
        Ok(to_response(root_domain_id, record))
    }

    /// 删除一份校验文件；文件 id 不存在时 404。
    pub(crate) async fn delete_verification_file(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        file_id: &str,
    ) -> WebServiceResult<()> {
        if !self
            .repository
            .delete_root_domain_verification_file(tenant_id, root_domain_id, file_id)
            .await?
        {
            return Err(WebServiceError::not_found(
                "no verification file with this id is registered on the root domain",
            ));
        }
        Ok(())
    }

    /// 自检：按平台爬虫的视角取回 `https://<apex>/<fileName>`（失败再试
    /// http），与保存内容逐字节比对。https 命中即采信；内容不匹配时两个
    /// 方案都试完，操作员拿到的是两种协议各自的事实。自检从不抛错。
    pub(crate) async fn check_verification_file(
        &self,
        tenant_id: i64,
        root_domain_id: &str,
        file_id: &str,
    ) -> WebServiceResult<DomainVerificationFileCheckResponse> {
        let record = self
            .repository
            .root_domain_verification_file(tenant_id, root_domain_id, file_id)
            .await?;
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
            let url = format!("{scheme}://{apex}/{}", record.file_name);
            let outcome = client.get_text(&url).await;
            match outcome {
                Ok((status, body)) if (200..300).contains(&status) => {
                    if body == record.content {
                        return Ok(DomainVerificationFileCheckResponse {
                            reachable: true,
                            matched: true,
                            status_code: Some(i32::from(status)),
                            scheme: Some(scheme.to_owned()),
                            detail: None,
                            checked_at,
                        });
                    }
                    if reachable.is_none() {
                        reachable = Some((i32::from(status), scheme));
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
        Ok(DomainVerificationFileCheckResponse {
            reachable: reachable.is_some(),
            matched: false,
            status_code,
            scheme,
            detail: Some(details.join("; ")),
            checked_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_verification_file_capacity, normalize_content, normalize_file_name,
        normalize_platform, MAX_VERIFICATION_FILES_PER_ZONE,
    };

    #[test]
    fn file_names_accept_the_shipped_platform_shapes() {
        assert_eq!(
            normalize_file_name(" MP_verify_aB123.txt ").unwrap(),
            "MP_verify_aB123.txt"
        );
        assert_eq!(
            normalize_file_name("apple-developer-domain-association.txt").unwrap(),
            "apple-developer-domain-association.txt"
        );
        assert_eq!(
            normalize_file_name("google1a2b3c4d5e6f7g8h.html").unwrap(),
            "google1a2b3c4d5e6f7g8h.html"
        );
        assert_eq!(
            normalize_file_name("BingSiteAuth.xml").unwrap(),
            "BingSiteAuth.xml"
        );
        for bad in [
            "",
            "no-extension",
            ".txt",
            "spaces in name.txt",
            "中文.txt",
            "/path/MP_verify.txt",
            "a.txt.exe",
            "a.php",
            "a.TXT",
        ] {
            assert!(normalize_file_name(bad).is_err(), "{bad} must be refused");
        }
        let long = format!("{}.txt", "a".repeat(125));
        assert!(normalize_file_name(&long).is_err());
    }

    #[test]
    fn content_is_bounded_and_kept_verbatim() {
        assert_eq!(normalize_content("token-abc").unwrap(), "token-abc");
        assert_eq!(normalize_content("token\n").unwrap(), "token\n");
        assert!(normalize_content("").is_err());
        assert!(normalize_content(&"x".repeat(4097)).is_err());
        assert!(normalize_content(&"x".repeat(4096)).is_ok());
        assert!(normalize_content("ok\u{0}bad").is_err());
        let html = "<html>\n<body>\ngoogle-site-verification: google123.html\n</body>\n</html>\n";
        assert_eq!(normalize_content(html).unwrap(), html);
    }

    #[test]
    fn platforms_are_a_closed_set_normalized_to_wire_codes() {
        assert_eq!(
            normalize_platform("wechat_mini_program").unwrap(),
            "WECHAT_MINI_PROGRAM"
        );
        assert_eq!(normalize_platform("custom").unwrap(), "CUSTOM");
        assert!(normalize_platform("").is_err());
        assert!(normalize_platform("WECHAT").is_err());
    }

    #[test]
    fn a_new_file_is_refused_once_the_zone_is_full() {
        let full: Vec<String> = (0..MAX_VERIFICATION_FILES_PER_ZONE)
            .map(|index| format!("MP_verify_{index}.txt"))
            .collect();
        assert!(ensure_verification_file_capacity(&full, "MP_verify_new.txt").is_err());
        assert!(ensure_verification_file_capacity(&full, "MP_verify_0.txt").is_ok());
        let roomy: Vec<String> = full
            .iter()
            .take(MAX_VERIFICATION_FILES_PER_ZONE - 1)
            .cloned()
            .collect();
        assert!(ensure_verification_file_capacity(&roomy, "MP_verify_new.txt").is_ok());
    }
}
