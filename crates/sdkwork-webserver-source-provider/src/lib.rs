use async_trait::async_trait;
use sdkwork_drive_config::DatabaseConfig;
use sdkwork_drive_object_runtime::DriveObjectStoreRuntime;
use sdkwork_drive_uploader_service::service::{
    DriveUploaderService, PrepareUploaderUploadCommand, SqlUploaderStore, UploadBytesCommand,
    UploaderActor, UploaderRetention, UploaderTarget,
};
use sdkwork_drive_workspace_service::infrastructure::sql::connect_postgres_database_and_install_schema;
use sdkwork_intelligence_webserver_service::{
    ApplicationSourceImporter, GitSourceImportRequest, ImportedApplicationSource,
};
use sdkwork_webserver_contract::{SourceVersionConfigSnapshot, WebServiceError, WebServiceResult};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::io::{Cursor, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::lookup_host;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::timeout;
use url::{Host, Url};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const MAX_FILES: usize = 500;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 512;
const MAX_PATH_DEPTH: usize = 32;
const GIT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_GIT_IMPORT_CONCURRENCY: usize = 2;
const MAXIMUM_GIT_IMPORT_CONCURRENCY: usize = 16;
const GIT_IMPORT_CONCURRENCY_ENV: &str = "SDKWORK_WEBSERVER_GIT_IMPORT_CONCURRENCY";

pub struct GitDriveSourceImporter {
    pool: PgPool,
    uploader: DriveUploaderService<SqlUploaderStore>,
    object_runtime: DriveObjectStoreRuntime,
    import_permits: Arc<Semaphore>,
}

impl GitDriveSourceImporter {
    pub async fn from_env() -> Result<Self, String> {
        let config = DatabaseConfig::from_env()
            .map_err(|error| format!("resolve Drive database config failed: {error}"))?;
        let pool = connect_postgres_database_and_install_schema(&config)
            .await
            .map_err(|error| format!("initialize Drive source importer failed: {error}"))?;
        let concurrency = std::env::var(GIT_IMPORT_CONCURRENCY_ENV)
            .ok()
            .map(|value| {
                value.parse::<usize>().map_err(|_| {
                    format!("{GIT_IMPORT_CONCURRENCY_ENV} must be an integer between 1 and {MAXIMUM_GIT_IMPORT_CONCURRENCY}")
                })
            })
            .transpose()?
            .unwrap_or(DEFAULT_GIT_IMPORT_CONCURRENCY);
        if !(1..=MAXIMUM_GIT_IMPORT_CONCURRENCY).contains(&concurrency) {
            return Err(format!(
                "{GIT_IMPORT_CONCURRENCY_ENV} must be an integer between 1 and {MAXIMUM_GIT_IMPORT_CONCURRENCY}"
            ));
        }
        Ok(Self {
            uploader: DriveUploaderService::new(SqlUploaderStore::new(pool.clone())),
            object_runtime: DriveObjectStoreRuntime::new(pool.clone()),
            pool,
            import_permits: Arc::new(Semaphore::new(concurrency)),
        })
    }

    async fn upload_archive(
        &self,
        request: &GitSourceImportRequest,
        archive: Vec<u8>,
        archive_hash: String,
    ) -> WebServiceResult<String> {
        let now = chrono::Utc::now().timestamp_millis();
        let upload_id = format!("git-{now}-{}", &archive_hash[..16]);
        let operator_id = request
            .actor_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "sdkwork-webserver".to_string());
        let actor = request.actor_id.map_or_else(
            || UploaderActor::System {
                operator_id: operator_id.clone(),
            },
            |user_id| UploaderActor::User {
                user_id: user_id.to_string(),
            },
        );
        let prepare = PrepareUploaderUploadCommand {
            id: upload_id.clone(),
            task_id: format!("web-git-{archive_hash}"),
            tenant_id: request.tenant_id.to_string(),
            organization_id: request.organization_id.map(|value| value.to_string()),
            actor,
            app_id: "sdkwork-web".to_string(),
            app_resource_type: "web.application.source".to_string(),
            app_resource_id: request.application_id.clone(),
            scene: Some("git-repository".to_string()),
            source: Some("sdkwork-webserver-source-provider".to_string()),
            upload_profile_code: "archive".to_string(),
            file_fingerprint: archive_hash,
            original_file_name: source_archive_name(&request.version_tag),
            content_type: "application/zip".to_string(),
            content_length: archive.len() as i64,
            chunk_size_bytes: 8 * 1024 * 1024,
            target: UploaderTarget::AutoUploadSpace {
                parent_node_id: None,
            },
            retention: UploaderRetention::LongTerm,
            operator_id,
            now_epoch_ms: now,
        };
        let prepared = self
            .uploader
            .prepare_upload(prepare.clone())
            .await
            .map_err(map_drive_error)?;
        let provider_id = prepared.storage_provider_id.as_deref().ok_or_else(|| {
            WebServiceError::Internal("Drive upload is missing a storage provider".to_string())
        })?;
        let provider_version: i64 = sqlx::query_scalar(
            "SELECT version FROM dr_drive_storage_provider WHERE id = $1 AND status = 'active'",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            WebServiceError::Internal(format!("resolve Drive storage provider failed: {error}"))
        })?
        .ok_or_else(|| {
            WebServiceError::Internal("Drive storage provider is unavailable".to_string())
        })?;
        let object_store = self
            .object_runtime
            .resolve(provider_id, provider_version)
            .await
            .map_err(|error| {
                WebServiceError::Internal(format!("resolve Drive object store failed: {error}"))
            })?;
        let completed = self
            .uploader
            .upload_bytes(
                object_store.as_ref(),
                UploadBytesCommand {
                    prepare,
                    body: archive,
                    uploaded_at_epoch_ms: now,
                },
            )
            .await
            .map_err(map_drive_error)?;
        Ok(format!(
            "drive://spaces/{}/nodes/{}",
            completed.space_id, completed.node_id
        ))
    }
}

#[async_trait]
impl ApplicationSourceImporter for GitDriveSourceImporter {
    async fn import_git(
        &self,
        request: &GitSourceImportRequest,
    ) -> WebServiceResult<ImportedApplicationSource> {
        let _permit = self
            .import_permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                WebServiceError::conflict("Git source import capacity is exhausted; retry later")
            })?;
        let target = validate_repository_target(&request.repository_url).await?;
        let checkout = TempDir::new().map_err(internal_io("create Git import workspace"))?;
        let repository_root = checkout.path().join("repository");
        clone_repository(request, &target, &repository_root).await?;
        let commit_hash = resolve_commit_hash(&repository_root).await?;
        let package = tokio::task::spawn_blocking({
            let repository_root = repository_root.clone();
            move || package_repository(&repository_root)
        })
        .await
        .map_err(|error| {
            WebServiceError::Internal(format!("Git packaging task failed: {error}"))
        })??;
        let artifact_hash = sha256_hex(&package.archive);
        let artifact_size = package.archive.len() as i64;
        let artifact_drive_uri = self
            .upload_archive(request, package.archive, artifact_hash.clone())
            .await?;
        Ok(ImportedApplicationSource {
            artifact_drive_uri,
            artifact_size,
            artifact_hash,
            commit_hash,
            config_snapshot: package.config_snapshot,
        })
    }
}

struct PackagedRepository {
    archive: Vec<u8>,
    config_snapshot: SourceVersionConfigSnapshot,
}

#[derive(Debug)]
struct ValidatedRepositoryTarget {
    url: Url,
    host_name: String,
    resolved_addresses: Vec<IpAddr>,
}

async fn validate_repository_target(
    repository_url: &str,
) -> WebServiceResult<ValidatedRepositoryTarget> {
    let url = Url::parse(repository_url)
        .map_err(|_| WebServiceError::validation("repositoryUrl must be an absolute HTTPS URL"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(WebServiceError::validation(
            "repositoryUrl must use HTTPS port 443 without credentials, query, or fragment",
        ));
    }
    let host = url
        .host()
        .ok_or_else(|| WebServiceError::validation("repositoryUrl must include a public host"))?;
    let host_name = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let allowed_hosts = configured_allowed_hosts().ok_or_else(|| {
        WebServiceError::validation(
            "Git source import is disabled until SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS is configured",
        )
    })?;
    if !allowed_hosts.iter().any(|allowed| allowed == &host_name) {
        return Err(WebServiceError::validation(
            "repositoryUrl host is not allowed by SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS",
        ));
    }
    let resolved_addresses = match host {
        Host::Ipv4(address) if is_forbidden_ip(IpAddr::V4(address)) => {
            return Err(WebServiceError::validation(
                "repositoryUrl must resolve to a public address",
            ));
        }
        Host::Ipv6(address) if is_forbidden_ip(IpAddr::V6(address)) => {
            return Err(WebServiceError::validation(
                "repositoryUrl must resolve to a public address",
            ));
        }
        Host::Domain(domain) => {
            let mut addresses = timeout(GIT_TIMEOUT, lookup_host((domain, 443)))
                .await
                .map_err(|_| WebServiceError::validation("repositoryUrl DNS lookup timed out"))?
                .map_err(|_| {
                    WebServiceError::validation("repositoryUrl host could not be resolved")
                })?
                .map(|address| address.ip())
                .collect::<Vec<_>>();
            addresses.sort_unstable();
            addresses.dedup();
            if addresses.is_empty() || addresses.iter().copied().any(is_forbidden_ip) {
                return Err(WebServiceError::validation(
                    "repositoryUrl must resolve only to public addresses",
                ));
            }
            addresses
        }
        Host::Ipv4(address) => vec![IpAddr::V4(address)],
        Host::Ipv6(address) => vec![IpAddr::V6(address)],
    };
    Ok(ValidatedRepositoryTarget {
        url,
        host_name,
        resolved_addresses,
    })
}

async fn clone_repository(
    request: &GitSourceImportRequest,
    target: &ValidatedRepositoryTarget,
    repository_root: &Path,
) -> WebServiceResult<()> {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("credential.helper=")
        .arg("-c")
        .arg("core.askPass=")
        .arg("-c")
        .arg("http.followRedirects=false");
    for address in &target.resolved_addresses {
        let address = match address {
            IpAddr::V4(address) => address.to_string(),
            IpAddr::V6(address) => format!("[{address}]"),
        };
        command.arg("-c").arg(format!(
            "http.curloptResolve={}:443:{address}",
            target.host_name
        ));
    }
    command
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("--single-branch")
        .arg("--no-tags");
    if let Some(git_ref) = request.git_ref.as_deref() {
        command.arg("--branch").arg(git_ref);
    }
    command
        .arg(target.url.as_str())
        .arg(repository_root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let status = timeout(GIT_TIMEOUT, command.status())
        .await
        .map_err(|_| WebServiceError::validation("Git repository import timed out"))?
        .map_err(|error| {
            WebServiceError::Internal(format!("start Git repository import failed: {error}"))
        })?;
    if !status.success() {
        return Err(WebServiceError::validation(
            "Git repository could not be cloned with anonymous HTTPS access",
        ));
    }
    Ok(())
}

async fn resolve_commit_hash(repository_root: &Path) -> WebServiceResult<String> {
    let output = timeout(
        Duration::from_secs(5),
        Command::new("git")
            .arg("-C")
            .arg(repository_root)
            .arg("rev-parse")
            .arg("--verify")
            .arg("HEAD")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output(),
    )
    .await
    .map_err(|_| WebServiceError::Internal("resolve Git commit timed out".to_string()))?
    .map_err(|error| WebServiceError::Internal(format!("resolve Git commit failed: {error}")))?;
    let hash = String::from_utf8(output.stdout).map_err(|_| {
        WebServiceError::Internal("Git returned a non-UTF8 commit hash".to_string())
    })?;
    let hash = hash.trim().to_ascii_lowercase();
    if !output.status.success()
        || hash.len() != 40
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(WebServiceError::Internal(
            "Git returned an invalid commit hash".to_string(),
        ));
    }
    Ok(hash)
}

fn package_repository(repository_root: &Path) -> WebServiceResult<PackagedRepository> {
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    collect_repository_files(
        repository_root,
        repository_root,
        &mut files,
        &mut total_bytes,
    )?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() {
        return Err(WebServiceError::validation(
            "Git repository does not contain deployable files",
        ));
    }
    let config_snapshot = SourceVersionConfigSnapshot {
        app_config_detected: files
            .iter()
            .any(|(path, _)| path == "sdkwork.app.config.json"),
        deployment_config_detected: files
            .iter()
            .any(|(path, _)| path == "etc/sdkwork.deployment.config.json"),
        ..SourceVersionConfigSnapshot::default()
    };
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let fixed_time = zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
        .map_err(|_| WebServiceError::Internal("create ZIP timestamp failed".to_string()))?;
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(fixed_time)
        .unix_permissions(0o644);
    for (path, absolute) in files {
        writer.start_file(path, options).map_err(|error| {
            WebServiceError::Internal(format!("create source ZIP failed: {error}"))
        })?;
        let content = std::fs::read(absolute).map_err(internal_io("read Git source file"))?;
        writer
            .write_all(&content)
            .map_err(internal_io("write Git source ZIP"))?;
    }
    let archive = writer
        .finish()
        .map_err(|error| WebServiceError::Internal(format!("finish source ZIP failed: {error}")))?
        .into_inner();
    if archive.len() > MAX_ARCHIVE_BYTES {
        return Err(WebServiceError::validation(
            "Git source archive exceeds 64 MiB",
        ));
    }
    Ok(PackagedRepository {
        archive,
        config_snapshot,
    })
}

fn collect_repository_files(
    repository_root: &Path,
    directory: &Path,
    files: &mut Vec<(String, PathBuf)>,
    total_bytes: &mut u64,
) -> WebServiceResult<()> {
    for entry in std::fs::read_dir(directory).map_err(internal_io("read Git source directory"))? {
        let entry = entry.map_err(internal_io("read Git source entry"))?;
        let file_type = entry
            .file_type()
            .map_err(internal_io("inspect Git source entry"))?;
        let absolute = entry.path();
        let relative = absolute.strip_prefix(repository_root).map_err(|_| {
            WebServiceError::Internal("Git source path escaped its root".to_string())
        })?;
        let normalized = normalize_source_path(relative)?;
        if normalized.split('/').any(|segment| segment == ".git") {
            continue;
        }
        if file_type.is_symlink() {
            return Err(WebServiceError::validation(
                "Git source must not contain symbolic links",
            ));
        }
        if file_type.is_dir() {
            collect_repository_files(repository_root, &absolute, files, total_bytes)?;
            continue;
        }
        if !file_type.is_file() {
            return Err(WebServiceError::validation(
                "Git source contains an unsupported filesystem entry",
            ));
        }
        let size = entry
            .metadata()
            .map_err(internal_io("inspect Git source file"))?
            .len();
        if size > MAX_FILE_BYTES {
            return Err(WebServiceError::validation(
                "Git source contains a file larger than 16 MiB",
            ));
        }
        *total_bytes = total_bytes.saturating_add(size);
        if *total_bytes > MAX_TOTAL_BYTES {
            return Err(WebServiceError::validation(
                "Git source exceeds 64 MiB of file content",
            ));
        }
        files.push((normalized, absolute));
        if files.len() > MAX_FILES {
            return Err(WebServiceError::validation(
                "Git source contains more than 500 files",
            ));
        }
    }
    Ok(())
}

fn normalize_source_path(path: &Path) -> WebServiceResult<String> {
    let mut segments = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(WebServiceError::validation(
                "Git source contains an unsafe or excessively deep path",
            ));
        };
        segments.push(
            segment
                .to_str()
                .ok_or_else(|| WebServiceError::validation("Git source paths must use UTF-8"))?,
        );
    }
    let normalized = segments.join("/");
    let depth = normalized.split('/').count();
    if normalized.is_empty()
        || normalized.len() > MAX_PATH_BYTES
        || depth > MAX_PATH_DEPTH
        || normalized.chars().any(char::is_control)
    {
        return Err(WebServiceError::validation(
            "Git source contains an unsafe or excessively deep path",
        ));
    }
    Ok(normalized)
}

fn configured_allowed_hosts() -> Option<Vec<String>> {
    let hosts = std::env::var("SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS").ok()?;
    let hosts = hosts
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    (!hosts.is_empty()).then_some(hosts)
}

fn is_forbidden_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_forbidden_ipv4(address),
        IpAddr::V6(address) => is_forbidden_ipv6(address),
    }
}

fn is_forbidden_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_multicast()
        || address.is_unspecified()
        || a == 0
        || a >= 240
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && matches!(b, 18 | 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
}

fn is_forbidden_ipv6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || address.to_ipv4_mapped().is_some_and(is_forbidden_ipv4)
}

fn source_archive_name(version_tag: &str) -> String {
    let version = version_tag
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("application-{version}.zip")
}

fn sha256_hex(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn map_drive_error(error: sdkwork_drive_workspace_service::DriveServiceError) -> WebServiceError {
    WebServiceError::Internal(format!("store Git source in Drive failed: {error:?}"))
}

fn internal_io(context: &'static str) -> impl FnOnce(std::io::Error) -> WebServiceError {
    move |error| WebServiceError::Internal(format!("{context} failed: {error}"))
}

#[cfg(test)]
mod tests;
