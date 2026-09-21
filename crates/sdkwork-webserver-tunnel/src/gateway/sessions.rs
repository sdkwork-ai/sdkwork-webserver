//! Gateway session table (PRD §57).
//!
//! Tracks live agent sessions: identity, the transport connection used to
//! open data streams, heartbeat freshness, and the per-session stream
//! admission budget. Locks are held only for map access.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use tokio::sync::Semaphore;

use sdkwork_webserver_tunnel_core::{
    Device, DeviceId, Result, SessionId, SessionSnapshot, SessionState, TunnelError,
};
use sdkwork_webserver_tunnel_transport::TunnelConnection;

use crate::metrics::TunnelMetrics;

/// One live session entry.
pub struct SessionEntry {
    /// Authenticated device behind the session.
    pub device: Device,
    /// Transport used to open data streams toward this agent.
    pub connection: Arc<dyn TunnelConnection>,
    /// Instant of the last accepted heartbeat (monotonic clock).
    pub last_heartbeat: Instant,
    /// Session admission instant (monotonic clock).
    pub connected_at: Instant,
    /// Stream admission budget (PRD §106 `maxStreamsPerSession`).
    pub stream_budget: Arc<Semaphore>,
}

/// Thread-safe table of live sessions.
pub struct SessionTable {
    inner: RwLock<TableInner>,
    stream_budget: u32,
    version: AtomicU64,
}

#[derive(Default)]
struct TableInner {
    by_id: HashMap<SessionId, SessionEntry>,
    by_device: HashMap<DeviceId, SessionId>,
}

impl SessionTable {
    /// Builds an empty table granting `stream_budget` concurrent streams per
    /// session.
    pub fn new(stream_budget: u32) -> Self {
        Self {
            inner: RwLock::new(TableInner::default()),
            stream_budget: stream_budget.max(1),
            version: AtomicU64::new(0),
        }
    }

    /// Monotonic table version; changes on every mutation.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// Registers a session, closing and removing any previous session of the
    /// same device (reconnect replacement). Returns the replaced session id,
    /// when one existed.
    pub fn insert(
        &self,
        session_id: SessionId,
        device: Device,
        connection: Arc<dyn TunnelConnection>,
        metrics: &TunnelMetrics,
    ) -> Option<SessionId> {
        let mut inner = self
            .inner
            .write()
            .expect("session table lock is never held across awaits");
        let replaced = inner.by_device.remove(&device.id);
        if let Some(old_id) = &replaced {
            if let Some(old) = inner.by_id.remove(old_id) {
                old.connection.close("superseded by a new session");
                metrics.record_session_close();
                self.version.fetch_add(1, Ordering::Relaxed);
            }
        }
        inner
            .by_device
            .insert(device.id.clone(), session_id.clone());
        inner.by_id.insert(
            session_id,
            SessionEntry {
                device,
                connection,
                last_heartbeat: Instant::now(),
                connected_at: Instant::now(),
                stream_budget: Arc::new(Semaphore::new(
                    usize::try_from(self.stream_budget).unwrap_or(usize::MAX),
                )),
            },
        );
        self.version.fetch_add(1, Ordering::Relaxed);
        replaced
    }

    /// Looks up a live session's connection.
    pub fn connection(&self, session_id: &SessionId) -> Option<Arc<dyn TunnelConnection>> {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        inner
            .by_id
            .get(session_id)
            .map(|entry| entry.connection.clone())
    }

    /// Looks up a session id by device id.
    pub fn session_of_device(&self, device_id: &DeviceId) -> Option<SessionId> {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        inner.by_device.get(device_id).cloned()
    }

    /// Refreshes the heartbeat instant of a live session.
    pub fn touch(&self, session_id: &SessionId) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .expect("session table lock is never held across awaits");
        let entry = inner
            .by_id
            .get_mut(session_id)
            .ok_or(TunnelError::SessionNotFound)?;
        entry.last_heartbeat = Instant::now();
        Ok(())
    }

    /// Acquires one stream admission permit for the session (bounded,
    /// non-blocking; PRD §106/§109).
    pub fn try_acquire_stream(
        &self,
        session_id: &SessionId,
    ) -> Result<tokio::sync::OwnedSemaphorePermit> {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        let entry = inner
            .by_id
            .get(session_id)
            .ok_or(TunnelError::SessionNotFound)?;
        entry
            .stream_budget
            .clone()
            .try_acquire_owned()
            .map_err(|_| TunnelError::ResourceLimit("maxStreamsPerSession"))
    }

    /// Removes a session and returns its entry.
    pub fn remove(&self, session_id: &SessionId, metrics: &TunnelMetrics) -> Option<SessionEntry> {
        let mut inner = self
            .inner
            .write()
            .expect("session table lock is never held across awaits");
        let removed = inner.by_id.remove(session_id);
        if let Some(entry) = &removed {
            if inner.by_device.get(&entry.device.id) == Some(session_id) {
                inner.by_device.remove(&entry.device.id);
            }
            self.version.fetch_add(1, Ordering::Relaxed);
            metrics.record_session_close();
        }
        removed
    }

    /// Removes every session whose heartbeat is older than `idle`; returns
    /// the ids of the expired sessions (PRD §28 heartbeat timeout).
    pub fn expire_idle(
        &self,
        idle: std::time::Duration,
        metrics: &TunnelMetrics,
    ) -> Vec<SessionId> {
        let mut inner = self
            .inner
            .write()
            .expect("session table lock is never held across awaits");
        let now = Instant::now();
        let expired: Vec<SessionId> = inner
            .by_id
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_heartbeat) > idle)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &expired {
            if let Some(entry) = inner.by_id.remove(id) {
                entry.connection.close("heartbeat timeout");
                if inner.by_device.get(&entry.device.id) == Some(id) {
                    inner.by_device.remove(&entry.device.id);
                }
                metrics.record_session_close();
            }
        }
        if !expired.is_empty() {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
        expired
    }

    /// Distinct connected device count (PRD §106 `maxDevices` input).
    pub fn device_count(&self) -> usize {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        inner.by_device.len()
    }

    /// Live session count.
    pub fn session_count(&self) -> usize {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        inner.by_id.len()
    }

    /// Snapshot of every live session for APIs (PRD §82 status surface).
    pub fn snapshots(
        &self,
        route_ids_by_session: &HashMap<SessionId, Vec<String>>,
    ) -> Vec<SessionSnapshot> {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        let mut snapshots: Vec<SessionSnapshot> = inner
            .by_id
            .iter()
            .map(|(id, entry)| {
                let connected_at = epoch_utc(entry.connected_at);
                SessionSnapshot {
                    id: id.clone(),
                    device_id: entry.device.id.clone(),
                    state: SessionState::Ready,
                    connected_at,
                    last_heartbeat: epoch_utc(entry.last_heartbeat),
                    route_ids: route_ids_by_session.get(id).cloned().unwrap_or_default(),
                }
            })
            .collect();
        snapshots.sort_by(|left, right| left.id.cmp(&right.id));
        snapshots
    }

    /// Distinct device ids currently connected.
    pub fn device_ids(&self) -> Vec<DeviceId> {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        let mut ids: Vec<DeviceId> = inner.by_device.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// True when the device is currently connected.
    pub fn is_connected(&self, device_id: &DeviceId) -> bool {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        inner.by_device.contains_key(device_id)
    }

    /// Every session id (sorted).
    pub fn ids(&self) -> Vec<SessionId> {
        let inner = self
            .inner
            .read()
            .expect("session table lock is never held across awaits");
        let mut ids: Vec<SessionId> = inner.by_id.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// True when at least `limit` devices would be exceeded by one more
    /// (PRD §106).
    pub fn device_budget_exhausted(&self, limit: u32) -> bool {
        usize::try_from(limit)
            .map(|limit| self.device_count() >= limit)
            .unwrap_or(true)
    }

    /// Device ids currently holding sessions, as a set.
    pub fn device_set(&self) -> HashSet<DeviceId> {
        self.device_ids().into_iter().collect()
    }
}

fn epoch_utc(instant: Instant) -> chrono::DateTime<chrono::Utc> {
    // `Instant` has no wall-clock anchor; sessions expose their monotonic
    // age converted onto a "now-anchored" timeline for API output.
    let now = chrono::Utc::now();
    let age = instant.elapsed();
    now - chrono::Duration::from_std(age).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_tunnel_core::DevicePlatform;
    use sdkwork_webserver_tunnel_transport::TunnelStream;
    use std::sync::atomic::AtomicBool;

    struct StubConnection {
        closed: AtomicBool,
    }

    impl StubConnection {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                closed: AtomicBool::new(false),
            })
        }
    }

    #[async_trait::async_trait]
    impl TunnelConnection for StubConnection {
        async fn open_stream(&self) -> Result<Box<dyn TunnelStream>> {
            Err(TunnelError::ConnectionClosed)
        }

        async fn accept_stream(&self) -> Result<Box<dyn TunnelStream>> {
            Err(TunnelError::ConnectionClosed)
        }

        fn close(&self, _reason: &str) {
            self.closed.store(true, Ordering::Relaxed);
        }

        async fn closed(&self) -> Result<()> {
            Err(TunnelError::ConnectionClosed)
        }

        fn remote_addr(&self) -> Result<std::net::SocketAddr> {
            Err(TunnelError::ConnectionFailed("stub".to_owned()))
        }
    }

    fn device(id: &str) -> Device {
        Device::new(
            DeviceId::parse(id).expect("valid id"),
            "test",
            DevicePlatform::Linux,
        )
        .expect("valid device")
    }

    #[test]
    fn insert_replaces_previous_device_session() {
        let metrics = TunnelMetrics::new();
        let table = SessionTable::new(8);
        let first = SessionId::parse("session_1").expect("valid id");
        let second = SessionId::parse("session_2").expect("valid id");
        table.insert(
            first.clone(),
            device("dev_a"),
            StubConnection::new(),
            &metrics,
        );
        table.insert(
            second.clone(),
            device("dev_a"),
            StubConnection::new(),
            &metrics,
        );
        assert_eq!(table.session_count(), 1);
        assert_eq!(
            table.session_of_device(&DeviceId::parse("dev_a").expect("valid id")),
            Some(second)
        );
        assert!(!table.ids().contains(&first));
    }

    #[test]
    fn heartbeat_touch_and_expiry() {
        let metrics = TunnelMetrics::new();
        let table = SessionTable::new(8);
        let id = SessionId::parse("session_1").expect("valid id");
        table.insert(id.clone(), device("dev_a"), StubConnection::new(), &metrics);
        assert!(table.touch(&id).is_ok());
        assert!(table
            .expire_idle(std::time::Duration::from_secs(60), &metrics)
            .is_empty());
        assert_eq!(table.session_count(), 1);
    }

    #[test]
    fn stream_budget_is_bounded() {
        let metrics = TunnelMetrics::new();
        let table = SessionTable::new(1);
        let id = SessionId::parse("session_1").expect("valid id");
        table.insert(id.clone(), device("dev_a"), StubConnection::new(), &metrics);
        let first = table.try_acquire_stream(&id);
        assert!(first.is_ok());
        let second = table.try_acquire_stream(&id);
        assert!(matches!(second, Err(TunnelError::ResourceLimit(_))));
        drop(first);
        assert!(table.try_acquire_stream(&id).is_ok());
    }
}
