use thiserror::Error;

use crate::ConfigDiagnostic;

#[derive(Debug, Error)]
pub enum TlsRuntimeSnapshotError {
    #[error("TLS runtime snapshot is {actual_bytes} bytes; maximum is {maximum_bytes}")]
    TooLarge {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
    #[error("TLS runtime snapshot is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("embedded TLS runtime snapshot JSON Schema is invalid: {0}")]
    InvalidSchema(String),
    #[error("TLS runtime snapshot hash mismatch: expected {expected}, calculated {calculated}")]
    HashMismatch {
        expected: String,
        calculated: String,
    },
    #[error("TLS runtime snapshot failed validation")]
    Validation { diagnostics: Vec<ConfigDiagnostic> },
}

impl TlsRuntimeSnapshotError {
    pub fn diagnostics(&self) -> &[ConfigDiagnostic] {
        match self {
            Self::Validation { diagnostics } => diagnostics,
            _ => &[],
        }
    }
}
