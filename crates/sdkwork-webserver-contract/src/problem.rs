//! Web service error model aligned with OpenAPI problem responses.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebServiceErrorKind {
    NotFound,
    Conflict,
    Validation,
    Forbidden,
    DatabaseUnavailable,
    /// A required runtime capability (for example a configured source
    /// importer) is not assembled in this deployment. Maps to HTTP 503.
    Unavailable,
    Internal,
}

/// Deliberately a flat, `Clone` payload enum. The typed upstream error
/// (sqlx, IO, provider) is logged with full `Debug` detail where it occurs
/// (see `store_error` and every repository boundary), then flattened; the
/// route layer masks `Internal`/`DatabaseUnavailable` details so internal
/// error text never reaches clients. A `source()` chain would add no
/// recoverable information — the source is already persisted at the origin
/// — and would leak internals into any downstream `source()` walker, against
/// the redaction contract (SECURITY_SPEC).
#[derive(Debug, thiserror::Error)]
pub enum WebServiceError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("forbidden")]
    Forbidden,
    #[error("database unavailable")]
    DatabaseUnavailable,
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl WebServiceError {
    pub fn kind(&self) -> WebServiceErrorKind {
        match self {
            Self::NotFound(_) => WebServiceErrorKind::NotFound,
            Self::Conflict(_) => WebServiceErrorKind::Conflict,
            Self::Validation(_) => WebServiceErrorKind::Validation,
            Self::Forbidden => WebServiceErrorKind::Forbidden,
            Self::DatabaseUnavailable => WebServiceErrorKind::DatabaseUnavailable,
            Self::Unavailable(_) => WebServiceErrorKind::Unavailable,
            Self::Internal(_) => WebServiceErrorKind::Internal,
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::NotFound(detail.into())
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::Conflict(detail.into())
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self::Validation(detail.into())
    }

    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::Unavailable(detail.into())
    }
}

pub type WebServiceResult<T> = Result<T, WebServiceError>;
