//! nginx-compatible `limit_conn` per-key connection admission.
//!
//! Each `limit_conn_zone` tracks per-client concurrent connection counts.
//! `admit` returns a lease that releases the slot when dropped; the handler
//! attaches the lease to the response body so a connection stays counted
//! for as long as its response streams. Requests beyond the zone's tracked
//! key capacity or a key's `maxConnections` are rejected with 503.

use std::{
    collections::HashMap,
    net::IpAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use http_body::{Frame, SizeHint};
use sdkwork_webserver_core::{LimitConnConfig, LimitConnZoneConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LimitConnDecision {
    Allow,
    Reject,
}

pub(super) struct LimitConnRuntime {
    zones: HashMap<String, Arc<Mutex<ZoneState>>>,
}

struct ZoneState {
    max_keys: u32,
    entries: HashMap<IpAddr, u32>,
}

impl LimitConnRuntime {
    pub(super) fn from_zones(zones: &[LimitConnZoneConfig]) -> Self {
        let mut map = HashMap::new();
        for zone in zones {
            map.insert(
                zone.name.clone(),
                Arc::new(Mutex::new(ZoneState {
                    max_keys: zone.max_keys.max(1),
                    entries: HashMap::new(),
                })),
            );
        }
        Self { zones: map }
    }

    /// Admit a request against the first matching rule. Returns a lease
    /// that the caller must keep alive for the duration of the response
    /// (it releases the slot on drop). A no-op lease is returned when no
    /// rule applies.
    pub(super) fn admit(
        &self,
        client_ip: IpAddr,
        rules: &[LimitConnConfig],
    ) -> Result<ConnectionLease, LimitConnDecision> {
        for rule in rules {
            let Some(zone) = self.zones.get(&rule.zone) else {
                // Semantic validation should reject unknown zones; fail closed.
                return Err(LimitConnDecision::Reject);
            };
            let Ok(mut state) = zone.lock() else {
                return Err(LimitConnDecision::Reject);
            };
            if !state.try_acquire(client_ip, rule.max_connections) {
                return Err(LimitConnDecision::Reject);
            }
            return Ok(ConnectionLease {
                zone: Some(zone.clone()),
                client_ip,
            });
        }
        Ok(ConnectionLease {
            zone: None,
            client_ip,
        })
    }
}

impl ZoneState {
    fn try_acquire(&mut self, client_ip: IpAddr, max_connections: u32) -> bool {
        if let Some(count) = self.entries.get_mut(&client_ip) {
            if *count >= max_connections {
                return false;
            }
            *count += 1;
            return true;
        }
        if self.entries.len() as u32 >= self.max_keys {
            // Zone at tracked-key capacity with a new key: reject, matching
            // nginx's shared-zone exhaustion behavior.
            return false;
        }
        self.entries.insert(client_ip, 1);
        true
    }
}

/// Releases its connection slot on drop. `zone == None` is the no-op lease
/// used when no rule matched, so the response body wrapper stays simple.
pub(super) struct ConnectionLease {
    zone: Option<Arc<Mutex<ZoneState>>>,
    client_ip: IpAddr,
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let Some(zone) = &self.zone else {
            return;
        };
        let Ok(mut state) = zone.lock() else {
            return;
        };
        if let Some(count) = state.entries.get_mut(&self.client_ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.entries.remove(&self.client_ip);
            }
        }
    }
}

/// Response body wrapper that holds a `ConnectionLease` for the full
/// lifetime of the body stream: the connection stays counted until the
/// response completes or is abandoned.
pub(super) struct LeaseBody {
    pub(super) inner: axum::body::Body,
    /// Drop guard: the lease field is never read, only dropped with the
    /// body (releasing the limit_conn slot).
    #[allow(dead_code)]
    pub(super) lease: ConnectionLease,
}

impl http_body::Body for LeaseBody {
    type Data = bytes::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.inner).poll_frame(context)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::{LimitConnDecision, LimitConnRuntime};
    use sdkwork_webserver_core::{LimitConnConfig, LimitConnZoneConfig};
    use std::net::{IpAddr, Ipv4Addr};

    fn zone() -> LimitConnZoneConfig {
        LimitConnZoneConfig {
            name: "perip".to_owned(),
            key: "$binary_remote_addr".to_owned(),
            max_keys: 16,
        }
    }

    #[test]
    fn admits_up_to_max_connections_and_releases_on_drop() {
        let runtime = LimitConnRuntime::from_zones(&[zone()]);
        let rules = [LimitConnConfig {
            zone: "perip".to_owned(),
            max_connections: 2,
        }];
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let first = runtime.admit(ip, &rules).expect("first");
        let second = runtime.admit(ip, &rules).expect("second");
        assert!(matches!(
            runtime.admit(ip, &rules),
            Err(LimitConnDecision::Reject)
        ));
        drop(first);
        // One slot freed; the next request is admitted again.
        assert!(runtime.admit(ip, &rules).is_ok());
        drop(second);
        assert!(runtime.admit(ip, &rules).is_ok());
    }

    #[test]
    fn different_clients_do_not_share_budgets() {
        let runtime = LimitConnRuntime::from_zones(&[zone()]);
        let rules = [LimitConnConfig {
            zone: "perip".to_owned(),
            max_connections: 1,
        }];
        let first_ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let second_ip = IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8));
        let first_lease = runtime.admit(first_ip, &rules).expect("first client");
        let second_lease = runtime.admit(second_ip, &rules).expect("second client");
        assert!(matches!(
            runtime.admit(first_ip, &rules),
            Err(LimitConnDecision::Reject)
        ));
        drop(first_lease);
        drop(second_lease);
    }

    #[test]
    fn unknown_zone_fails_closed() {
        let runtime = LimitConnRuntime::from_zones(&[zone()]);
        let rules = [LimitConnConfig {
            zone: "missing".to_owned(),
            max_connections: 1,
        }];
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        assert!(matches!(
            runtime.admit(ip, &rules),
            Err(LimitConnDecision::Reject)
        ));
    }

    #[test]
    fn no_rules_produce_a_noop_lease() {
        let runtime = LimitConnRuntime::from_zones(&[zone()]);
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        assert!(runtime.admit(ip, &[]).is_ok());
    }

    #[test]
    fn zone_key_capacity_is_enforced() {
        let runtime = LimitConnRuntime::from_zones(&[LimitConnZoneConfig {
            name: "tiny".to_owned(),
            key: "$binary_remote_addr".to_owned(),
            max_keys: 1,
        }]);
        let rules = [LimitConnConfig {
            zone: "tiny".to_owned(),
            max_connections: 8,
        }];
        let first_ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let second_ip = IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8));
        let first_lease = runtime.admit(first_ip, &rules).expect("first client");
        assert!(matches!(
            runtime.admit(second_ip, &rules),
            Err(LimitConnDecision::Reject)
        ));
        drop(first_lease);
    }
}
