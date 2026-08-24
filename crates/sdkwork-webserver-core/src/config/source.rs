//! Unified Web Server configuration loading (strategy pattern).
//!
//! `ConfigSource` is the common interface over the three authored formats:
//! stock nginx `http`/`stream` configuration (`NginxConfConfigSource`),
//! layout v3 `server.toml` directories and single-file TOML
//! (`TomlConfigSource`), and the JSON application config
//! (`JsonConfigSource`). Every strategy materializes into the same
//! `WebServerAppConfig` model, so semantic validation and compilation are
//! shared downstream — one interface, different implementations, high
//! cohesion and low coupling.
//!
//! `WebServerConfigLoader` is the registry facade: it auto-detects the
//! format of a path (file extension, directory layout, then content
//! sniffing) and dispatches to the matching strategy. `ConfigLoadOptions`
//! can pin the format explicitly for ambiguous paths (for example an
//! extension-less nginx conf that starts with a TOML-looking `key =` line).

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::nginx::{load_nginx_compat, NginxLoadReport};

use super::{
    error::WebServerConfigError,
    loader::{load_json_app_config, WebServerConfigFileRevision},
    model::WebServerAppConfig,
    server_toml::{load_server_toml_app, load_server_toml_file},
    validate_webserver_config, CompiledWebServerApp,
};

/// Default `app_key` for strategies that do not receive one explicitly.
pub const DEFAULT_APP_KEY: &str = "webserver";
/// Default layout v3 profile for TOML directories.
pub const DEFAULT_TOML_PROFILE: &str = "standalone";
/// Default lifecycle environment for layout v3 TOML directories.
pub const DEFAULT_TOML_ENVIRONMENT: &str = "production";

/// Number of leading bytes read for content sniffing.
const MAX_SNIFF_BYTES: usize = 64 * 1024;

/// Authored configuration formats supported by the unified loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// Stock nginx `http`/`stream` configuration (`nginx.conf`,
    /// `sites-enabled/*.conf`).
    NginxConf,
    /// Layout v3 `server.toml` directory or a single TOML file.
    Toml,
    /// JSON application config (`sdkwork.webserver.config.json`).
    Json,
}

impl ConfigFormat {
    pub const ALL: [ConfigFormat; 3] = [ConfigFormat::Json, ConfigFormat::Toml, ConfigFormat::NginxConf];

    pub fn as_str(self) -> &'static str {
        match self {
            ConfigFormat::NginxConf => "nginx",
            ConfigFormat::Toml => "toml",
            ConfigFormat::Json => "json",
        }
    }

    /// Format by file extension (`nginx.conf` file name included). Returns
    /// `None` when the path has no recognizable extension.
    pub fn from_extension(path: &Path) -> Option<ConfigFormat> {
        let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        if file_name == "nginx.conf" {
            return Some(ConfigFormat::NginxConf);
        }
        match path.extension().and_then(|value| value.to_str()) {
            Some("json") => Some(ConfigFormat::Json),
            Some("toml") => Some(ConfigFormat::Toml),
            Some("conf") | Some("nginx") | Some("nginxconf") => Some(ConfigFormat::NginxConf),
            _ => None,
        }
    }

    /// Detect the format of a path deterministically: extension, directory
    /// layout, then content sniffing for unknown extensions.
    pub fn detect(path: &Path) -> Result<ConfigFormat, WebServerConfigError> {
        let metadata =
            fs::metadata(path).map_err(|source| WebServerConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        if metadata.is_dir() {
            return detect_directory(path);
        }
        if let Some(format) = ConfigFormat::from_extension(path) {
            return Ok(format);
        }
        sniff_format(path)
    }
}

fn detect_directory(path: &Path) -> Result<ConfigFormat, WebServerConfigError> {
    if path.join("server.common.toml").is_file() {
        return Ok(ConfigFormat::Toml);
    }
    let read_dir = fs::read_dir(path).map_err(|source| WebServerConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut has_conf = false;
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".conf") {
            has_conf = true;
        } else if name == "sdkwork.webserver.config.json" {
            return Ok(ConfigFormat::Json);
        }
    }
    if has_conf {
        return Ok(ConfigFormat::NginxConf);
    }
    Err(WebServerConfigError::Materialize(format!(
        "cannot detect the config format of directory {}: expected a server.common.toml layout, *.conf site files, or sdkwork.webserver.config.json (pass --format to override)",
        path.display()
    )))
}

/// Sniff the format of a file with an unknown extension. JSON opens with
/// `{`, TOML with a `[table]` header or a `key = value` assignment, and
/// nginx with a directive name; `#` line comments are skipped.
fn sniff_format(path: &Path) -> Result<ConfigFormat, WebServerConfigError> {
    let bytes = read_sniff_bytes(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut position = 0;
    loop {
        while let Some(character) = text[position..].chars().next() {
            if character.is_whitespace() {
                position += character.len_utf8();
            } else {
                break;
            }
        }
        if text[position..].starts_with('#') {
            match text[position..].find('\n') {
                Some(offset) => position += offset + 1,
                None => break,
            }
            continue;
        }
        break;
    }
    let Some(first) = text[position..].chars().next() else {
        return Err(WebServerConfigError::Materialize(format!(
            "cannot detect the config format of {}: the file is empty",
            path.display()
        )));
    };
    match first {
        '{' => return Ok(ConfigFormat::Json),
        '[' => return Ok(ConfigFormat::Toml),
        '=' => {
            return Err(WebServerConfigError::Materialize(format!(
                "cannot detect the config format of {}: the file does not start with a recognizable JSON, TOML, or nginx construct (pass --format to override)",
                path.display()
            )))
        }
        _ => {}
    }
    // TOML assignments are `key = value` (or `key=value`); nginx starts with
    // a bare directive name whose arguments may contain `=` only after the
    // directive token (`limit_req_zone … zone=perip:10m;`).
    let line_end = text[position..]
        .find('\n')
        .unwrap_or(text.len() - position);
    let line = &text[position..position + line_end];
    let token_end = line
        .char_indices()
        .find_map(|(offset, character)| {
            if character.is_whitespace() || matches!(character, '{' | ';' | '}') {
                Some(offset)
            } else {
                None
            }
        })
        .unwrap_or(line.len());
    let token = &line[..token_end];
    if token.contains('=') || line.contains(" = ") {
        Ok(ConfigFormat::Toml)
    } else {
        Ok(ConfigFormat::NginxConf)
    }
}

fn read_sniff_bytes(path: &Path) -> Result<Vec<u8>, WebServerConfigError> {
    let metadata = fs::metadata(path).map_err(|source| WebServerConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > MAX_SNIFF_BYTES as u64 {
        return Err(WebServerConfigError::Materialize(format!(
            "cannot sniff the config format of {}: the file is {} bytes (sniffing is limited to {} bytes); pass --format to override",
            path.display(),
            metadata.len(),
            MAX_SNIFF_BYTES
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_to_end(&mut file, &mut bytes))
        .map_err(|source| WebServerConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(bytes)
}

/// Options shared by every config source strategy.
#[derive(Debug, Clone, Default)]
pub struct ConfigLoadOptions {
    /// Explicit format override; skips auto-detection when set.
    pub format: Option<ConfigFormat>,
    /// App key stamped into the materialized model. The JSON source keeps
    /// the `appKey` declared inside the document.
    pub app_key: Option<String>,
    /// Layout v3 profile for TOML directories (`standalone` / `cloud`);
    /// defaults to `standalone`.
    pub profile: Option<String>,
    /// Layout v3 lifecycle environment (`development` / `test` / `staging` /
    /// `production`); defaults to `production`.
    pub environment: Option<String>,
}

impl ConfigLoadOptions {
    pub fn with_format(format: ConfigFormat) -> Self {
        Self {
            format: Some(format),
            ..Self::default()
        }
    }
}

/// One materialized configuration plus its source provenance.
#[derive(Debug)]
pub struct LoadedWebServerConfig {
    pub app: WebServerAppConfig,
    pub format: ConfigFormat,
    /// The file or directory that was loaded.
    pub source: PathBuf,
    /// Single-file sources carry a content revision for change detection.
    pub revision: Option<WebServerConfigFileRevision>,
    /// nginx progressive compatibility: site files skipped with their
    /// materialization reasons.
    pub skipped: Vec<(PathBuf, String)>,
}

/// Strategy interface implemented by every authored config format.
pub trait ConfigSource: Send + Sync {
    fn format(&self) -> ConfigFormat;

    /// Whether this strategy can load the path (extension or directory
    /// layout). Content-based detection is owned by `ConfigFormat::detect`.
    fn matches(&self, path: &Path) -> bool;

    fn load(
        &self,
        path: &Path,
        options: &ConfigLoadOptions,
    ) -> Result<LoadedWebServerConfig, WebServerConfigError>;
}

/// JSON strategy: schema-validated `sdkwork.webserver.config.json`.
#[derive(Debug, Default)]
pub struct JsonConfigSource;

impl ConfigSource for JsonConfigSource {
    fn format(&self) -> ConfigFormat {
        ConfigFormat::Json
    }

    fn matches(&self, path: &Path) -> bool {
        ConfigFormat::from_extension(path) == Some(ConfigFormat::Json)
    }

    fn load(
        &self,
        path: &Path,
        _options: &ConfigLoadOptions,
    ) -> Result<LoadedWebServerConfig, WebServerConfigError> {
        let (app, revision) = load_json_app_config(path)?;
        Ok(LoadedWebServerConfig {
            app,
            format: ConfigFormat::Json,
            source: path.to_path_buf(),
            revision: Some(revision),
            skipped: Vec::new(),
        })
    }
}

/// TOML strategy: layout v3 directory (`server.common.toml` plus
/// `server.<environment>.toml` and `server.<profile>.toml`) or a single TOML file.
#[derive(Debug, Default)]
pub struct TomlConfigSource;

impl ConfigSource for TomlConfigSource {
    fn format(&self) -> ConfigFormat {
        ConfigFormat::Toml
    }

    fn matches(&self, path: &Path) -> bool {
        if path.is_dir() {
            path.join("server.common.toml").is_file()
        } else {
            ConfigFormat::from_extension(path) == Some(ConfigFormat::Toml)
        }
    }

    fn load(
        &self,
        path: &Path,
        options: &ConfigLoadOptions,
    ) -> Result<LoadedWebServerConfig, WebServerConfigError> {
        let app_key = options.app_key.as_deref().unwrap_or(DEFAULT_APP_KEY);
        if path.is_dir() {
            let profile = options.profile.as_deref().unwrap_or(DEFAULT_TOML_PROFILE);
            let environment = options.environment.as_deref().unwrap_or(DEFAULT_TOML_ENVIRONMENT);
            let app = load_server_toml_app(path, profile, environment, app_key)?;
            return Ok(LoadedWebServerConfig {
                app,
                format: ConfigFormat::Toml,
                source: path.to_path_buf(),
                revision: None,
                skipped: Vec::new(),
            });
        }
        let app = load_server_toml_file(path, app_key)?;
        Ok(LoadedWebServerConfig {
            app,
            format: ConfigFormat::Toml,
            source: path.to_path_buf(),
            revision: Some(super::loader::inspect_webserver_config_revision(path)?),
            skipped: Vec::new(),
        })
    }
}

/// nginx strategy: stock `nginx.conf`, a `sites-enabled` directory, or a
/// mixed tree with companion `stream-conf.d` (`load_nginx_compat`).
#[derive(Debug, Default)]
pub struct NginxConfConfigSource;

impl ConfigSource for NginxConfConfigSource {
    fn format(&self) -> ConfigFormat {
        ConfigFormat::NginxConf
    }

    fn matches(&self, path: &Path) -> bool {
        if path.is_dir() {
            path.join("server.common.toml").is_file() == false
                && path
                    .read_dir()
                    .map(|entries| {
                        entries.flatten().any(|entry| {
                            entry
                                .path()
                                .extension()
                                .and_then(|value| value.to_str())
                                == Some("conf")
                        })
                    })
                    .unwrap_or(false)
        } else {
            ConfigFormat::from_extension(path) == Some(ConfigFormat::NginxConf)
        }
    }

    fn load(
        &self,
        path: &Path,
        options: &ConfigLoadOptions,
    ) -> Result<LoadedWebServerConfig, WebServerConfigError> {
        let app_key = options.app_key.as_deref().unwrap_or(DEFAULT_APP_KEY);
        let report: NginxLoadReport = load_nginx_compat(path, app_key)?;
        Ok(LoadedWebServerConfig {
            app: report.app,
            format: ConfigFormat::NginxConf,
            source: path.to_path_buf(),
            revision: None,
            skipped: report.skipped,
        })
    }
}

/// Registry facade over the registered `ConfigSource` strategies.
#[derive(Default)]
pub struct WebServerConfigLoader {
    sources: Vec<Box<dyn ConfigSource>>,
}

impl WebServerConfigLoader {
    pub fn new() -> Self {
        Self {
            sources: vec![
                Box::<JsonConfigSource>::default(),
                Box::<TomlConfigSource>::default(),
                Box::<NginxConfConfigSource>::default(),
            ],
        }
    }

    /// Resolve the format for a path: explicit option, then detection.
    pub fn format_of(
        &self,
        path: &Path,
        options: &ConfigLoadOptions,
    ) -> Result<ConfigFormat, WebServerConfigError> {
        if let Some(format) = options.format {
            return Ok(format);
        }
        ConfigFormat::detect(path)
    }

    /// Load and materialize a configuration of any supported format.
    pub fn load(
        &self,
        path: &Path,
        options: &ConfigLoadOptions,
    ) -> Result<LoadedWebServerConfig, WebServerConfigError> {
        let format = self.format_of(path, options)?;
        let source = self
            .sources
            .iter()
            .find(|candidate| candidate.format() == format)
            .ok_or_else(|| {
                WebServerConfigError::Materialize(format!(
                    "no config source is registered for format {}",
                    format.as_str()
                ))
            })?;
        source.load(path, options)
    }

    /// Load, semantically validate, and compile a configuration of any
    /// supported format into the ready-to-serve app.
    pub fn load_and_compile(
        &self,
        path: &Path,
        options: &ConfigLoadOptions,
    ) -> Result<CompiledWebServerApp, WebServerConfigError> {
        let loaded = self.load(path, options)?;
        validate_webserver_config(&loaded.app)?;
        let base_directory = base_directory_for(&loaded);
        CompiledWebServerApp::compile(loaded.app, &base_directory)
    }
}

/// Compile base directory per format. JSON and single-file TOML anchor
/// relative roots and certificate paths to the config file's directory;
/// TOML directories anchor to the layout directory; nginx materialization
/// already resolved every path against the conf file's directory, so the
/// compiled model treats them as absolute.
fn base_directory_for(loaded: &LoadedWebServerConfig) -> PathBuf {
    let parent = || {
        loaded
            .source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"))
    };
    match loaded.format {
        ConfigFormat::Json => parent(),
        ConfigFormat::Toml if loaded.source.is_dir() => loaded.source.clone(),
        ConfigFormat::Toml => parent(),
        ConfigFormat::NginxConf => PathBuf::from("/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(temp: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
        let path = temp.path().join(name);
        fs::write(&path, content).expect("write fixture");
        path
    }

    #[test]
    fn detects_formats_by_extension() {
        let temp = tempfile::tempdir().expect("temp");
        let json = write(&temp, "app.json", "{}");
        let toml = write(&temp, "server.toml", "[main]\n");
        let conf = write(&temp, "site.conf", "server { listen 80; }\n");
        let nginx_named = write(&temp, "nginx.conf", "events {}\n");
        assert_eq!(ConfigFormat::detect(&json).expect("json"), ConfigFormat::Json);
        assert_eq!(ConfigFormat::detect(&toml).expect("toml"), ConfigFormat::Toml);
        assert_eq!(
            ConfigFormat::detect(&conf).expect("conf"),
            ConfigFormat::NginxConf
        );
        assert_eq!(
            ConfigFormat::detect(&nginx_named).expect("nginx.conf"),
            ConfigFormat::NginxConf
        );
    }

    #[test]
    fn sniffs_unknown_extensions_by_content() {
        let temp = tempfile::tempdir().expect("temp");
        let json = write(&temp, "config.cfg", "{\n  \"appKey\": \"a\"\n}");
        let toml = write(&temp, "config.xyz", "# comment\n[main]\nworkerProcesses = 4\n");
        let toml_assignment = write(&temp, "config.data", "enabled = true\n");
        let nginx = write(&temp, "config.inc", "# comment\nserver {\n  listen 80;\n}\n");
        assert_eq!(ConfigFormat::detect(&json).expect("json"), ConfigFormat::Json);
        assert_eq!(ConfigFormat::detect(&toml).expect("toml"), ConfigFormat::Toml);
        assert_eq!(
            ConfigFormat::detect(&toml_assignment).expect("toml assignment"),
            ConfigFormat::Toml
        );
        assert_eq!(
            ConfigFormat::detect(&nginx).expect("nginx"),
            ConfigFormat::NginxConf
        );
    }

    #[test]
    fn detects_directory_layouts() {
        let temp = tempfile::tempdir().expect("temp");
        let layout = temp.path().join("layout");
        let sites = temp.path().join("sites");
        fs::create_dir_all(&layout).unwrap();
        fs::create_dir_all(&sites).unwrap();
        fs::write(
            layout.join("server.common.toml"),
            "specVersion = 1\nkind = \"x\"\n",
        )
        .unwrap();
        fs::write(layout.join("server.standalone.toml"), "profile = \"standalone\"\n").unwrap();
        fs::write(sites.join("web.conf"), "server { listen 80; }\n").unwrap();
        assert_eq!(
            ConfigFormat::detect(&layout).expect("layout"),
            ConfigFormat::Toml
        );
        assert_eq!(
            ConfigFormat::detect(&sites).expect("sites"),
            ConfigFormat::NginxConf
        );
        let empty = temp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(ConfigFormat::detect(&empty).is_err());
    }

    #[test]
    fn loader_dispatches_to_the_matching_strategy() {
        let temp = tempfile::tempdir().expect("temp");
        let nginx = write(
            &temp,
            "web.conf",
            "server {\n    listen 80;\n    server_name web.example.com;\n    location / { return 200 \"ok\"; }\n}\n",
        );
        let toml = write(
            &temp,
            "web.toml",
            "[[http.server]]\nlisten = [\"80\"]\nserverName = [\"web.example.com\"]\n\n[[http.server.location]]\nmatch = \"/\"\nreturnStatus = 200\nreturnBody = \"ok\"\n",
        );
        let loader = WebServerConfigLoader::new();
        let nginx_loaded = loader.load(&nginx, &ConfigLoadOptions::default()).expect("nginx");
        assert_eq!(nginx_loaded.format, ConfigFormat::NginxConf);
        assert_eq!(nginx_loaded.app.virtual_hosts.len(), 1);
        let toml_loaded = loader.load(&toml, &ConfigLoadOptions::default()).expect("toml");
        assert_eq!(toml_loaded.format, ConfigFormat::Toml);
        assert_eq!(toml_loaded.app.virtual_hosts.len(), 1);
        assert!(toml_loaded.revision.is_some());
    }

    #[test]
    fn explicit_format_override_skips_detection() {
        let temp = tempfile::tempdir().expect("temp");
        // Extension-less nginx content is sniffed as nginx by default.
        let path = write(
            &temp,
            "extensionless",
            "server { listen 80; location / { return 200 \"ok\"; } }\n",
        );
        let loader = WebServerConfigLoader::new();
        let detected = loader
            .format_of(&path, &ConfigLoadOptions::default())
            .expect("detect");
        assert_eq!(detected, ConfigFormat::NginxConf);
        let forced = loader
            .format_of(&path, &ConfigLoadOptions::with_format(ConfigFormat::Toml))
            .expect("forced");
        assert_eq!(forced, ConfigFormat::Toml);
    }

    #[test]
    fn single_file_toml_materializes_and_carries_revision() {
        let temp = tempfile::tempdir().expect("temp");
        let toml = write(
            &temp,
            "server.toml",
            "specVersion = 1\nkind = \"sdkwork.webserver.server\"\nid = \"single\"\nprofile = \"standalone\"\n\n[[http.server]]\nlisten = [\"8080\"]\nserverName = [\"single.local\"]\n\n[[http.server.location]]\nmatch = \"/\"\nreturnStatus = 200\n",
        );
        let loader = WebServerConfigLoader::new();
        let loaded = loader.load(&toml, &ConfigLoadOptions::default()).expect("load");
        assert_eq!(loaded.format, ConfigFormat::Toml);
        assert_eq!(loaded.app.app_key, DEFAULT_APP_KEY);
        assert_eq!(loaded.app.virtual_hosts.len(), 1);
        let revision = loaded.revision.expect("single-file revision");
        assert_eq!(revision.size_bytes(), fs::metadata(&toml).unwrap().len() as u64);
    }

    #[test]
    fn unknown_extension_without_sniffable_content_fails_closed() {
        let temp = tempfile::tempdir().expect("temp");
        let path = write(&temp, "empty.weird", "");
        let error = ConfigFormat::detect(&path).expect_err("empty file must fail");
        assert!(error.to_string().contains("cannot detect"));
    }
}
