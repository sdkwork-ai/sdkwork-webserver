use super::*;
use sdkwork_webserver_contract::WebServiceErrorKind;
use std::fs::File;
use std::sync::Mutex;

static GIT_ALLOWED_HOSTS_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvironmentVariableGuard {
    previous_value: Option<std::ffi::OsString>,
}

impl EnvironmentVariableGuard {
    fn set(value: &str) -> Self {
        let previous_value = std::env::var_os("SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS");
        std::env::set_var("SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS", value);
        Self { previous_value }
    }
}

impl Drop for EnvironmentVariableGuard {
    fn drop(&mut self) {
        match &self.previous_value {
            Some(value) => std::env::set_var("SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS", value),
            None => std::env::remove_var("SDKWORK_WEBSERVER_GIT_ALLOWED_HOSTS"),
        }
    }
}

#[test]
fn private_and_documentation_addresses_are_rejected() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "100.64.0.1",
        "169.254.1.1",
        "192.0.2.1",
        "198.51.100.1",
        "203.0.113.1",
        "::1",
        "fc00::1",
        "fe80::1",
        "2001:db8::1",
    ] {
        assert!(is_forbidden_ip(address.parse().unwrap()), "{address}");
    }
    assert!(!is_forbidden_ip("8.8.8.8".parse().unwrap()));
    assert!(!is_forbidden_ip("2606:4700:4700::1111".parse().unwrap()));
}

#[tokio::test]
async fn invalid_repository_url_forms_are_rejected_before_network_access() {
    for repository_url in [
        "team/repository.git",
        "ssh://git@8.8.8.8/team/repository.git",
        "http://8.8.8.8/team/repository.git",
        "https://user@8.8.8.8/team/repository.git",
        "https://user:password@8.8.8.8/team/repository.git",
        "https://8.8.8.8:8443/team/repository.git",
        "https://8.8.8.8/team/repository.git?token=secret",
        "https://8.8.8.8/team/repository.git#main",
    ] {
        expect_validation(validate_repository_target(repository_url).await);
    }
}

#[tokio::test]
// The environment lock must stay held across awaits so parallel tests cannot
// observe a half-set allowlist; tokio::test runs on a current-thread runtime.
#[allow(clippy::await_holding_lock)]
async fn configured_host_allowlist_is_case_insensitive_and_fail_closed() {
    let lock = GIT_ALLOWED_HOSTS_ENV_LOCK.lock().unwrap();
    let _guard = EnvironmentVariableGuard::set(" 8.8.8.8, GITHUB.COM ");
    let validated = validate_repository_target("https://8.8.8.8/team/repository.git").await;
    let rejected = validate_repository_target("https://1.1.1.1/team/repository.git").await;
    drop(lock);
    validated.expect("allowlisted public host");
    expect_validation(rejected);
}

#[tokio::test]
// The environment lock must stay held across awaits so parallel tests cannot
// observe a half-set allowlist; tokio::test runs on a current-thread runtime.
#[allow(clippy::await_holding_lock)]
async fn missing_host_allowlist_disables_git_import() {
    let lock = GIT_ALLOWED_HOSTS_ENV_LOCK.lock().unwrap();
    let _guard = EnvironmentVariableGuard::set("");
    let result = validate_repository_target("https://8.8.8.8/team/repository.git").await;
    drop(lock);
    expect_validation(result);
}

#[tokio::test]
// The environment lock must stay held across awaits so parallel tests cannot
// observe a half-set allowlist; tokio::test runs on a current-thread runtime.
#[allow(clippy::await_holding_lock)]
async fn validated_target_retains_the_pinned_public_address() {
    let lock = GIT_ALLOWED_HOSTS_ENV_LOCK.lock().unwrap();
    let _guard = EnvironmentVariableGuard::set("8.8.8.8");
    let target = validate_repository_target("https://8.8.8.8/team/repository.git")
        .await
        .expect("allowlisted public target");
    drop(lock);
    assert_eq!(target.host_name, "8.8.8.8");
    assert_eq!(
        target.resolved_addresses,
        vec!["8.8.8.8".parse::<IpAddr>().unwrap()]
    );
}

#[test]
fn repository_packaging_is_deterministic_and_detects_standard_configuration() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("etc")).unwrap();
    std::fs::write(root.path().join("index.html"), "hello").unwrap();
    std::fs::write(root.path().join("sdkwork.app.config.json"), "{}").unwrap();
    std::fs::write(root.path().join("etc/sdkwork.deployment.config.json"), "{}").unwrap();
    let first = package_repository(root.path()).unwrap();
    let second = package_repository(root.path()).unwrap();
    assert_eq!(first.archive, second.archive);
    assert!(first.config_snapshot.app_config_detected);
    assert!(first.config_snapshot.deployment_config_detected);
}

#[test]
fn unsafe_and_excessively_deep_paths_are_rejected() {
    expect_validation(normalize_source_path(Path::new("../escape.txt")));
    expect_validation(normalize_source_path(Path::new("/absolute.txt")));

    let root = TempDir::new().unwrap();
    let mut directory = root.path().to_path_buf();
    for index in 0..=MAX_PATH_DEPTH {
        directory.push(format!("level-{index}"));
    }
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("index.html"), "deep").unwrap();
    expect_validation(package_repository(root.path()));
}

#[cfg(any(unix, windows))]
#[test]
fn symbolic_links_are_rejected() {
    let root = TempDir::new().unwrap();
    let target = root.path().join("target.txt");
    let link = root.path().join("link.txt");
    std::fs::write(&target, "target").unwrap();
    // Windows requires the SeCreateSymbolicLinkPrivilege (admin or
    // Developer Mode); without it the fixture cannot materialize and the
    // rejection behavior is exercised on privileged platforms instead.
    if let Err(error) = create_file_symlink(&target, &link) {
        eprintln!("skip: symbolic links unavailable in this environment: {error}");
        return;
    }
    expect_validation(package_repository(root.path()));
}

#[test]
fn file_count_limit_is_enforced() {
    let root = TempDir::new().unwrap();
    for index in 0..=MAX_FILES {
        std::fs::write(root.path().join(format!("file-{index:03}.txt")), []).unwrap();
    }
    expect_validation(package_repository(root.path()));
}

#[test]
fn individual_file_size_limit_is_enforced() {
    let root = TempDir::new().unwrap();
    File::create(root.path().join("oversized.bin"))
        .unwrap()
        .set_len(MAX_FILE_BYTES + 1)
        .unwrap();
    expect_validation(package_repository(root.path()));
}

#[test]
fn total_file_size_limit_is_enforced() {
    let root = TempDir::new().unwrap();
    for index in 0..5 {
        File::create(root.path().join(format!("part-{index}.bin")))
            .unwrap()
            .set_len(MAX_FILE_BYTES)
            .unwrap();
    }
    expect_validation(package_repository(root.path()));
}

fn expect_validation<T>(result: WebServiceResult<T>) {
    match result {
        Ok(_) => panic!("expected validation failure"),
        Err(error) => assert_eq!(error.kind(), WebServiceErrorKind::Validation),
    }
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
