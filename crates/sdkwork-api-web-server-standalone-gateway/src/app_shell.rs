use std::{
    env,
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use axum::{
    body::Body,
    http::{
        header::{self, ACCEPT, CACHE_CONTROL},
        HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode,
    },
    routing::any,
    Router,
};
use sdkwork_web_bootstrap::{ReadinessCheck, ReadinessFuture};
use serde_json::Value;

use crate::data_plane::{
    static_file_response::serve_opened_file,
    static_path::{open_static_path, OpenedStaticFile, StaticPathError, StaticPathTarget},
};

pub(crate) const PC_STATIC_ROOT_ENV: &str = "SDKWORK_WEBSERVER_PC_STATIC_ROOT";

const WEB_DEPLOYMENT_PROFILE_ENV: &str = "SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE";
const DEPLOYMENT_PROFILE_ENV: &str = "SDKWORK_DEPLOYMENT_PROFILE";
const CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN_ENV: &str =
    "SDKWORK_WEBSERVER_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN";
const INDEX_FILE: &str = "index.html";
const RUNTIME_ENV_FILE: &str = "runtime-env.json";
const RUNTIME_ENV_PATH: &str = "/runtime-env.json";
const MAX_BOOTSTRAP_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_STATIC_FILES: usize = 2048;
const X_CONTENT_TYPE_OPTIONS: HeaderName = HeaderName::from_static("x-content-type-options");
const RESERVED_PATHS: [&str; 7] = [
    "/app",
    "/backend",
    "/internal",
    "/openapi.json",
    "/healthz",
    "/readyz",
    "/livez",
];
const SDK_BASE_URL_FIELDS: [&str; 4] = [
    "appApiBaseUrl",
    "backendApiBaseUrl",
    "driveAppApiBaseUrl",
    "appbaseAppApiBaseUrl",
];

/// Validates the configured standalone PC shell without starting API dependencies or a listener.
pub fn validate_pc_app_shell_from_env() -> Result<(), String> {
    PcAppShellConfig::from_env()?
        .map(|_| ())
        .ok_or_else(|| format!("{PC_STATIC_ROOT_ENV} is not configured for this runtime"))
}

#[derive(Clone, Debug)]
pub(crate) struct PcAppShellConfig {
    root: PathBuf,
    environment: String,
    required_files: Arc<Vec<PathBuf>>,
    /// Credential-entry bootstrap Access-Token injected into the served
    /// `index.html` (materialized from the TOML `[secrets]`
    /// `credential_entry_bootstrap_access_token_file` reference). The login
    /// renderer sends it so the identity-service metadata endpoints accept
    /// the anonymous credential-entry bootstrap.
    bootstrap_access_token: Option<String>,
}

impl PcAppShellConfig {
    pub(crate) fn from_env() -> Result<Option<Self>, String> {
        let deployment_profile = env::var(WEB_DEPLOYMENT_PROFILE_ENV)
            .or_else(|_| env::var(DEPLOYMENT_PROFILE_ENV))
            .unwrap_or_else(|_| "standalone".to_owned())
            .trim()
            .to_ascii_lowercase();
        let environment = sdkwork_webserver_contract::web_environment_name();
        Self::resolve(
            &deployment_profile,
            &environment,
            env::var_os(PC_STATIC_ROOT_ENV),
        )
    }

    fn resolve(
        deployment_profile: &str,
        environment: &str,
        configured_root: Option<OsString>,
    ) -> Result<Option<Self>, String> {
        if configured_root
            .as_ref()
            .is_some_and(|value| value.is_empty())
        {
            return Err(format!("{PC_STATIC_ROOT_ENV} must not be empty"));
        }
        if deployment_profile != "standalone" {
            return match configured_root {
                Some(_) => Err(format!(
                    "{PC_STATIC_ROOT_ENV} is supported only for the standalone deployment profile"
                )),
                None => Ok(None),
            };
        }
        let Some(root) = configured_root else {
            if matches!(environment, "production" | "prod") {
                return Err(format!(
                    "{PC_STATIC_ROOT_ENV} is required for standalone production"
                ));
            }
            return Ok(None);
        };

        let root = resolve_static_root(PathBuf::from(root))?;
        Self::preflight(root, environment).map(Some)
    }

    fn preflight(root: PathBuf, environment: &str) -> Result<Self, String> {
        let environment = canonical_environment(environment)?.to_owned();
        let required_files = collect_static_files(&root)?;
        validate_bootstrap_files(&root, &environment)?;
        let bootstrap_access_token = env::var(CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN_ENV)
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty());

        Ok(Self {
            root,
            environment,
            required_files: Arc::new(required_files),
            bootstrap_access_token,
        })
    }

    pub(crate) fn readiness_check(&self) -> Arc<dyn ReadinessCheck> {
        Arc::new(PcAppShellReadinessCheck {
            root: self.root.clone(),
            environment: self.environment.clone(),
            required_files: Arc::clone(&self.required_files),
        })
    }

    pub(crate) fn mount(self, router: Router) -> Router {
        let config = Arc::new(self);
        Router::new()
            .fallback(any(move |request: Request<Body>| {
                let config = Arc::clone(&config);
                async move { serve_request(config, request).await }
            }))
            .merge(router)
    }
}

#[derive(Clone, Debug)]
struct PcAppShellReadinessCheck {
    root: PathBuf,
    environment: String,
    required_files: Arc<Vec<PathBuf>>,
}

impl ReadinessCheck for PcAppShellReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let root = self.root.clone();
        let environment = self.environment.clone();
        let required_files = Arc::clone(&self.required_files);
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let current_files = collect_static_files(&root)?;
                if current_files != *required_files {
                    return Err(format!(
                        "{PC_STATIC_ROOT_ENV} static file inventory changed after startup"
                    ));
                }
                validate_bootstrap_files(&root, &environment)
            })
            .await
            .map_err(|error| format!("PC app shell readiness task failed: {error}"))?
        })
    }
}

fn resolve_static_root(configured: PathBuf) -> Result<PathBuf, String> {
    if configured.is_absolute() {
        return Ok(configured);
    }
    if configured
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "relative {PC_STATIC_ROOT_ENV} must not contain parent-directory components"
        ));
    }
    if matches!(configured.components().next(), Some(Component::CurDir)) {
        let current_directory = env::current_dir().map_err(|error| {
            format!("could not resolve current directory for {PC_STATIC_ROOT_ENV}: {error}")
        })?;
        return Ok(current_directory.join(configured));
    }

    let executable = env::current_exe().map_err(|error| {
        format!("could not resolve current executable for {PC_STATIC_ROOT_ENV}: {error}")
    })?;
    resolve_packaged_static_root(&configured, &executable)
}

fn resolve_packaged_static_root(configured: &Path, executable: &Path) -> Result<PathBuf, String> {
    let binary_directory = executable.parent().ok_or_else(|| {
        format!("current executable has no parent for relative {PC_STATIC_ROOT_ENV}")
    })?;
    if binary_directory
        .file_name()
        .and_then(|value| value.to_str())
        != Some("bin")
    {
        return Err(format!(
            "relative {PC_STATIC_ROOT_ENV} paths resolve from a packaged bin/ directory; use an explicit ./ path for source-tree execution"
        ));
    }
    let package_root = binary_directory.parent().ok_or_else(|| {
        format!("packaged bin directory has no package root for {PC_STATIC_ROOT_ENV}")
    })?;
    Ok(package_root.join(configured))
}

fn collect_static_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let root_metadata = fs::symlink_metadata(root).map_err(|error| {
        format!(
            "{PC_STATIC_ROOT_ENV} {} is not available: {error}",
            root.display()
        )
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(format!(
            "{PC_STATIC_ROOT_ENV} {} must be a non-symlink directory",
            root.display()
        ));
    }

    let mut files = Vec::new();
    collect_static_files_from(root, root, &mut files)?;
    files.sort();
    if files.len() > MAX_STATIC_FILES {
        return Err(format!(
            "{PC_STATIC_ROOT_ENV} contains more than {MAX_STATIC_FILES} files"
        ));
    }
    Ok(files)
}

fn collect_static_files_from(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("could not list {PC_STATIC_ROOT_ENV}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not list {PC_STATIC_ROOT_ENV}: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("could not inspect {PC_STATIC_ROOT_ENV}: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "{PC_STATIC_ROOT_ENV} entry {} must not be a symbolic link",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_static_files_from(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| format!("{PC_STATIC_ROOT_ENV} entry escaped its configured root"))?;
            files.push(relative.to_owned());
            if files.len() > MAX_STATIC_FILES {
                return Err(format!(
                    "{PC_STATIC_ROOT_ENV} contains more than {MAX_STATIC_FILES} files"
                ));
            }
        } else {
            return Err(format!(
                "{PC_STATIC_ROOT_ENV} entry {} must be a regular file or directory",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_bootstrap_files(root: &Path, environment: &str) -> Result<(), String> {
    let index = read_bootstrap_file(root, INDEX_FILE)?;
    let index = std::str::from_utf8(&index)
        .map_err(|_| format!("{INDEX_FILE} in {PC_STATIC_ROOT_ENV} must be valid UTF-8"))?;
    let lower_index = index.to_ascii_lowercase();
    if index.trim().is_empty()
        || (!lower_index.contains("<!doctype html") && !lower_index.contains("<html"))
    {
        return Err(format!(
            "{INDEX_FILE} in {PC_STATIC_ROOT_ENV} must contain an HTML document"
        ));
    }

    let runtime_env = read_bootstrap_file(root, RUNTIME_ENV_FILE)?;
    let runtime_env: Value = serde_json::from_slice(&runtime_env).map_err(|error| {
        format!("{RUNTIME_ENV_FILE} in {PC_STATIC_ROOT_ENV} must be valid JSON: {error}")
    })?;
    validate_runtime_env(&runtime_env, environment)
}

fn read_bootstrap_file(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        format!(
            "required {relative} is not available in {PC_STATIC_ROOT_ENV} {}: {error}",
            root.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "required {relative} in {PC_STATIC_ROOT_ENV} must be a non-symlink regular file"
        ));
    }
    if metadata.len() == 0 || metadata.len() > MAX_BOOTSTRAP_FILE_BYTES {
        return Err(format!(
            "required {relative} in {PC_STATIC_ROOT_ENV} must contain 1..={MAX_BOOTSTRAP_FILE_BYTES} bytes"
        ));
    }
    let bytes = fs::read(&path).map_err(|error| {
        format!("required {relative} could not be read from {PC_STATIC_ROOT_ENV}: {error}")
    })?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_BOOTSTRAP_FILE_BYTES {
        return Err(format!(
            "required {relative} in {PC_STATIC_ROOT_ENV} changed during validation"
        ));
    }
    Ok(bytes)
}

fn canonical_environment(environment: &str) -> Result<&str, String> {
    match environment {
        "development" | "dev" => Ok("development"),
        "test" => Ok("test"),
        "staging" | "stage" => Ok("staging"),
        "production" | "prod" => Ok("production"),
        other => Err(format!(
            "unsupported SDKWORK_WEBSERVER_ENVIRONMENT {other} for standalone static delivery"
        )),
    }
}

fn validate_runtime_env(value: &Value, environment: &str) -> Result<(), String> {
    let object = value.as_object().ok_or_else(|| {
        format!("{RUNTIME_ENV_FILE} in {PC_STATIC_ROOT_ENV} must contain a JSON object")
    })?;
    let profile_id = format!("standalone.{environment}");
    for (field, expected) in [
        ("environment", environment),
        ("deploymentProfile", "standalone"),
        ("profileId", profile_id.as_str()),
        ("runtimeTarget", "browser"),
        ("browserOriginMode", "same-origin"),
    ] {
        if object.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "{RUNTIME_ENV_FILE}.{field} must equal {expected} for standalone static delivery"
            ));
        }
    }
    for field in SDK_BASE_URL_FIELDS {
        if object.get(field).and_then(Value::as_str) != Some("/") {
            return Err(format!(
                "{RUNTIME_ENV_FILE}.{field} must use the canonical same-origin root /"
            ));
        }
    }
    Ok(())
}

async fn serve_request(config: Arc<PcAppShellConfig>, request: Request<Body>) -> Response<Body> {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    if !matches!(method, Method::GET | Method::HEAD) || is_reserved_path(&path) {
        return not_found();
    }

    let relative = path.trim_start_matches('/');
    let html_navigation = is_html_navigation(request.headers(), &path);
    let fallback = html_navigation.then_some(INDEX_FILE);
    let target = match open_static_path(&config.root, relative, path.ends_with('/'), fallback).await
    {
        Ok(target) => target,
        Err(error) => return path_error_response(error),
    };

    match target {
        StaticPathTarget::File(file) => {
            if let Some(token) = config.bootstrap_access_token.as_deref().filter(|_| {
                file.path_hint
                    .file_name()
                    .is_some_and(|name| name == INDEX_FILE)
            }) {
                return serve_index_with_bootstrap_token(&config.root, file, token, &method);
            }
            let mut response = serve_opened_file(file, &method, request.headers()).await;
            response
                .headers_mut()
                .insert(CACHE_CONTROL, cache_policy(&path));
            response
                .headers_mut()
                .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            response
        }
        StaticPathTarget::RedirectToDirectory => redirect_to_directory(request.uri()),
    }
}

/// Serves `index.html` with the credential-entry bootstrap Access-Token
/// injected as an inline script so the identity-service metadata endpoints
/// accept the anonymous login renderer (`@sdkwork/iam-credential-entry`).
/// The token is a compact JWT without HTML-significant characters; the
/// inline script is written before `</head>`. The static path hints are
/// root-relative, so the file is re-read from the configured root instead of
/// the process working directory.
fn serve_index_with_bootstrap_token(
    root: &Path,
    file: OpenedStaticFile,
    token: &str,
    method: &Method,
) -> Response<Body> {
    let html = match fs::read(root.join(&file.path_hint)) {
        Ok(html) => html,
        Err(_) => return empty_response(StatusCode::NOT_FOUND),
    };
    if method == Method::HEAD {
        return empty_response(StatusCode::OK);
    }
    let injected = format!(
        "<script>globalThis.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__=\"{}\";</script>",
        token
    );
    let html = match std::str::from_utf8(&html) {
        Ok(html) if html.contains("</head>") => html
            .replacen("</head>", &format!("{injected}</head>"), 1)
            .into_bytes(),
        _ => html_into_bytes_with_injected_tail(&html, injected.as_bytes()),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(CACHE_CONTROL, "public, no-cache")
        .header(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"))
        .body(Body::from(html))
        .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR))
}

fn html_into_bytes_with_injected_tail(html: &[u8], injected: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(html.len() + injected.len() + 8);
    output.extend_from_slice(html);
    output.extend_from_slice(injected);
    output
}

fn is_reserved_path(path: &str) -> bool {
    if path == "/metrics" || path.starts_with("/metrics/") {
        return true;
    }
    RESERVED_PATHS.iter().any(|reserved| {
        path.strip_prefix(*reserved)
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
    })
}

fn is_html_navigation(headers: &HeaderMap, path: &str) -> bool {
    if path == "/assets"
        || path.starts_with("/assets/")
        || path == RUNTIME_ENV_PATH
        || path.starts_with("/runtime-env.json/")
        || path.contains('%')
        || path.contains('\\')
        || has_file_extension(path)
    {
        return false;
    }
    headers.get_all(ACCEPT).iter().any(|value| {
        value
            .to_str()
            .ok()
            .is_some_and(|value| value.to_ascii_lowercase().contains("text/html"))
    })
}

fn has_file_extension(path: &str) -> bool {
    let final_segment = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    Path::new(final_segment).extension().is_some()
}

fn cache_policy(path: &str) -> HeaderValue {
    if path == RUNTIME_ENV_PATH {
        HeaderValue::from_static("no-store")
    } else if is_fingerprinted_asset(path) {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        HeaderValue::from_static("no-cache")
    }
}

fn is_fingerprinted_asset(path: &str) -> bool {
    let Some(file_name) = path
        .strip_prefix("/assets/")
        .and_then(|value| value.rsplit('/').next())
    else {
        return false;
    };
    let stem = file_name.split('.').next().unwrap_or_default();
    let hash = stem
        .rsplit_once('-')
        .map(|(_, hash)| hash)
        .unwrap_or_default();
    (8..=64).contains(&hash.len())
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn path_error_response(error: StaticPathError) -> Response<Body> {
    let status = match error {
        StaticPathError::Invalid => StatusCode::BAD_REQUEST,
        StaticPathError::Forbidden => StatusCode::FORBIDDEN,
        StaticPathError::NotFound => StatusCode::NOT_FOUND,
        StaticPathError::Io => StatusCode::INTERNAL_SERVER_ERROR,
    };
    empty_response(status)
}

fn redirect_to_directory(uri: &axum::http::Uri) -> Response<Body> {
    let Some(path_and_query) = uri.path_and_query() else {
        return empty_response(StatusCode::BAD_REQUEST);
    };
    let location = match path_and_query.query() {
        Some(query) => format!("{}/?{query}", path_and_query.path()),
        None => format!("{}/", path_and_query.path()),
    };
    let Ok(location) = HeaderValue::from_str(&location) else {
        return empty_response(StatusCode::BAD_REQUEST);
    };
    Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(axum::http::header::LOCATION, location)
        .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Body::empty())
        .unwrap_or_else(|_| empty_response(StatusCode::INTERNAL_SERVER_ERROR))
}

fn not_found() -> Response<Body> {
    empty_response(StatusCode::NOT_FOUND)
}

fn empty_response(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Body::empty())
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::to_bytes,
        http::{header::CONTENT_TYPE, Request},
        middleware::{self, Next},
        response::IntoResponse,
        routing::get,
    };
    use tempfile::TempDir;
    use tower::ServiceExt;

    use sdkwork_web_bootstrap::{
        service_router, AlwaysReady, CompositeReadinessCheck, ServiceRouterConfig,
    };

    use super::*;

    const RUNTIME_ENV: &str = r#"{
  "environment": "development",
  "deploymentProfile": "standalone",
  "profileId": "standalone.development",
  "runtimeTarget": "browser",
  "browserOriginMode": "same-origin",
  "appApiBaseUrl": "/",
  "backendApiBaseUrl": "/",
  "driveAppApiBaseUrl": "/",
  "appbaseAppApiBaseUrl": "/"
}
"#;

    #[test]
    fn production_requires_a_preflighted_static_root() {
        let missing = PcAppShellConfig::resolve("standalone", "production", None)
            .expect_err("production must require static root");
        assert!(missing.contains(PC_STATIC_ROOT_ENV));

        let temp = TempDir::new().unwrap();
        let invalid = PcAppShellConfig::resolve(
            "standalone",
            "production",
            Some(temp.path().as_os_str().to_owned()),
        )
        .expect_err("index and runtime env are required");
        assert!(invalid.contains(INDEX_FILE));
    }

    #[test]
    fn development_without_static_root_keeps_api_only_router() {
        assert!(PcAppShellConfig::resolve("standalone", "development", None)
            .unwrap()
            .is_none());
        assert!(PcAppShellConfig::resolve("cloud", "production", None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn relative_roots_are_package_relative_unless_dot_prefixed() {
        let package = PathBuf::from("install/sdkwork-web");
        let executable = package.join("bin/gateway");
        assert_eq!(
            resolve_packaged_static_root(Path::new("share/sdkwork/webserver-pc"), &executable)
                .unwrap(),
            package.join("share/sdkwork/webserver-pc")
        );

        let current_directory = env::current_dir().unwrap();
        assert_eq!(
            resolve_static_root(PathBuf::from("./apps/sdkwork-webserver-pc/dist")).unwrap(),
            current_directory.join("./apps/sdkwork-webserver-pc/dist")
        );
        assert!(resolve_packaged_static_root(
            Path::new("share/sdkwork/webserver-pc"),
            Path::new("target/debug/gateway")
        )
        .unwrap_err()
        .contains("explicit ./ path"));
        assert!(resolve_static_root(PathBuf::from("../outside"))
            .unwrap_err()
            .contains("parent-directory"));
    }

    #[test]
    fn preflight_rejects_cross_origin_runtime_configuration() {
        let temp = fixture_root();
        fs::write(
            temp.path().join(RUNTIME_ENV_FILE),
            RUNTIME_ENV.replace(
                "\"appApiBaseUrl\": \"/\"",
                "\"appApiBaseUrl\": \"http://127.0.0.1:3900\"",
            ),
        )
        .unwrap();
        let error = PcAppShellConfig::preflight(temp.path().to_owned(), "development")
            .expect_err("cross-origin runtime env must fail closed");
        assert!(error.contains("appApiBaseUrl"));
    }

    #[tokio::test]
    async fn serves_shell_runtime_and_immutable_assets_with_explicit_route_precedence() {
        let temp = fixture_root();
        let config = PcAppShellConfig::preflight(temp.path().to_owned(), "development").unwrap();
        let router = config.mount(
            Router::new()
                .route("/app/v3/api/existing", get(|| async { "api-response" }))
                .route("/healthz", get(|| async { "healthy" })),
        );

        let api = request(&router, Method::GET, "/app/v3/api/existing", None).await;
        assert_eq!(api.status(), StatusCode::OK);
        assert_eq!(body(api).await, "api-response");

        let index = request(&router, Method::GET, "/", Some("text/html")).await;
        assert_eq!(index.status(), StatusCode::OK);
        assert_eq!(index.headers()[CACHE_CONTROL], "no-cache");
        assert!(body(index).await.contains("<html"));

        let navigation = request(&router, Method::GET, "/console/sites", Some("text/html")).await;
        assert_eq!(navigation.status(), StatusCode::OK);
        assert_eq!(navigation.headers()[CACHE_CONTROL], "no-cache");
        assert!(body(navigation).await.contains("<html"));

        let runtime = request(&router, Method::GET, "/runtime-env.json", None).await;
        assert_eq!(runtime.status(), StatusCode::OK);
        assert_eq!(runtime.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(runtime.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");

        let asset = request(&router, Method::GET, "/assets/index-AbCd1234.js", None).await;
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(
            asset.headers()[CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
    }

    #[tokio::test]
    async fn index_injects_credential_entry_bootstrap_access_token_when_configured() {
        let temp = fixture_root();
        let mut config =
            PcAppShellConfig::preflight(temp.path().to_owned(), "development").unwrap();
        config.bootstrap_access_token = Some("header.payload.signature".to_owned());
        let router = config.mount(Router::new());

        let index = request(&router, Method::GET, "/", Some("text/html")).await;
        assert_eq!(index.status(), StatusCode::OK);
        let html = body(index).await;
        assert!(html.contains(
            "globalThis.__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__=\"header.payload.signature\""
        ));

        let navigation = request(&router, Method::GET, "/console/sites", Some("text/html")).await;
        assert_eq!(navigation.status(), StatusCode::OK);
        assert!(body(navigation).await.contains("BOOTSTRAP_ACCESS_TOKEN"));
    }

    #[tokio::test]
    async fn index_without_bootstrap_token_is_served_unchanged() {
        let temp = fixture_root();
        let config = PcAppShellConfig::preflight(temp.path().to_owned(), "development").unwrap();
        let router = config.mount(Router::new());

        let index = request(&router, Method::GET, "/", Some("text/html")).await;
        assert_eq!(index.status(), StatusCode::OK);
        assert!(!body(index).await.contains("BOOTSTRAP_ACCESS_TOKEN"));
    }

    #[tokio::test]
    async fn shell_mount_stays_outside_api_authentication_layers() {
        let temp = fixture_root();
        let config = PcAppShellConfig::preflight(temp.path().to_owned(), "development").unwrap();
        let protected = Router::new()
            .route("/app/v3/api/existing", get(|| async { "api-response" }))
            .layer(middleware::from_fn(reject_request));
        let router = config.mount(protected);

        let index = request(&router, Method::GET, "/", Some("text/html")).await;
        assert_eq!(index.status(), StatusCode::OK);
        assert!(body(index).await.contains("<html"));

        let api = request(&router, Method::GET, "/app/v3/api/existing", None).await;
        assert_eq!(api.status(), StatusCode::UNAUTHORIZED);

        let missing = request(
            &router,
            Method::GET,
            "/app/v3/api/does-not-exist",
            Some("text/html"),
        )
        .await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert_ne!(
            missing.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("text/html"))
        );
    }

    #[tokio::test]
    async fn never_rewrites_api_infrastructure_assets_or_non_navigation_requests() {
        let temp = fixture_root();
        let config = PcAppShellConfig::preflight(temp.path().to_owned(), "development").unwrap();
        let router = config.mount(Router::new());
        for path in [
            "/app/v3/api/missing",
            "/backend/v3/api/missing",
            "/internal/v3/api/missing",
            "/app%2Fv3%2Fapi%2Fmissing",
            "/openapi.json",
            "/healthz",
            "/readyz",
            "/livez",
            "/metrics",
            "/runtime-env.json/child",
            "/assets/missing-AbCd1234.js",
            "/favicon.ico",
        ] {
            let response = request(&router, Method::GET, path, Some("text/html")).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "path={path}");
            assert_ne!(
                response.headers().get(CONTENT_TYPE),
                Some(&HeaderValue::from_static("text/html")),
                "path={path}"
            );
        }

        let non_html = request(
            &router,
            Method::GET,
            "/console/sites",
            Some("application/json"),
        )
        .await;
        assert_eq!(non_html.status(), StatusCode::NOT_FOUND);
        let post = request(&router, Method::POST, "/console/sites", Some("text/html")).await;
        assert_eq!(post.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn readiness_fails_when_a_preflighted_shell_asset_disappears() {
        let temp = fixture_root_for("production");
        let config = PcAppShellConfig::preflight(temp.path().to_owned(), "production").unwrap();
        let readiness = Arc::new(CompositeReadinessCheck::new(vec![
            Arc::new(AlwaysReady),
            config.readiness_check(),
        ]));
        let router = config.mount(service_router(
            Router::new(),
            ServiceRouterConfig::default().with_readiness_check(readiness),
        ));

        let ready = request(&router, Method::GET, "/readyz", None).await;
        assert_eq!(ready.status(), StatusCode::OK);
        fs::remove_file(temp.path().join("assets/index-AbCd1234.js")).unwrap();
        let not_ready = request(&router, Method::GET, "/readyz", None).await;
        assert_eq!(not_ready.status(), StatusCode::SERVICE_UNAVAILABLE);
        let live = request(&router, Method::GET, "/healthz", None).await;
        assert_eq!(live.status(), StatusCode::OK);
    }

    fn fixture_root() -> TempDir {
        fixture_root_for("development")
    }

    fn fixture_root_for(environment: &str) -> TempDir {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join(INDEX_FILE),
            "<!doctype html><html><body>shell</body></html>",
        )
        .unwrap();
        let runtime_env = RUNTIME_ENV
            .replace(
                "\"environment\": \"development\"",
                &format!("\"environment\": \"{environment}\""),
            )
            .replace(
                "\"profileId\": \"standalone.development\"",
                &format!("\"profileId\": \"standalone.{environment}\""),
            );
        fs::write(temp.path().join(RUNTIME_ENV_FILE), runtime_env).unwrap();
        fs::create_dir(temp.path().join("assets")).unwrap();
        fs::write(
            temp.path().join("assets/index-AbCd1234.js"),
            "console.log('shell');",
        )
        .unwrap();
        temp
    }

    async fn request(
        router: &Router,
        method: Method,
        uri: &str,
        accept: Option<&str>,
    ) -> Response<Body> {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(accept) = accept {
            request = request.header(ACCEPT, accept);
        }
        router
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body(response: Response<Body>) -> String {
        String::from_utf8(
            to_bytes(response.into_body(), MAX_BOOTSTRAP_FILE_BYTES as usize)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    async fn reject_request(_request: Request<Body>, _next: Next) -> Response<Body> {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
