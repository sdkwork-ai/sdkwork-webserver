//! Typed tunnel errors (PRD §50).
//!
//! One `TunnelError` enum per the domain crate; protocol, transport, and
//! composition crates convert their causes into these variants so callers
//! handle remediation instead of string matching.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, TunnelError>;

/// The field a validation failure applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationField {
    /// Identifier-shaped input.
    Id,
    /// Host / domain-shaped input.
    Domain,
    /// Socket-address-shaped input.
    Target,
    /// A configuration value.
    Config,
}

impl fmt::Display for ValidationField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id => f.write_str("id"),
            Self::Domain => f.write_str("domain"),
            Self::Target => f.write_str("target"),
            Self::Config => f.write_str("config"),
        }
    }
}

/// Domain-level tunnel failures. Variants encode remediation (PRD §50) and
/// never carry credentials or tokens in their `Display`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TunnelError {
    /// Input failed domain validation.
    #[error("tunnel validation failed for {field}: {reason}")]
    Validation {
        /// Which input kind failed.
        field: ValidationField,
        /// Why the input was rejected.
        reason: String,
    },
    /// Authentication credentials were missing or wrong.
    #[error("tunnel authentication failed")]
    AuthenticationFailed,
    /// The authenticated identity may not perform the operation.
    #[error("tunnel authorization denied")]
    AuthorizationDenied,
    /// The transport could not be established.
    #[error("tunnel connection failed: {0}")]
    ConnectionFailed(String),
    /// An established transport closed unexpectedly.
    #[error("tunnel connection closed")]
    ConnectionClosed,
    /// A protocol message violated STP framing or semantics.
    #[error("tunnel protocol error: {0}")]
    Protocol(String),
    /// The route definition is not usable.
    #[error("tunnel route is invalid: {0}")]
    InvalidRoute(String),
    /// No route matched the request.
    #[error("tunnel route not found")]
    RouteNotFound,
    /// The route is already registered by another owner.
    #[error("tunnel route conflicts with an existing registration: {0}")]
    RouteConflict(String),
    /// The referenced device is unknown.
    #[error("tunnel device not found")]
    DeviceNotFound,
    /// The referenced session is unknown or no longer live.
    #[error("tunnel session not found")]
    SessionNotFound,
    /// A resource limit rejected the operation (PRD §106).
    #[error("tunnel resource limit reached: {0}")]
    ResourceLimit(&'static str),
    /// The operation exceeded its configured timeout.
    #[error("tunnel operation timed out: {0}")]
    Timeout(&'static str),
    /// The local target behind an agent is unreachable.
    #[error("tunnel local target failed: {0}")]
    TargetUnreachable(String),
}
