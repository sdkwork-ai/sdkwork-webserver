//! 微信公众号域名验证文件的边缘服务。
//!
//! 微信公众平台要求在域名根路径可访问一个形如 `MP_verify_xxxxx.txt` 的校验
//! 文件后才能完成 JS 接口安全域名 / 网页授权域名 / 业务域名的归属验证。文件
//! 内容来自 Deploy 控制面（控制台上传，存于 `deploy_dns_zone`）；standalone
//! 部署形态下边缘与控制面共享进程与数据库，因此这里按主机名做进程内读取
//! ——与 `deploy_fallback` 的嵌入式查询同一信任级别，不需要新的分发通道。
//!
//! # 窄优先级，精确命中
//!
//! 只在"请求路径恰为单段 `.txt` 根路径、且该主机名对应的 zone 上报了同名
//! 文件"时接管：命中即按字节原样返回；任何不命中都放行给后续路由，宿主
//! 应用自己的 `/x.txt` 不受影响。GET/HEAD 之外的动词在确有文件时回答 405，
//! 没有文件时同样放行。
//!
//! # 进程级共享源
//!
//! 验证文件查询与 `process_shared_database_pool` 一样是进程级事实：数据面
//! 的每个监听器都读同一份，因此在网站数据面启动时装载一次，处理器经
//! [`shared_source`] 读取，而不是把查询穿过多层公开的启动签名。

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Response, StatusCode},
};

/// 单个文件名的长度上限，与控制面一致。
const MAX_VERIFICATION_FILE_NAME_BYTES: usize = 64;

/// 命中缓存的正向寿命。验证文件极少变化；替换后至多等这么久生效。
const POSITIVE_TTL: Duration = Duration::from_secs(60);

/// 未命中（zone 没有文件）的负向寿命，短得多：上传后希望尽快可用。
const NEGATIVE_TTL: Duration = Duration::from_secs(10);

/// 缓存条目硬上限。
///
/// 条目按请求 Host 键入，而 Host 在面向互联网的监听器上是攻击者可控的：
/// TTL 只在"同名再次被请求"时惰性生效，没有条目上限的地图会随出现的
/// 不同主机名单调增长。写入时顺带清扫过期条目并在达到上限时按到期时间
/// 驱逐最早的条目，保证地图有界——被驱逐的主机下一条请求只是重新查询。
const MAX_CACHE_ENTRIES: usize = 4096;

/// 一个 zone 上报的微信验证文件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WechatVerificationFile {
    pub file_name: String,
    pub content: String,
}

/// 主机名 → 验证文件 的读取源。
#[async_trait]
pub(crate) trait WechatVerificationSource: Send + Sync {
    /// 返回 `hostname` 所属 zone 的验证文件；该主机名没有可服务的文件时为
    /// `None`。查询失败同样以 `None` 作答——验证文件是窄优先级的附加能力，
    /// 数据库抖动不应把宿主应用自己的 404 变成边缘的 503。
    async fn lookup(&self, hostname: &str) -> Option<WechatVerificationFile>;
}

#[derive(Clone)]
struct CacheEntry {
    file: Option<WechatVerificationFile>,
    expires_at: Instant,
}

/// 带主机名缓存的读取源。
///
/// 正向寿命长（文件几乎不变），负向寿命短（上传后希望尽快可用）：两个 TTL
/// 的差就是"上传到微信爬虫能取到"的最坏等待，与操作员在控制台点自检的
/// 节奏一致。
pub(crate) struct CachedWechatVerificationSource {
    inner: Arc<dyn WechatVerificationSource>,
    cache: ArcSwap<HashMap<String, CacheEntry>>,
}

impl CachedWechatVerificationSource {
    pub(crate) fn new(inner: Arc<dyn WechatVerificationSource>) -> Self {
        Self {
            inner,
            cache: ArcSwap::from_pointee(HashMap::new()),
        }
    }
}

#[async_trait]
impl WechatVerificationSource for CachedWechatVerificationSource {
    async fn lookup(&self, hostname: &str) -> Option<WechatVerificationFile> {
        let now = Instant::now();
        if let Some(entry) = self.cache.load().get(hostname) {
            if entry.expires_at > now {
                return entry.file.clone();
            }
        }
        let file = self.inner.lookup(hostname).await;
        let ttl = if file.is_some() {
            POSITIVE_TTL
        } else {
            NEGATIVE_TTL
        };
        let mut cache = HashMap::clone(&self.cache.load());
        // 清扫过期条目：TTL 的惰性生效在写入时补一次全图清扫，地图不会
        // 留住无人再问的死条目。
        cache.retain(|_, entry| entry.expires_at > now);
        if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(hostname) {
            // 容量封顶：全部条目都未过期时，驱逐最早到期的条目。
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
                file: file.clone(),
                expires_at: now + ttl,
            },
        );
        self.cache.store(Arc::new(cache));
        file
    }
}

/// 生产读取源：共享 Deploy 数据库的进程内查询。
#[cfg(feature = "management")]
pub(crate) struct DeployWechatVerificationSource {
    repository: sdkwork_api_webserver_assembly::DeployRepository,
}

#[cfg(feature = "management")]
impl DeployWechatVerificationSource {
    pub(crate) fn new(repository: sdkwork_api_webserver_assembly::DeployRepository) -> Self {
        Self { repository }
    }
}

#[cfg(feature = "management")]
#[async_trait]
impl WechatVerificationSource for DeployWechatVerificationSource {
    async fn lookup(&self, hostname: &str) -> Option<WechatVerificationFile> {
        let (file_name, content) = self
            .repository
            .wechat_verification_by_hostname_lookup(hostname)
            .await
            .ok()??;
        Some(WechatVerificationFile { file_name, content })
    }
}

static SHARED_SOURCE: ArcSwapOption<CachedWechatVerificationSource> = ArcSwapOption::const_empty();

/// 装载进程级共享读取源（网站数据面启动时调用一次；重复装载以最后一次
/// 为准，与运行时生成的重启语义一致）。
#[cfg(feature = "management")]
pub(crate) fn install_shared_source(source: CachedWechatVerificationSource) {
    SHARED_SOURCE.store(Some(Arc::new(source)));
}

/// 当前共享读取源；未装载（非 management 构建，或共享数据库不可用）时为
/// `None`，处理器直接放行。
pub(crate) fn shared_source() -> Option<Arc<CachedWechatVerificationSource>> {
    SHARED_SOURCE.load_full()
}

/// 尝试按微信验证文件应答一个请求。
///
/// `hostname` 是请求 Host 里的裸主机名（已归一化），`path` 是归一化请求
/// 路径。只接管"单段 `.txt` 根路径 + 该主机名确有同名文件"的请求；其余
/// 一律返回 `None` 放行。
pub(crate) async fn serve_wechat_verification_file(
    source: &CachedWechatVerificationSource,
    hostname: &str,
    path: &str,
    method: &str,
) -> Option<Response<Body>> {
    let Some(file_name) = requested_verification_file_name(path) else {
        return None;
    };
    if !matches!(method, "GET" | "HEAD") {
        // 动词在命中的命名空间里仍然收口：验证文件是只读资源，写动词不
        // 应被转交给宿主应用，否则一个 PUT 就可能让微信抓到被改写的内容。
        return match source.lookup(hostname).await {
            Some(_) => Some(text_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "method is not allowed\n",
            )),
            None => None,
        };
    }
    let file = source.lookup(hostname).await?;
    // 文件名按精确比较：微信生成的名字大小写敏感，平台不做大小写归一。
    if file.file_name != file_name {
        return None;
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
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    // 校验窗口材料：绝不被中间缓存；HEAD 报告与 GET 相同的长度。
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

/// 请求路径恰为"单段 `.txt` 根路径"时返回该文件名，否则 `None`。
///
/// 校验比控制面上传更宽一点：微信历史上发放过带下划线与连字符的名字，
/// 语义上平台只关心"名字与 zone 上报的完全一致"。
fn requested_verification_file_name(path: &str) -> Option<&str> {
    let file_name = path.strip_prefix('/')?;
    if file_name.is_empty() || file_name.contains('/') {
        return None;
    }
    if !file_name.ends_with(".txt")
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
        files: Mutex<HashMap<String, WechatVerificationFile>>,
    }

    #[async_trait]
    impl WechatVerificationSource for MapSource {
        async fn lookup(&self, hostname: &str) -> Option<WechatVerificationFile> {
            self.files.lock().expect("lock").get(hostname).cloned()
        }
    }

    fn file(name: &str) -> WechatVerificationFile {
        WechatVerificationFile {
            file_name: name.to_owned(),
            content: "wechat-token-1".to_owned(),
        }
    }

    #[tokio::test]
    async fn an_exact_hit_is_served_verbatim() {
        let source = MapSource::default();
        source
            .files
            .lock()
            .expect("lock")
            .insert("www.example.com".to_owned(), file("MP_verify_abc.txt"));
        let cached = CachedWechatVerificationSource::new(Arc::new(source));

        let response =
            serve_wechat_verification_file(&cached, "www.example.com", "/MP_verify_abc.txt", "GET")
                .await
                .expect("an exact hit must be served");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn everything_else_falls_through() {
        let source = MapSource::default();
        source
            .files
            .lock()
            .expect("lock")
            .insert("www.example.com".to_owned(), file("MP_verify_abc.txt"));
        let cached = CachedWechatVerificationSource::new(Arc::new(source));

        // 不同的文件名、不同主机、非根路径、目录路径：都不是平台的验证文件，
        // 一律放行给宿主应用。
        for (hostname, path) in [
            ("www.example.com", "/MP_verify_other.txt"),
            ("other.example.com", "/MP_verify_abc.txt"),
            ("www.example.com", "/assets/MP_verify_abc.txt"),
            ("www.example.com", "/MP_verify_abc.txt/"),
        ] {
            assert!(
                serve_wechat_verification_file(&cached, hostname, path, "GET")
                    .await
                    .is_none(),
                "{hostname}{path} must fall through"
            );
        }
        // 大小写敏感：微信的名字不做大小写归一。
        assert!(serve_wechat_verification_file(
            &cached,
            "www.example.com",
            "/mp_verify_abc.txt",
            "GET"
        )
        .await
        .is_none());
    }

    #[tokio::test]
    async fn write_verbs_are_refused_only_where_a_file_exists() {
        let source = MapSource::default();
        source
            .files
            .lock()
            .expect("lock")
            .insert("www.example.com".to_owned(), file("MP_verify_abc.txt"));
        let cached = CachedWechatVerificationSource::new(Arc::new(source));

        let response =
            serve_wechat_verification_file(&cached, "www.example.com", "/MP_verify_abc.txt", "PUT")
                .await
                .expect("a write against a claimed file must be refused");
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        // 同样的写动词落在没有文件的主机上则放行——那是宿主应用的资源。
        assert!(serve_wechat_verification_file(
            &cached,
            "other.example.com",
            "/MP_verify_abc.txt",
            "PUT"
        )
        .await
        .is_none());
    }

    /// 缓存有界：超过上限的不同主机名不会让地图单调增长。
    #[tokio::test]
    async fn the_cache_is_capped_under_host_diversity() {
        let source = Arc::new(MapSource::default());
        let cached = CachedWechatVerificationSource::new(source.clone());
        // 超出上限一截的不同主机名全部走一次查询（各自未命中）。
        for index in 0..(MAX_CACHE_ENTRIES + 64) {
            let host = format!("host-{index}.example.com");
            assert!(cached.lookup(&host).await.is_none());
        }
        let size = cached.cache.load().len();
        assert!(
            size <= MAX_CACHE_ENTRIES,
            "cache grew to {size} entries; the cap must hold"
        );
        // 被驱逐的主机重新查询仍能正确回答（只是缓存未命中）。
        assert!(cached.lookup("host-0.example.com").await.is_none());
    }

    /// 命中只读一次源：重复请求由缓存应答。文件近乎不变，缓存把验证文件
    /// 的成本压到每主机每分钟一次查询。
    #[tokio::test]
    async fn repeated_hits_are_served_from_the_cache() {
        #[derive(Default)]
        struct CountingSource {
            files: Mutex<HashMap<String, WechatVerificationFile>>,
            reads: std::sync::atomic::AtomicUsize,
        }
        #[async_trait]
        impl WechatVerificationSource for CountingSource {
            async fn lookup(&self, hostname: &str) -> Option<WechatVerificationFile> {
                self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                self.files.lock().expect("lock").get(hostname).cloned()
            }
        }
        let source = Arc::new(CountingSource::default());
        source
            .files
            .lock()
            .expect("lock")
            .insert("www.example.com".to_owned(), file("MP_verify_abc.txt"));
        let cached = CachedWechatVerificationSource::new(source.clone());

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
