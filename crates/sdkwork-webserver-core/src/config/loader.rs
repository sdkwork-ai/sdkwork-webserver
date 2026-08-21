use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

use sdkwork_utils_rust::crypto::sha256_hash;
use serde_json::Value;

use super::{
    validate_webserver_config, CompiledWebServerApp, ConfigDiagnostic, WebServerAppConfig,
    WebServerConfigError,
};

pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SCHEMA_DIAGNOSTICS: usize = 64;
const SCHEMA: &str = include_str!("../../../../specs/sdkwork.webserver.config.schema.json");

#[derive(Debug)]
pub struct CompiledWebServerRevision {
    app: CompiledWebServerApp,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebServerConfigFileRevision {
    sha256: String,
    size_bytes: u64,
}

impl WebServerConfigFileRevision {
    pub(crate) fn new(sha256: String, size_bytes: u64) -> Self {
        Self {
            sha256,
            size_bytes,
        }
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

impl CompiledWebServerRevision {
    pub fn app(&self) -> &CompiledWebServerApp {
        &self.app
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn into_app(self) -> CompiledWebServerApp {
        self.app
    }
}

pub fn load_and_compile_webserver_config(
    path: impl AsRef<Path>,
) -> Result<CompiledWebServerApp, WebServerConfigError> {
    load_and_compile_webserver_config_revision(path).map(CompiledWebServerRevision::into_app)
}

pub fn load_and_compile_webserver_config_revision(
    path: impl AsRef<Path>,
) -> Result<CompiledWebServerRevision, WebServerConfigError> {
    let path = path.as_ref();
    let bytes = read_bounded_config(path)?;
    let sha256 = sha256_hash(&bytes);
    compile_webserver_config_revision(path, bytes, sha256)
}

pub fn inspect_webserver_config_revision(
    path: impl AsRef<Path>,
) -> Result<WebServerConfigFileRevision, WebServerConfigError> {
    let bytes = read_bounded_config(path.as_ref())?;
    Ok(WebServerConfigFileRevision {
        sha256: sha256_hash(&bytes),
        size_bytes: bytes.len() as u64,
    })
}

pub(crate) fn read_bounded_config(path: &Path) -> Result<Vec<u8>, WebServerConfigError> {
    let metadata = fs::metadata(path).map_err(|source| WebServerConfigError::Inspect {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(WebServerConfigError::TooLarge {
            path: path.to_path_buf(),
            actual_bytes: metadata.len(),
            maximum_bytes: MAX_CONFIG_BYTES,
        });
    }

    let mut file = File::open(path).map_err(|source| WebServerConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_CONFIG_BYTES + 1) as usize);
    file.by_ref()
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| WebServerConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(WebServerConfigError::TooLarge {
            path: path.to_path_buf(),
            actual_bytes: bytes.len() as u64,
            maximum_bytes: MAX_CONFIG_BYTES,
        });
    }
    Ok(bytes)
}

fn compile_webserver_config_revision(
    path: &Path,
    bytes: Vec<u8>,
    sha256: String,
) -> Result<CompiledWebServerRevision, WebServerConfigError> {
    let (config, _revision) = deserialize_json_app(path, &bytes, &sha256)?;
    let base_directory = path.parent().unwrap_or_else(|| Path::new("."));
    let app = CompiledWebServerApp::compile(config, base_directory)?;
    Ok(CompiledWebServerRevision {
        app,
        sha256,
        size_bytes: bytes.len() as u64,
    })
}

/// Load a JSON app config from disk and return the validated model plus its
/// content revision. Shared by the JSON strategy of the unified config
/// loader (`config::source`) and the revision-based entry points above.
pub(crate) fn load_json_app_config(
    path: &Path,
) -> Result<(WebServerAppConfig, WebServerConfigFileRevision), WebServerConfigError> {
    let bytes = read_bounded_config(path)?;
    let sha256 = sha256_hash(&bytes);
    let (config, revision) = deserialize_json_app(path, &bytes, &sha256)?;
    Ok((config, revision))
}

fn deserialize_json_app(
    path: &Path,
    bytes: &[u8],
    sha256: &str,
) -> Result<(WebServerAppConfig, WebServerConfigFileRevision), WebServerConfigError> {
    let instance: Value =
        serde_json::from_slice(bytes).map_err(|source| WebServerConfigError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    validate_schema(&instance)?;

    let config: WebServerAppConfig =
        serde_json::from_value(instance).map_err(|source| WebServerConfigError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    validate_webserver_config(&config)?;
    Ok((
        config,
        WebServerConfigFileRevision::new(sha256.to_owned(), bytes.len() as u64),
    ))
}

/// Compile an already-materialized app model (for example the nginx
/// compatibility mapper output) with semantic and path validation. The JSON
/// Schema check is intentionally skipped: in-memory models are assembled by
/// typed constructors (server.toml / nginx mapping), and the schema's
/// file-oriented `null`/id constraints do not apply to their serialization.
pub fn load_and_compile_webserver_config_json(
    config: &WebServerAppConfig,
) -> Result<CompiledWebServerApp, WebServerConfigError> {
    validate_webserver_config(config)?;
    CompiledWebServerApp::compile(config.clone(), Path::new("/"))
}

fn validate_schema(instance: &Value) -> Result<(), WebServerConfigError> {
    let schema: Value = serde_json::from_str(SCHEMA)
        .map_err(|error| WebServerConfigError::InvalidSchema(error.to_string()))?;
    let validator = jsonschema::draft202012::new(&schema)
        .map_err(|error| WebServerConfigError::InvalidSchema(error.to_string()))?;

    let diagnostics = validator
        .iter_errors(instance)
        .take(MAX_SCHEMA_DIAGNOSTICS)
        .map(|error| {
            ConfigDiagnostic::new(
                error.instance_path().as_str(),
                truncate_diagnostic(&error.to_string()),
            )
        })
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(WebServerConfigError::Validation { diagnostics })
    }
}

fn truncate_diagnostic(message: &str) -> String {
    const MAX_DIAGNOSTIC_BYTES: usize = 512;
    if message.len() <= MAX_DIAGNOSTIC_BYTES {
        return message.to_owned();
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &message[..end])
}
