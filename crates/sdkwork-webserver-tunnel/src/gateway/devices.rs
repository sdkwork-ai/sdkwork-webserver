//! Gateway device inventory — the "machines" an operator manages.
//!
//! A tunnel device is a machine running a connector. The session table knows
//! the *live* ones and the route registry knows what they registered, but both
//! forget a machine the moment it disconnects: an operator looking at a
//! console needs "which machines do I have, which are online, what do they
//! serve, how much traffic have they moved, and is this one switched off".
//!
//! This registry is that memory. It is deliberately the *only* place that
//! holds machine history (first/last seen, traffic totals, the enable switch)
//! and it never duplicates live state: online-ness, session identity and
//! routes are read from their own owners when a listing is produced, so the
//! admin view can never disagree with the forwarding decisions the gateway is
//! actually making.
//!
//! Bounded by `maxDevices`: an unauthenticated peer cannot make the process
//! remember unbounded machines, and a device that cannot open a session (see
//! [`DeviceRegistry::admit`]) is never recorded at all.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};

use sdkwork_webserver_tunnel_core::{Device, DeviceId, RouteId, TunnelRoute};

/// Why a machine may not open a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAdmission {
    /// The device may connect.
    Admitted,
    /// An operator switched this machine off.
    Disabled,
    /// The inventory is full (`maxDevices`).
    InventoryFull,
}

/// What the gateway remembers about one machine.
#[derive(Debug, Clone)]
pub struct DeviceRecord {
    /// Identity last reported by the machine (name/platform follow the agent).
    pub device: Device,
    /// First time this gateway ever saw the machine.
    pub first_seen: DateTime<Utc>,
    /// Last heartbeat or registration from the machine.
    pub last_seen: DateTime<Utc>,
    /// When the current (or last) session started.
    pub connected_at: Option<DateTime<Utc>>,
    /// When the last session ended, if it did.
    pub disconnected_at: Option<DateTime<Utc>>,
    /// Peer address of the last session.
    pub remote_addr: Option<String>,
    /// Operator switch: a disabled machine is refused at authentication.
    pub enabled: bool,
    /// Bytes relayed from visitors to this machine.
    pub bytes_in: u64,
    /// Bytes relayed from this machine to visitors.
    pub bytes_out: u64,
    /// Sessions this machine has opened since the gateway started.
    pub sessions_total: u64,
}

impl DeviceRecord {
    fn new(device: Device, remote_addr: Option<String>, now: DateTime<Utc>) -> Self {
        Self {
            device,
            first_seen: now,
            last_seen: now,
            connected_at: Some(now),
            disconnected_at: None,
            remote_addr,
            enabled: true,
            bytes_in: 0,
            bytes_out: 0,
            sessions_total: 1,
        }
    }
}

/// Thread-safe machine inventory.
#[derive(Default)]
pub struct DeviceRegistry {
    inner: RwLock<HashMap<DeviceId, DeviceRecord>>,
    /// Machines an operator switched off. Kept apart from the records so a
    /// disabled machine stays disabled even if its record is evicted.
    disabled: RwLock<std::collections::HashSet<DeviceId>>,
    version: AtomicU64,
    /// Largest inventory the gateway will keep (`maxDevices`).
    capacity: RwLock<usize>,
}

impl DeviceRegistry {
    /// An empty inventory with `capacity` slots.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: RwLock::new(capacity.max(1)),
            ..Self::default()
        }
    }

    /// Monotonic version; changes on every mutation an API consumer can see.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// Applies the configured device ceiling (called when limits are known).
    pub fn set_capacity(&self, capacity: usize) {
        let mut slot = self
            .capacity
            .write()
            .expect("device capacity lock is never held across awaits");
        *slot = capacity.max(1);
    }

    /// Decides whether a machine may open a session, before anything is
    /// recorded: a refused peer must not become an inventory entry.
    pub fn admit(&self, device_id: &DeviceId) -> DeviceAdmission {
        if self.is_disabled(device_id) {
            return DeviceAdmission::Disabled;
        }
        let known = self
            .inner
            .read()
            .expect("device registry lock is never held across awaits")
            .contains_key(device_id);
        if known {
            return DeviceAdmission::Admitted;
        }
        let capacity = *self
            .capacity
            .read()
            .expect("device capacity lock is never held across awaits");
        let recorded = self
            .inner
            .read()
            .expect("device registry lock is never held across awaits")
            .len();
        if recorded >= capacity {
            return DeviceAdmission::InventoryFull;
        }
        DeviceAdmission::Admitted
    }

    /// Records a machine connecting (creating its record on first contact).
    pub fn observe_connect(&self, device: &Device, peer: SocketAddr, now: DateTime<Utc>) {
        let mut inner = self
            .inner
            .write()
            .expect("device registry lock is never held across awaits");
        match inner.get_mut(&device.id) {
            Some(record) => {
                // Name and platform come from the agent and may legitimately
                // change (an operator renamed the machine); identity does not.
                record.device = device.clone();
                record.last_seen = now;
                record.connected_at = Some(now);
                record.disconnected_at = None;
                record.remote_addr = Some(peer.to_string());
                record.sessions_total = record.sessions_total.saturating_add(1);
            }
            None => {
                inner.insert(
                    device.id.clone(),
                    DeviceRecord::new(device.clone(), Some(peer.to_string()), now),
                );
            }
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a heartbeat from a live machine.
    pub fn observe_activity(&self, device_id: &DeviceId, now: DateTime<Utc>) {
        let mut inner = self
            .inner
            .write()
            .expect("device registry lock is never held across awaits");
        if let Some(record) = inner.get_mut(device_id) {
            record.last_seen = now;
        }
    }

    /// Records a machine going offline.
    pub fn observe_disconnect(&self, device_id: &DeviceId, now: DateTime<Utc>) {
        let mut inner = self
            .inner
            .write()
            .expect("device registry lock is never held across awaits");
        if let Some(record) = inner.get_mut(device_id) {
            record.disconnected_at = Some(now);
            record.last_seen = now;
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// Adds relayed traffic to a machine's totals.
    pub fn add_traffic(&self, device_id: &DeviceId, bytes_in: u64, bytes_out: u64) {
        let mut inner = self
            .inner
            .write()
            .expect("device registry lock is never held across awaits");
        if let Some(record) = inner.get_mut(device_id) {
            record.bytes_in = record.bytes_in.saturating_add(bytes_in);
            record.bytes_out = record.bytes_out.saturating_add(bytes_out);
        }
    }

    /// True when an operator switched the machine off.
    pub fn is_disabled(&self, device_id: &DeviceId) -> bool {
        self.disabled
            .read()
            .expect("device switch lock is never held across awaits")
            .contains(device_id)
    }

    /// Switches a machine on or off. Returns `false` when the machine is not
    /// in the inventory (an unknown id cannot be pre-emptively disabled: the
    /// inventory is the only source of machine identity).
    pub fn set_enabled(&self, device_id: &DeviceId, enabled: bool) -> bool {
        if !self
            .inner
            .read()
            .expect("device registry lock is never held across awaits")
            .contains_key(device_id)
        {
            return false;
        }
        let mut disabled = self
            .disabled
            .write()
            .expect("device switch lock is never held across awaits");
        if enabled {
            disabled.remove(device_id);
        } else {
            disabled.insert(device_id.clone());
        }
        drop(disabled);
        if let Some(record) = self
            .inner
            .write()
            .expect("device registry lock is never held across awaits")
            .get_mut(device_id)
        {
            record.enabled = enabled;
        }
        self.version.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// One machine's record, with the enable switch applied.
    pub fn get(&self, device_id: &DeviceId) -> Option<DeviceRecord> {
        let mut record = self
            .inner
            .read()
            .expect("device registry lock is never held across awaits")
            .get(device_id)
            .cloned()?;
        record.enabled = !self.is_disabled(device_id);
        Some(record)
    }

    /// Every machine, most recently seen first.
    pub fn list(&self) -> Vec<DeviceRecord> {
        let disabled = self
            .disabled
            .read()
            .expect("device switch lock is never held across awaits")
            .clone();
        let inner = self
            .inner
            .read()
            .expect("device registry lock is never held across awaits");
        let mut records: Vec<DeviceRecord> = inner
            .values()
            .cloned()
            .map(|mut record| {
                record.enabled = !disabled.contains(&record.device.id);
                record
            })
            .collect();
        records.sort_by(|left, right| {
            right
                .last_seen
                .cmp(&left.last_seen)
                .then_with(|| left.device.id.to_string().cmp(&right.device.id.to_string()))
        });
        records
    }

    /// Machine count in the inventory.
    pub fn count(&self) -> usize {
        self.inner
            .read()
            .expect("device registry lock is never held across awaits")
            .len()
    }
}

/// One machine as an operator sees it: inventory plus what it serves now.
#[derive(Debug, Clone)]
pub struct TunnelDeviceView {
    /// Identity last reported by the machine.
    pub device: Device,
    /// Whether a session is live right now.
    pub online: bool,
    /// Live session id, when online.
    pub session_id: Option<String>,
    /// Operator switch state.
    pub enabled: bool,
    /// First/last contact and session boundaries.
    pub first_seen: DateTime<Utc>,
    /// Last heartbeat or registration.
    pub last_seen: DateTime<Utc>,
    /// When the current (or last) session started.
    pub connected_at: Option<DateTime<Utc>>,
    /// When the last session ended.
    pub disconnected_at: Option<DateTime<Utc>>,
    /// Peer address of the last session.
    pub remote_addr: Option<String>,
    /// Bytes relayed toward the machine.
    pub bytes_in: u64,
    /// Bytes relayed from the machine.
    pub bytes_out: u64,
    /// Sessions opened since the gateway started.
    pub sessions_total: u64,
    /// Routes this machine currently serves.
    pub routes: Vec<TunnelRoute>,
    /// Route ids this machine serves, for callers that only need the ids.
    pub route_ids: Vec<RouteId>,
    /// Routes whose last probe reported them down.
    pub unhealthy_route_ids: Vec<RouteId>,
}

impl TunnelDeviceView {
    /// Total routes served.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }

    /// Routes reported healthy.
    pub fn healthy_route_count(&self) -> usize {
        self.routes
            .len()
            .saturating_sub(self.unhealthy_route_ids.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_tunnel_core::{DeviceId, DevicePlatform};

    fn device(id: &str, name: &str) -> Device {
        Device::new(
            DeviceId::parse(id).expect("valid id"),
            name,
            DevicePlatform::Linux,
        )
        .expect("valid device")
    }

    fn peer() -> SocketAddr {
        "203.0.113.7:51000".parse().expect("valid peer")
    }

    #[test]
    fn a_machine_is_remembered_after_it_disconnects() {
        let registry = DeviceRegistry::new(16);
        let machine = device("dev_edge_1", "edge-1");
        assert_eq!(registry.admit(&machine.id), DeviceAdmission::Admitted);
        registry.observe_connect(&machine, peer(), Utc::now());
        assert_eq!(registry.count(), 1);

        let later = Utc::now() + chrono::Duration::seconds(30);
        registry.observe_activity(&machine.id, later);
        registry.observe_disconnect(&machine.id, later);

        let record = registry.get(&machine.id).expect("remembered");
        assert_eq!(record.device.name, "edge-1");
        assert_eq!(record.remote_addr.as_deref(), Some("203.0.113.7:51000"));
        assert_eq!(record.last_seen, later);
        assert!(record.disconnected_at.is_some(), "offline is recorded");
        // A reconnect updates the same record instead of duplicating it.
        registry.observe_connect(&machine, peer(), later);
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.get(&machine.id).expect("record").sessions_total, 2);
    }

    #[test]
    fn a_disabled_machine_is_refused_before_it_is_recorded() {
        let registry = DeviceRegistry::new(16);
        let machine = device("dev_edge_2", "edge-2");
        // An unknown machine cannot be switched off: the inventory is the only
        // source of identity, so a typo cannot pre-emptively block a stranger.
        assert!(!registry.set_enabled(&machine.id, false));

        registry.observe_connect(&machine, peer(), Utc::now());
        assert!(registry.set_enabled(&machine.id, false));
        assert_eq!(registry.admit(&machine.id), DeviceAdmission::Disabled);
        assert!(!registry.get(&machine.id).expect("record").enabled);

        assert!(registry.set_enabled(&machine.id, true));
        assert_eq!(registry.admit(&machine.id), DeviceAdmission::Admitted);
        assert!(registry.get(&machine.id).expect("record").enabled);
    }

    #[test]
    fn the_inventory_is_bounded_and_never_records_a_refused_peer() {
        let registry = DeviceRegistry::new(2);
        registry.observe_connect(&device("dev_a", "a"), peer(), Utc::now());
        registry.observe_connect(&device("dev_b", "b"), peer(), Utc::now());
        // A known machine is always admitted, even at capacity: a reconnect
        // must never be refused because a stranger filled the inventory.
        assert_eq!(
            registry.admit(&DeviceId::parse("dev_a").expect("id")),
            DeviceAdmission::Admitted
        );
        let stranger = DeviceId::parse("dev_c").expect("id");
        assert_eq!(registry.admit(&stranger), DeviceAdmission::InventoryFull);
        assert_eq!(registry.count(), 2, "a refused peer is not recorded");
    }

    #[test]
    fn traffic_totals_accumulate_per_machine() {
        let registry = DeviceRegistry::new(4);
        let machine = device("dev_edge_3", "edge-3");
        registry.observe_connect(&machine, peer(), Utc::now());
        registry.add_traffic(&machine.id, 1_000, 2_000);
        registry.add_traffic(&machine.id, 500, 250);
        let record = registry.get(&machine.id).expect("record");
        assert_eq!(record.bytes_in, 1_500);
        assert_eq!(record.bytes_out, 2_250);
        // Traffic for a machine the gateway never saw is dropped, not invented.
        registry.add_traffic(&DeviceId::parse("dev_ghost").expect("id"), 10, 10);
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn records_are_listed_most_recently_seen_first() {
        let registry = DeviceRegistry::new(8);
        let now = Utc::now();
        registry.observe_connect(&device("dev_old", "old"), peer(), now);
        registry.observe_connect(
            &device("dev_new", "new"),
            peer(),
            now + chrono::Duration::seconds(10),
        );
        let listed: Vec<String> = registry
            .list()
            .into_iter()
            .map(|record| record.device.name)
            .collect();
        assert_eq!(listed, vec!["new".to_owned(), "old".to_owned()]);
        // A heartbeat moves a machine back to the top: the listing answers
        // "what is alive right now", not "what was created last".
        registry.observe_activity(
            &DeviceId::parse("dev_old").expect("id"),
            now + chrono::Duration::seconds(20),
        );
        let listed: Vec<String> = registry
            .list()
            .into_iter()
            .map(|record| record.device.name)
            .collect();
        assert_eq!(listed, vec!["old".to_owned(), "new".to_owned()]);
    }

    #[test]
    fn a_device_view_counts_healthy_routes() {
        let view = TunnelDeviceView {
            device: device("dev_x", "x"),
            online: true,
            session_id: Some("sess_x".to_owned()),
            enabled: true,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            connected_at: Some(Utc::now()),
            disconnected_at: None,
            remote_addr: None,
            bytes_in: 0,
            bytes_out: 0,
            sessions_total: 1,
            routes: Vec::new(),
            route_ids: Vec::new(),
            unhealthy_route_ids: Vec::new(),
        };
        assert_eq!(view.route_count(), 0);
        assert_eq!(view.healthy_route_count(), 0);
    }
}
