//! Typed errors for the mail-protocol substrate.

use thiserror::Error;

/// Result alias for every mail-protocol codec.
pub type Result<T> = core::result::Result<T, Error>;

/// One protocol codec failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    /// A line exceeded the configured maximum before CRLF.
    #[error("{protocol}: line exceeds {maximum} bytes")]
    LineTooLong {
        /// Lowercase protocol name (`pop3`/`imap`/`smtp`/`mail`).
        protocol: &'static str,
        /// Configured maximum in bytes.
        maximum: usize,
    },
    /// A line or token was not valid UTF-8.
    #[error("{protocol}: {field} is not valid UTF-8")]
    InvalidUtf8 {
        /// Lowercase protocol name.
        protocol: &'static str,
        /// Field being decoded.
        field: &'static str,
    },
    /// A line was syntactically malformed.
    #[error("{protocol}: malformed {field}: {reason}")]
    Malformed {
        /// Lowercase protocol name.
        protocol: &'static str,
        /// Field being decoded.
        field: &'static str,
        /// Static reason, never attacker-controlled bytes.
        reason: &'static str,
    },
    /// A protocol element was unexpected in the current dialog state.
    #[error("{protocol}: unexpected {field}")]
    Unexpected {
        /// Lowercase protocol name.
        protocol: &'static str,
        /// Field being decoded or transitioned.
        field: &'static str,
    },
    /// A buffer or count limit was exceeded.
    #[error("{protocol}: {field} exceeds the {maximum} limit")]
    LimitExceeded {
        /// Lowercase protocol name.
        protocol: &'static str,
        /// Limited field.
        field: &'static str,
        /// Configured maximum.
        maximum: usize,
    },
    /// A dialog transition was attempted from an incompatible state.
    #[error("{protocol}: invalid transition {from} -> {to}")]
    InvalidTransition {
        /// Lowercase protocol name.
        protocol: &'static str,
        /// Current state.
        from: &'static str,
        /// Attempted target state.
        to: &'static str,
    },
    /// A SASL mechanism is not supported by the bounded implementation.
    #[error("{protocol}: unsupported mechanism `{mechanism}`")]
    UnsupportedMechanism {
        /// Lowercase protocol name.
        protocol: &'static str,
        /// Mechanism token (length-bounded by the caller).
        mechanism: String,
    },
    /// A base64 auth line could not be decoded.
    #[error("auth: invalid base64")]
    Base64,
    /// A PLAIN credential blob was not `authzid NUL authcid NUL passwd`.
    #[error("auth: malformed PLAIN credentials")]
    PlainCredentials,
    /// A credential component contained a forbidden byte.
    #[error("auth: credential contains a forbidden byte")]
    CredentialByte,
    /// An IMAP literal declared a size outside the configured bound.
    #[error("imap: literal size {size} exceeds the {maximum} limit")]
    LiteralTooLarge {
        /// Declared literal size.
        size: usize,
        /// Configured maximum.
        maximum: usize,
    },
    /// A dot-stuffed multiline response never reached its `.` terminator.
    #[error("{protocol}: multiline response is not terminated")]
    UnterminatedMultiline {
        /// Lowercase protocol name.
        protocol: &'static str,
    },
    /// Bytes remained after a complete multiline response terminator.
    #[error("{protocol}: trailing bytes after the multiline terminator")]
    TrailingBytes {
        /// Lowercase protocol name.
        protocol: &'static str,
    },
    /// A PROXY protocol v1 header was malformed.
    #[error("proxy-protocol: malformed header: {reason}")]
    ProxyProtocol {
        /// Static reason, never attacker-controlled bytes.
        reason: &'static str,
    },
    /// An `XCLIENT` attribute was invalid.
    #[error("xclient: invalid attribute: {reason}")]
    Xclient {
        /// Static reason, never attacker-controlled bytes.
        reason: &'static str,
    },
}

impl Error {
    /// A malformed-line helper for protocol modules.
    pub(crate) fn malformed(
        protocol: &'static str,
        field: &'static str,
        reason: &'static str,
    ) -> Self {
        Self::Malformed {
            protocol,
            field,
            reason,
        }
    }

    /// An unexpected-element helper for protocol modules.
    pub(crate) fn unexpected(protocol: &'static str, field: &'static str) -> Self {
        Self::Unexpected { protocol, field }
    }

    /// A limit helper for protocol modules.
    pub(crate) fn limit(protocol: &'static str, field: &'static str, maximum: usize) -> Self {
        Self::LimitExceeded {
            protocol,
            field,
            maximum,
        }
    }

    /// An invalid-transition helper for protocol modules.
    pub(crate) fn transition(protocol: &'static str, from: &'static str, to: &'static str) -> Self {
        Self::InvalidTransition { protocol, from, to }
    }
}
