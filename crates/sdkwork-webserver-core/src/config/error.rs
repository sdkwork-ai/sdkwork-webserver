use std::{fmt, io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub path: String,
    pub message: String,
}

impl ConfigDiagnostic {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

#[derive(Debug, Error)]
pub enum WebServerConfigError {
    #[error("cannot inspect Web Server config {path}: {source}")]
    Inspect { path: PathBuf, source: io::Error },

    #[error("Web Server config {path} is {actual_bytes} bytes; maximum is {maximum_bytes}")]
    TooLarge {
        path: PathBuf,
        actual_bytes: u64,
        maximum_bytes: u64,
    },

    #[error("cannot read Web Server config {path}: {source}")]
    Read { path: PathBuf, source: io::Error },

    #[error("Web Server config {path} is not valid JSON: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Web Server TOML config {path} is not valid TOML: {source}")]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("Web Server TOML config cannot be materialized: {0}")]
    Materialize(String),

    #[error("embedded Web Server JSON Schema is invalid: {0}")]
    InvalidSchema(String),

    #[error("Web Server config failed validation")]
    Validation { diagnostics: Vec<ConfigDiagnostic> },

    /// A stock nginx directive could not be materialized (fail closed).
    /// The diagnostic's `path` is `file:line` and it is surfaced through
    /// `diagnostics()` like every other source format.
    #[error("{diagnostic}")]
    Nginx { diagnostic: ConfigDiagnostic },
}

impl WebServerConfigError {
    pub fn diagnostics(&self) -> &[ConfigDiagnostic] {
        match self {
            Self::Validation { diagnostics } => diagnostics,
            Self::Nginx { diagnostic } => std::slice::from_ref(diagnostic),
            _ => &[],
        }
    }
}
