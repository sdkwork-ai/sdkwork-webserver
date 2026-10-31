//! Single typed error surface shared by every codec in the crate.
//!
//! Errors carry the protocol name plus a structural description (field, limit,
//! reason); they never embed request bodies, credentials or upstream payload
//! bytes, so an error can be logged or surfaced at the edge without redaction.

use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// A codec failure. Every variant is recoverable by the caller: the edge maps
/// it to an upstream failure (502/504 class) or refuses the configuration.
#[derive(Debug, Error)]
pub enum Error {
    /// The input ended before a complete frame/field could be read.
    #[error(
        "{protocol}: truncated input while reading {field} ({expected} more byte(s) required)"
    )]
    Truncated {
        /// Protocol that detected the truncation.
        protocol: &'static str,
        /// Structural field that was being read.
        field: &'static str,
        /// Number of additional bytes the decoder needed.
        expected: usize,
    },

    /// A frame, header block or value exceeded its configured hard bound.
    #[error("{protocol}: {field} is {actual} bytes, which exceeds the {maximum}-byte limit")]
    LimitExceeded {
        /// Protocol that detected the violation.
        protocol: &'static str,
        /// Field or structure whose bound was exceeded.
        field: &'static str,
        /// Observed size or count.
        actual: usize,
        /// Configured maximum.
        maximum: usize,
    },

    /// The bytes do not form a legal frame for the protocol.
    #[error("{protocol}: invalid framing: {reason}")]
    Framing {
        /// Protocol that detected the violation.
        protocol: &'static str,
        /// Human-readable, value-free reason.
        reason: String,
    },

    /// The frame is well formed but violates the protocol state machine.
    #[error("{protocol}: protocol violation: {reason}")]
    Protocol {
        /// Protocol that detected the violation.
        protocol: &'static str,
        /// Human-readable, value-free reason.
        reason: String,
    },

    /// A textual field was not valid UTF-8 (protocol metadata is text).
    #[error("{protocol}: {field} is not valid UTF-8")]
    InvalidUtf8 {
        /// Protocol that detected the violation.
        protocol: &'static str,
        /// Field carrying the invalid text.
        field: &'static str,
    },

    /// An upstream status value is outside the HTTP status range.
    #[error("{protocol}: invalid response status {status:?}")]
    InvalidStatus {
        /// Protocol that produced the status.
        protocol: &'static str,
        /// The rejected status value.
        status: Option<u16>,
    },

    /// The FastCGI request-id allocator wrapped past its 16-bit space.
    #[error("fastcgi: request id space exhausted; open a new upstream connection")]
    RequestIdExhausted,
}

impl Error {
    /// Build a [`Error::Framing`] for `protocol`.
    pub(crate) fn framing(protocol: &'static str, reason: impl Into<String>) -> Self {
        Self::Framing {
            protocol,
            reason: reason.into(),
        }
    }

    /// Build a [`Error::Protocol`] for `protocol`.
    pub(crate) fn protocol(protocol: &'static str, reason: impl Into<String>) -> Self {
        Self::Protocol {
            protocol,
            reason: reason.into(),
        }
    }

    /// Build a [`Error::LimitExceeded`].
    pub(crate) fn limit(
        protocol: &'static str,
        field: &'static str,
        actual: usize,
        maximum: usize,
    ) -> Self {
        Self::LimitExceeded {
            protocol,
            field,
            actual,
            maximum,
        }
    }
}
