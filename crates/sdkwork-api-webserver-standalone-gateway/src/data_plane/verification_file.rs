//! 域名归属校验文件的边缘服务。
//!
//! 各平台都要求在域名根路径可公开取回一个自己签发的文件后才能完成归属验证:
//! 微信公众号 / 小程序(JS 接口安全域名、网页授权域名、业务域名)、支付宝与抖音
//! 小程序、Google Search Console(.html)、Apple 的
//! `apple-developer-domain-association.txt`、Bing 一类站点验证(.xml)。文件内容
//! 来自控制面(控制台上传,存于 `deploy_dns_zone_verification_file` 与
//! `webserver_domain_verification_file`);standalone 部署形态下边缘与控制面
//! 共享进程与数据库,因此这里按主机名做进程内读取——与 `deploy_fallback` 的
//! 嵌入式查询同一信任级别,不需要新的分发通道。
//!
//! # 窄优先级,精确命中
//!
//! 只在"请求路径恰为单段 `.txt`/`.html`/`.xml` 根路径、且该主机名所属 zone
//! 登记过同名文件"时接管:命中即按字节原样返回,`Content-Type` 按扩展名推断;
//! 任何不命中都放行给后续路由,宿主应用自己的 `/x.txt` 不受影响。GET/HEAD 之外
//! 的动词在确有该文件时回答 405,没有文件时同样放行。
//!
//! # 进程级共享源
//!
//! 验证文件查询与 `process_shared_database_pool` 一样是进程级事实:数据面的每个
//! 监听器都读同一份,因此在网站数据面启动时装载一次,处理器经 [`shared_source`]
//! 读取,而不是把查询穿过多层公开的启动签名。

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Response, StatusCode},
};

/// 单个文件名的长度上限,与控制面一致。
const MAX_VERIFICATION_FILE_NAME_BYTES: usize = 128;

/// 命中缓存的正向寿命。验证文件极少变化;替换后至多等这么久生效。
const POSITIVE_TTL: Duration = Duration::from_secs(60);

/// 未命中(zone 没有文件)的负向寿命,短得多:上传后希望尽快可用。
const NEGATIVE_TTL: Duration = Duration::from_secs(10);

/// 缓存条目硬上限。
///
/// 条目按请求 Host 键入,而 Host 在面向互联网的监听器上是攻击者可控的:
/// TTL 只在"同名再次被请求"时惰性生效,没有条目上限的地图会随出现的不同主机名
/// 单调增长。写入时顺带清扫过期条目并在达到上限时按到期时间驱逐最早的条目,
/// 保证地图有界——被驱逐的主机下一条请求只是重新查询。
const MAX_CACHE_ENTRIES: usize = 4096;

/// 一个 zone 上报的一份域名归属校验文件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerificationFile {
    pub file_name: String,
    pub content: String,
}

/// 主机名 → 该 zone 登记的全部校验文件 的读取源。
#[async_trait]
pub(crate) trait VerificationFileSource: Send + Sync {
    /// 返回 `hostname` 所属 zone 的全部校验文件;该主机名没有可服务的文件时为
    /// `None`。查询失败同样以 `None` 作答——验证文件是窄优先级的附加能力,
    /// 数据库抖动不应把宿主应用自己的 404 变成边缘的 503。
    async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>>;
}

#[derive(Clone)]
struct CacheEntry {
    files: Option<Arc<Vec<VerificationFile>>>,
    expires_at: Instant,
}

/// 带主机名缓存的读取源。
///
/// 正向寿命长(文件几乎不变),负向寿命短(上传后希望尽快可用):两个 TTL 的差
/// 就是"上传到平台爬虫能取到"的最坏等待,与操作员在控制台点自检的节奏一致。
pub(crate) struct CachedVerificationFileSource {
    inner: Arc<dyn VerificationFileSource>,
    /// Guarded map, updated in place: an `ArcSwap` would clone the whole map
    /// on every insert, turning host diversity into O(n) copying per request.
    cache: std::sync::Mutex<HashMap<String, CacheEntry>>,
}

impl CachedVerificationFileSource {
    pub(crate) fn new(inner: Arc<dyn VerificationFileSource>) -> Self {
        Self {
            inner,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn cache_len(&self) -> usize {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

#[async_trait]
impl VerificationFileSource for CachedVerificationFileSource {
    async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>> {
        let now = Instant::now();
        // A live entry (positive or negative) answers without the inner
        // lookup; `Some(None)` is a cached negative and must not refetch.
        let cached: Option<Option<Arc<Vec<VerificationFile>>>> = {
            let cache = self
                .cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache
                .get(hostname)
                .filter(|entry| entry.expires_at > now)
                .map(|entry| entry.files.clone())
        };
        if let Some(files) = cached {
            return files.map(|files| (*files).clone());
        }
        let files = self.inner.lookup(hostname).await;
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // 清扫过期条目:TTL 的惰性生效在写入时补一次全图清扫,地图不会留住
        // 无人再问的死条目。
        cache.retain(|_, entry| entry.expires_at > now);
        if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(hostname) {
            // 容量封顶:全部条目都未过期时,驱逐最早到期的条目。
            let mut by_expiry: Vec<(String, Instant)> = cache
                .iter()
                .map(|(name, entry)| (name.clone(), entry.expires_at))
                .collect();
            by_expiry.sort_by_key(|(_, expires_at)| *expires_at);
            let evict = cache.len() + 1 - MAX_CACHE_ENTRIES;
            for (name, _) in by_expiry.into_iter().take(evict) {
                cache.remove(&name);
            }
        }
        cache.insert(
            hostname.to_owned(),
            CacheEntry {
                files: files.clone().map(Arc::new),
                expires_at: now + cache_ttl(&files),
            },
        );
        files
    }
}

/// 正向与负向寿命的差就是"上传到平台爬虫能取到"的最坏等待。
fn cache_ttl(files: &Option<Vec<VerificationFile>>) -> Duration {
    if files.is_some() {
        POSITIVE_TTL
    } else {
        NEGATIVE_TTL
    }
}

/// 生产读取源:共享 Deploy 数据库的进程内查询。
#[cfg(feature = "management")]
pub(crate) struct DeployVerificationFileSource {
    repository: sdkwork_api_webserver_assembly::DeployRepository,
}

#[cfg(feature = "management")]
impl DeployVerificationFileSource {
    pub(crate) fn new(repository: sdkwork_api_webserver_assembly::DeployRepository) -> Self {
        Self { repository }
    }
}

#[cfg(feature = "management")]
#[async_trait]
impl VerificationFileSource for DeployVerificationFileSource {
    async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>> {
        let rows = self
            .repository
            .verification_files_by_hostname_lookup(hostname)
            .await
            .ok()?;
        (!rows.is_empty()).then(|| {
            rows.into_iter()
                .map(|(file_name, content)| VerificationFile { file_name, content })
                .collect()
        })
    }
}

/// 生产读取源之二:Web Server 控制面自己的验证文件(admin 面上传,存于
/// `webserver_domain_verification_file`)。与 Deploy 源并列——两个控制面各自
/// 管理自己的域名登记,边缘按主机名先查 Deploy 库、再查 Web Server 库。
#[cfg(feature = "management")]
pub(crate) struct WebserverVerificationFileSource {
    pool: sqlx::PgPool,
}

#[cfg(feature = "management")]
impl WebserverVerificationFileSource {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "management")]
#[async_trait]
impl VerificationFileSource for WebserverVerificationFileSource {
    async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>> {
        let rows = sdkwork_api_webserver_assembly::verification_files_by_hostname_lookup(
            &self.pool, hostname,
        )
        .await
        .ok()?;
        (!rows.is_empty()).then(|| {
            rows.into_iter()
                .map(|(file_name, content)| VerificationFile { file_name, content })
                .collect()
        })
    }
}

/// 合并读取源:同一个主机名先问 Deploy 控制面,再问 Web Server 控制面。
/// 两个登记各自独立,命中其一即可;同名文件以先问到的为准。
#[cfg(feature = "management")]
pub(crate) struct MergedVerificationFileSource {
    first: Arc<dyn VerificationFileSource>,
    second: Arc<dyn VerificationFileSource>,
}

#[cfg(feature = "management")]
impl MergedVerificationFileSource {
    pub(crate) fn new(
        first: Arc<dyn VerificationFileSource>,
        second: Arc<dyn VerificationFileSource>,
    ) -> Self {
        Self { first, second }
    }
}

#[cfg(feature = "management")]
#[async_trait]
impl VerificationFileSource for MergedVerificationFileSource {
    async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>> {
        let first = self.first.lookup(hostname).await;
        let second = self.second.lookup(hostname).await;
        match (first, second) {
            (None, None) => None,
            (Some(files), None) => Some(files),
            (None, Some(files)) => Some(files),
            (Some(first), Some(second)) => {
                let mut merged = first;
                for file in second {
                    if !merged
                        .iter()
                        .any(|existing| existing.file_name == file.file_name)
                    {
                        merged.push(file);
                    }
                }
                Some(merged)
            }
        }
    }
}

static SHARED_SOURCE: ArcSwapOption<CachedVerificationFileSource> = ArcSwapOption::const_empty();

/// 装载进程级共享读取源(网站数据面启动时调用一次;重复装载以最后一次为准,
/// 与运行时生成的重启语义一致)。
#[cfg(feature = "management")]
pub(crate) fn install_shared_source(source: CachedVerificationFileSource) {
    SHARED_SOURCE.store(Some(Arc::new(source)));
}

/// 当前共享读取源;未装载(非 management 构建,或共享数据库不可用)时为
/// `None`,处理器直接放行。
pub(crate) fn shared_source() -> Option<Arc<CachedVerificationFileSource>> {
    SHARED_SOURCE.load_full()
}

/// 尝试按域名归属校验文件应答一个请求。
///
/// `hostname` 是请求 Host 里的裸主机名(已归一化),`path` 是归一化请求路径。
/// 只接管"单段 `.txt`/`.html`/`.xml` 根路径 + 该主机名确有同名文件"的请求;
/// 其余一律返回 `None` 放行。
pub(crate) async fn serve_verification_file(
    source: &CachedVerificationFileSource,
    hostname: &str,
    path: &str,
    method: &str,
) -> Option<Response<Body>> {
    let file_name = requested_verification_file_name(path)?;
    let files = source.lookup(hostname).await?;
    let file = match files.iter().find(|file| file.file_name == file_name) {
        Some(file) => file,
        None => return None,
    };
    if !matches!(method, "GET" | "HEAD") {
        // 动词在命中的命名空间里仍然收口:验证文件是只读资源,写动词不应被
        // 转交给宿主应用,否则一个 PUT 就可能让平台爬到被改写的内容。
        return Some(text_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "method is not allowed\n",
        ));
    }
    let body = if method == "HEAD" {
        Body::empty()
    } else {
        Body::from(file.content.clone())
    };
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        content_type_for(&file.file_name),
    );
    // 校验窗口材料:绝不被中间缓存;HEAD 报告与 GET 相同的长度。
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    if method == "HEAD" {
        if let Ok(value) = axum::http::HeaderValue::from_str(&file.content.len().to_string()) {
            response
                .headers_mut()
                .insert(axum::http::header::CONTENT_LENGTH, value);
        }
    }
    Some(response)
}

/// `Content-Type` 按扩展名推断:各平台按原文取回,但 `.html` 校验文件在浏览器
/// 里被当成纯文本展示会误导操作员排障。
fn content_type_for(file_name: &str) -> axum::http::HeaderValue {
    if file_name.ends_with(".html") {
        axum::http::HeaderValue::from_static("text/html; charset=utf-8")
    } else if file_name.ends_with(".xml") {
        axum::http::HeaderValue::from_static("application/xml; charset=utf-8")
    } else {
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8")
    }
}

/// 请求路径恰为"单段校验文件根路径"时返回该文件名,否则 `None`。
///
/// 校验比控制面上传更宽一点:平台历史上发放过带下划线与连字符的名字,
/// 语义上平台只关心"名字与 zone 上报的完全一致"。
fn requested_verification_file_name(path: &str) -> Option<&str> {
    let file_name = path.strip_prefix('/')?;
    if file_name.is_empty() || file_name.contains('/') {
        return None;
    }
    let supported = [".txt", ".html", ".xml"]
        .iter()
        .any(|extension| file_name.ends_with(extension));
    if !supported
        || file_name.len() > MAX_VERIFICATION_FILE_NAME_BYTES
        || !file_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return None;
    }
    Some(file_name)
}

fn text_response(status: StatusCode, message: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MapSource {
        files: Mutex<HashMap<String, Vec<VerificationFile>>>,
    }

    #[async_trait]
    impl VerificationFileSource for MapSource {
        async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>> {
            self.files.lock().expect("lock").get(hostname).cloned()
        }
    }

    fn file(name: &str, content: &str) -> VerificationFile {
        VerificationFile {
            file_name: name.to_owned(),
            content: content.to_owned(),
        }
    }

    fn source_with(hostname: &str, files: Vec<VerificationFile>) -> CachedVerificationFileSource {
        let source = MapSource::default();
        source
            .files
            .lock()
            .expect("lock")
            .insert(hostname.to_owned(), files);
        CachedVerificationFileSource::new(Arc::new(source))
    }

    #[tokio::test]
    async fn an_exact_hit_is_served_verbatim() {
        let cached = source_with(
            "www.example.com",
            vec![file("MP_verify_abc.txt", "verification-token-1")],
        );
        let response =
            serve_verification_file(&cached, "www.example.com", "/MP_verify_abc.txt", "GET")
                .await
                .expect("an exact hit must be served");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/plain; charset=utf-8")
        );
    }

    #[tokio::test]
    async fn several_platform_files_coexist_on_one_host() {
        let cached = source_with(
            "www.example.com",
            vec![
                file("MP_verify_abc.txt", "verification-token-1"),
                file("google123.html", "<html>google-site-verification</html>"),
                file("BingSiteAuth.xml", "<users/>"),
            ],
        );
        for (path, expected) in [
            ("/MP_verify_abc.txt", "text/plain; charset=utf-8"),
            ("/google123.html", "text/html; charset=utf-8"),
            ("/BingSiteAuth.xml", "application/xml; charset=utf-8"),
        ] {
            let response = serve_verification_file(&cached, "www.example.com", path, "GET")
                .await
                .unwrap_or_else(|| panic!("{path} must be served"));
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response
                    .headers()
                    .get(axum::http::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok()),
                Some(expected)
            );
        }
    }

    #[tokio::test]
    async fn everything_else_falls_through() {
        let cached = source_with(
            "www.example.com",
            vec![file("MP_verify_abc.txt", "verification-token-1")],
        );

        // 不同的文件名、不同主机、非根路径、目录路径:都不是平台的验证文件,
        // 一律放行给宿主应用。
        for (hostname, path) in [
            ("www.example.com", "/MP_verify_other.txt"),
            ("other.example.com", "/MP_verify_abc.txt"),
            ("www.example.com", "/assets/MP_verify_abc.txt"),
            ("www.example.com", "/MP_verify_abc.txt/"),
        ] {
            assert!(
                serve_verification_file(&cached, hostname, path, "GET")
                    .await
                    .is_none(),
                "{hostname}{path} must fall through"
            );
        }
        // 大小写敏感:平台的名字不做大小写归一。
        assert!(
            serve_verification_file(&cached, "www.example.com", "/mp_verify_abc.txt", "GET")
                .await
                .is_none()
        );
        // 不在支持扩展名之内的路径根本不进这个命名空间。
        assert!(requested_verification_file_name("/app.js").is_none());
    }

    #[tokio::test]
    async fn write_verbs_are_refused_only_where_a_file_exists() {
        let cached = source_with(
            "www.example.com",
            vec![file("MP_verify_abc.txt", "verification-token-1")],
        );

        let response =
            serve_verification_file(&cached, "www.example.com", "/MP_verify_abc.txt", "PUT")
                .await
                .expect("a write against a claimed file must be refused");
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        // 同样的写动词落在没有文件的主机上则放行——那是宿主应用的资源。
        assert!(
            serve_verification_file(&cached, "other.example.com", "/MP_verify_abc.txt", "PUT")
                .await
                .is_none()
        );
        // 落在同一主机但没有这个文件名的路径上也放行。
        assert!(
            serve_verification_file(&cached, "www.example.com", "/MP_verify_other.txt", "PUT")
                .await
                .is_none()
        );
    }

    /// 缓存有界:超过上限的不同主机名不会让地图单调增长。
    #[tokio::test]
    async fn the_cache_is_capped_under_host_diversity() {
        let source = Arc::new(MapSource::default());
        let cached = CachedVerificationFileSource::new(source.clone());
        // 超出上限一截的不同主机名全部走一次查询(各自未命中)。
        for index in 0..(MAX_CACHE_ENTRIES + 64) {
            let host = format!("host-{index}.example.com");
            assert!(cached.lookup(&host).await.is_none());
        }
        let size = cached.cache_len();
        assert!(
            size <= MAX_CACHE_ENTRIES,
            "cache grew to {size} entries; the cap must hold"
        );
        // 被驱逐的主机重新查询仍能正确回答(只是缓存未命中)。
        assert!(cached.lookup("host-0.example.com").await.is_none());
    }

    /// 命中只读一次源:重复请求由缓存应答。文件近乎不变,缓存把验证文件的
    /// 成本压到每主机每分钟一次查询。
    #[tokio::test]
    async fn repeated_hits_are_served_from_the_cache() {
        #[derive(Default)]
        struct CountingSource {
            files: Mutex<HashMap<String, Vec<VerificationFile>>>,
            reads: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl VerificationFileSource for CountingSource {
            async fn lookup(&self, hostname: &str) -> Option<Vec<VerificationFile>> {
                self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.files.lock().expect("lock").get(hostname).cloned()
            }
        }
        let source = Arc::new(CountingSource::default());
        source.files.lock().expect("lock").insert(
            "www.example.com".to_owned(),
            vec![file("MP_verify_abc.txt", "verification-token-1")],
        );
        let cached = CachedVerificationFileSource::new(source.clone());

        let first = cached.lookup("www.example.com").await.expect("hit");
        let second = cached.lookup("www.example.com").await.expect("hit");
        assert_eq!(first, second);
        assert_eq!(
            source.reads.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the second lookup must be served from the cache"
        );
    }
}
