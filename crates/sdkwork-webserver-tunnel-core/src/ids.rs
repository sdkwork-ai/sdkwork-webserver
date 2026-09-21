//! Strongly typed tunnel identifiers (PRD §101).
//!
//! Identifiers are validated newtypes rather than bare `String`s so an
//! identity can never be confused with another plane's identity at a
//! function boundary.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{TunnelError, ValidationField};

/// Macro shared by the string-form identifier newtypes: validation,
/// accessors, serde, and the three display traits.
macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident, $prefix:literal, $max:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Prefix required on every identifier of this kind.
            pub const PREFIX: &'static str = $prefix;

            /// Validates and wraps a raw identifier string.
            pub fn parse(raw: impl Into<String>) -> Result<Self, TunnelError> {
                let raw = raw.into();
                if raw.len() > $max {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Id,
                        reason: concat!(
                            stringify!($name),
                            " exceeds the maximum length"
                        )
                        .to_owned(),
                    });
                }
                if !raw.starts_with($prefix) {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Id,
                        reason: concat!(
                            stringify!($name),
                            " must start with `",
                            $prefix,
                            "`"
                        )
                        .to_owned(),
                    });
                }
                let body = &raw[$prefix.len()..];
                if body.is_empty()
                    || !body
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Err(TunnelError::Validation {
                        field: ValidationField::Id,
                        reason: concat!(
                            stringify!($name),
                            " body must be non-empty [A-Za-z0-9_-]"
                        )
                        .to_owned(),
                    });
                }
                Ok(Self(raw))
            }

            /// The validated identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consumes the newtype and returns the identifier text.
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = TunnelError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

string_id!(
    /// Stable identity of a tunnel device (agent installation).
    DeviceId,
    "dev_",
    128
);

string_id!(
    /// Identity of one live agent-to-gateway tunnel session.
    SessionId,
    "session_",
    128
);

string_id!(
    /// Identity of a registered tunnel route.
    RouteId,
    "route_",
    128
);

/// Per-connection data stream sequence number. Stream ids are scoped to one
/// session and only travel on the data plane, so a compact `u64` is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StreamId(u64);

impl StreamId {
    /// First stream id issued on a session.
    pub const FIRST: Self = Self(1);

    /// Wraps a raw counter value.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw counter value.
    pub const fn value(&self) -> u64 {
        self.0
    }

    /// The next stream id, saturating at `u64::MAX`.
    pub const fn next(&self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "stream_{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_accepts_valid_form() {
        let id = DeviceId::parse("dev_home-pc_01").expect("valid device id");
        assert_eq!(id.as_str(), "dev_home-pc_01");
        assert_eq!(id.to_string(), "dev_home-pc_01");
    }

    #[test]
    fn device_id_rejects_missing_prefix() {
        let error = DeviceId::parse("home-pc").expect_err("prefix is required");
        assert!(error.to_string().contains("dev_"));
    }

    #[test]
    fn device_id_rejects_invalid_body() {
        assert!(DeviceId::parse("dev_").is_err());
        assert!(DeviceId::parse("dev_bad id!").is_err());
    }

    #[test]
    fn identifiers_round_trip_through_serde() {
        let route = RouteId::parse("route_web").expect("valid route id");
        let encoded = serde_json::to_string(&route).expect("serialize");
        let decoded: RouteId = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, route);
    }

    #[test]
    fn serde_rejects_invalid_identifier() {
        let result = serde_json::from_str::<SessionId>("\"not-a-session\"");
        assert!(result.is_err());
    }

    #[test]
    fn stream_id_advances_saturating() {
        let id = StreamId::new(u64::MAX);
        assert_eq!(id.next().value(), u64::MAX);
        assert_eq!(StreamId::FIRST.next().value(), 2);
    }
}
