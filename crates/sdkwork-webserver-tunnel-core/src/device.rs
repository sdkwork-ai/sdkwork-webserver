//! Device identity (PRD §12, §26).
//!
//! A device is the stable identity of one agent installation. Secrets are
//! never part of the domain model: bearer tokens stay in the agent runtime
//! and are verified by the gateway authenticator without being stored here.

use serde::{Deserialize, Serialize};

use crate::error::{Result, TunnelError, ValidationField};
use crate::ids::DeviceId;

/// Operating environment a device reports during the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePlatform {
    /// Windows desktop or server.
    Windows,
    /// Linux host or container.
    Linux,
    /// macOS host.
    Macos,
    /// Anything else, including unknown hosts.
    Other,
}

impl DevicePlatform {
    /// Parses the platform label agents send in `Hello`.
    pub fn parse(raw: &str) -> Self {
        match raw.to_ascii_lowercase().as_str() {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" | "darwin" => Self::Macos,
            _ => Self::Other,
        }
    }

    /// Canonical label used on the wire and in logs.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Other => "other",
        }
    }
}

/// A tunnel device: identity plus descriptive metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Stable device identity.
    pub id: DeviceId,
    /// Human-readable device name (for operators, never used for matching).
    pub name: String,
    /// Reported operating environment.
    pub platform: DevicePlatform,
}

impl Device {
    /// Validates and builds a device record.
    pub fn new(id: DeviceId, name: impl Into<String>, platform: DevicePlatform) -> Result<Self> {
        let name = name.into();
        if name.is_empty() || name.len() > 128 {
            return Err(TunnelError::Validation {
                field: ValidationField::Config,
                reason: "device name must be 1..=128 characters".to_owned(),
            });
        }
        Ok(Self { id, name, platform })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_parses_common_labels() {
        assert_eq!(DevicePlatform::parse("Windows"), DevicePlatform::Windows);
        assert_eq!(DevicePlatform::parse("darwin"), DevicePlatform::Macos);
        assert_eq!(DevicePlatform::parse("solaris"), DevicePlatform::Other);
    }

    #[test]
    fn device_rejects_empty_name() {
        let id = DeviceId::parse("dev_x").expect("valid id");
        assert!(Device::new(id, "", DevicePlatform::Linux).is_err());
    }

    #[test]
    fn device_serializes_without_secrets() {
        let id = DeviceId::parse("dev_x").expect("valid id");
        let device = Device::new(id, "home-pc", DevicePlatform::Windows).expect("valid device");
        let json = serde_json::to_string(&device).expect("serialize");
        assert!(json.contains("home-pc"));
        assert!(!json.contains("token"));
    }
}
