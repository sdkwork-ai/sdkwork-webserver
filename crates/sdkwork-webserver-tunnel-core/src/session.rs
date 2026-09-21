//! Session lifecycle (PRD §13).
//!
//! A session is one live agent-to-gateway control relationship. The gateway
//! owns transitions; this module defines the state machine values and the
//! point-in-time snapshot used by managers, APIs, and logs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, SessionId};

/// Lifecycle of a tunnel session (PRD §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Transport established, handshake in flight.
    Connecting,
    /// Awaiting authentication outcome.
    Authenticating,
    /// Authenticated, routes not yet (re)registered.
    Connected,
    /// Authenticated with at least one active route.
    Ready,
    /// Live but impaired (for example heartbeat late).
    Degraded,
    /// Agent-side: retrying the transport after a failure.
    Reconnecting,
    /// Closed by either side or by expiry.
    Disconnected,
    /// Closed deliberately and finalized.
    Closed,
}

impl SessionState {
    /// True when the session can carry data-plane traffic.
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Connected | Self::Ready | Self::Degraded)
    }
}

/// A tunnel session record with its lifecycle timestamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelSession {
    /// Session identity issued by the gateway.
    pub id: SessionId,
    /// Authenticated device behind the session.
    pub device_id: DeviceId,
    /// Current lifecycle state.
    pub state: SessionState,
    /// When the transport was established.
    pub connected_at: DateTime<Utc>,
    /// Last accepted heartbeat instant.
    pub last_heartbeat: DateTime<Utc>,
}

impl TunnelSession {
    /// Builds a session in [`SessionState::Connecting`].
    pub fn connecting(id: SessionId, device_id: DeviceId, now: DateTime<Utc>) -> Self {
        Self {
            id,
            device_id,
            state: SessionState::Connecting,
            connected_at: now,
            last_heartbeat: now,
        }
    }

    /// Snapshot with a new state, preserving identity and timestamps.
    pub fn with_state(&self, state: SessionState) -> Self {
        Self {
            state,
            ..self.clone()
        }
    }
}

/// Read-only projection of a live session for APIs and metrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    /// Session identity.
    pub id: SessionId,
    /// Device identity.
    pub device_id: DeviceId,
    /// Lifecycle state.
    pub state: SessionState,
    /// When the transport was established.
    pub connected_at: DateTime<Utc>,
    /// Last accepted heartbeat instant.
    pub last_heartbeat: DateTime<Utc>,
    /// Routes registered by this session.
    pub route_ids: Vec<String>,
}

impl From<&TunnelSession> for SessionSnapshot {
    fn from(session: &TunnelSession) -> Self {
        Self {
            id: session.id.clone(),
            device_id: session.device_id.clone(),
            state: session.state,
            connected_at: session.connected_at,
            last_heartbeat: session.last_heartbeat,
            route_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TunnelSession {
        let now = Utc::now();
        TunnelSession::connecting(
            SessionId::parse("session_a").expect("valid id"),
            DeviceId::parse("dev_a").expect("valid id"),
            now,
        )
    }

    #[test]
    fn connecting_starts_non_live() {
        let session = sample();
        assert_eq!(session.state, SessionState::Connecting);
        assert!(!session.state.is_live());
    }

    #[test]
    fn ready_state_is_live() {
        assert!(SessionState::Ready.is_live());
        assert!(SessionState::Degraded.is_live());
        assert!(!SessionState::Closed.is_live());
    }

    #[test]
    fn with_state_preserves_identity() {
        let session = sample();
        let ready = session.with_state(SessionState::Ready);
        assert_eq!(ready.id, session.id);
        assert_eq!(ready.device_id, session.device_id);
        assert_eq!(ready.state, SessionState::Ready);
    }

    #[test]
    fn session_serializes_camel_case() {
        let session = sample();
        let json = serde_json::to_string(&session).expect("serialize");
        assert!(json.contains("deviceId"));
        assert!(json.contains("connectedAt"));
    }
}
