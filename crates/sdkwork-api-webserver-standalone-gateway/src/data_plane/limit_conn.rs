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
    /// Used as the `Err` sentinel that releases a tracked connection without
    /// rejecting it; the condition is matched but never constructed.
    #[allow(dead_code)]
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

    /// Admit a request against every configured rule (nginx checks each
    /// `limit_conn` directive). Returns a lease that the caller must keep
    /// alive for the duration of the response (it releases the slot on
    /// drop). Slots acquired by earlier rules are rolled back when a later
    /// rule rejects; a no-op lease is returned when no rule applies.
    pub(super) fn admit(
        &self,
        client_ip: IpAddr,
        rules: &[LimitConnConfig],
    ) -> Result<ConnectionLease, LimitConnDecision> {
        // A location may carry several `limit_conn` directives (one per zone).
        // One slot is taken in every zone, so the lease must remember every
        // zone it touched: releasing only the first one leaked the remaining
        // slots permanently and turned a multi-rule location into a 503 after
        // `maxConnections` requests for that key.
        let mut acquired: Vec<Arc<Mutex<ZoneState>>> = Vec::with_capacity(rules.len());
        for rule in rules {
            let Some(zone) = self.zones.get(&rule.zone) else {
                // Semantic validation should reject unknown zones; fail closed.
                release_all(&acquired, client_ip);
                return Err(LimitConnDecision::Reject);
            };
            let Ok(mut state) = zone.lock() else {
                release_all(&acquired, client_ip);
                return Err(LimitConnDecision::Reject);
            };
            let admitted = state.try_acquire(client_ip, rule.max_connections);
            // Drop the guard before any rollback: a rule list that repeats the
            // same zone would otherwise try to re-lock the mutex this thread
            // still holds and deadlock the request.
            drop(state);
            if !admitted {
                release_all(&acquired, client_ip);
                return Err(LimitConnDecision::Reject);
            }
            acquired.push(Arc::clone(zone));
        }
        Ok(ConnectionLease {
            zones: acquired,
            client_ip,
        })
    }

    /// Admit one **stream** connection against its `limit_conn` rules.
    ///
    /// nginx's stream module checks the rules at accept time and closes an
    /// over-limit connection silently. `dry_run` (nginx `limit_conn_dry_run on`)
    /// keeps counting the connection without denying it; the returned flag reports
    /// that a rule *would* have rejected it.
    pub(super) fn stream_admit(
        &self,
        client_ip: IpAddr,
        rules: &[LimitConnConfig],
        dry_run: bool,
    ) -> Result<(ConnectionLease, bool), LimitConnDecision> {
        let mut acquired: Vec<Arc<Mutex<ZoneState>>> = Vec::with_capacity(rules.len());
        let mut would_reject = false;
        for rule in rules {
            let Some(zone) = self.zones.get(&rule.zone) else {
                release_all(&acquired, client_ip);
                return Err(LimitConnDecision::Reject);
            };
            let Ok(mut state) = zone.lock() else {
                release_all(&acquired, client_ip);
                return Err(LimitConnDecision::Reject);
            };
            if dry_run {
                // Dry run counts every connection (even an over-limit one) but
                // never rejects; an untracked key past the zone capacity has no
                // slot to release.
                let (tracked, over_limit) =
                    state.try_acquire_stream(client_ip, rule.max_connections);
                drop(state);
                would_reject |= over_limit;
                if tracked {
                    acquired.push(Arc::clone(zone));
                }
                continue;
            }
            let admitted = state.try_acquire(client_ip, rule.max_connections);
            drop(state);
            if !admitted {
                release_all(&acquired, client_ip);
                return Err(LimitConnDecision::Reject);
            }
            acquired.push(Arc::clone(zone));
        }
        Ok((
            ConnectionLease {
                zones: acquired,
                client_ip,
            },
            would_reject,
        ))
    }
}

/// Roll back every slot a rejected admission already took.
fn release_all(zones: &[Arc<Mutex<ZoneState>>], client_ip: IpAddr) {
    for zone in zones {
        if let Ok(mut state) = zone.lock() {
            state.release_slot(client_ip);
        }
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

    /// Acquire one stream-connection slot, reporting `(tracked, over_limit)`.
    ///
    /// A dry-run admission still counts an over-limit connection (the zone
    /// reflects real concurrency), but never allocates a new key past the
    /// zone's `max_keys` capacity: an untracked connection has no slot to
    /// release.
    fn try_acquire_stream(&mut self, client_ip: IpAddr, max_connections: u32) -> (bool, bool) {
        if let Some(count) = self.entries.get_mut(&client_ip) {
            let over_limit = *count >= max_connections;
            *count = count.saturating_add(1);
            return (true, over_limit);
        }
        if self.entries.len() as u32 >= self.max_keys {
            return (false, true);
        }
        self.entries.insert(client_ip, 1);
        (true, max_connections == 0)
    }

    fn release_slot(&mut self, client_ip: IpAddr) {
        if let Some(count) = self.entries.get_mut(&client_ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.entries.remove(&client_ip);
            }
        }
    }
}

/// Releases its connection slots on drop — one counter per zone the request
/// was admitted against. An empty `zones` vector is the no-op lease used when
/// no rule matched, so the response body wrapper stays simple.
pub(super) struct ConnectionLease {
    zones: Vec<Arc<Mutex<ZoneState>>>,
    client_ip: IpAddr,
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        for zone in &self.zones {
            if let Ok(mut state) = zone.lock() {
                state.release_slot(self.client_ip);
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

    fn named_zone(name: &str) -> LimitConnZoneConfig {
        LimitConnZoneConfig {
            name: name.to_owned(),
            key: "$binary_remote_addr".to_owned(),
            max_keys: 16,
        }
    }

    /// Current counted connections for one key in one zone.
    fn rule(zone: &str, max_connections: u32) -> super::LimitConnConfig {
        super::LimitConnConfig {
            zone: zone.to_owned(),
            max_connections,
        }
    }

    fn counted(runtime: &LimitConnRuntime, zone_name: &str, ip: IpAddr) -> u32 {
        runtime
            .zones
            .get(zone_name)
            .and_then(|zone| zone.lock().ok())
            .and_then(|state| state.entries.get(&ip).copied())
            .unwrap_or(0)
    }

    #[test]
    fn stream_admission_denies_over_limit_and_counts_until_release() {
        let runtime = LimitConnRuntime::from_zones(&[named_zone("perip")]);
        let ip = "127.0.0.1".parse().unwrap();
        let rules = vec![rule("perip", 1)];
        let (lease, dry_run_reject) = runtime
            .stream_admit(ip, &rules, false)
            .expect("first connection admitted");
        assert!(!dry_run_reject);
        assert_eq!(counted(&runtime, "perip", ip), 1);
        assert!(matches!(
            runtime.stream_admit(ip, &rules, false),
            Err(super::LimitConnDecision::Reject)
        ));
        drop(lease);
        assert_eq!(counted(&runtime, "perip", ip), 0);
        runtime
            .stream_admit(ip, &rules, false)
            .expect("slot released");
    }

    #[test]
    fn stream_dry_run_counts_but_never_denies() {
        let runtime = LimitConnRuntime::from_zones(&[named_zone("perip")]);
        let ip = "127.0.0.1".parse().unwrap();
        let rules = vec![rule("perip", 1)];
        let (first, rejected) = runtime.stream_admit(ip, &rules, true).expect("first");
        assert!(!rejected);
        let (second, rejected) = runtime
            .stream_admit(ip, &rules, true)
            .expect("dry run allows");
        assert!(rejected, "dry run reports the would-be rejection");
        assert_eq!(counted(&runtime, "perip", ip), 2, "both are counted");
        drop(first);
        drop(second);
        assert_eq!(counted(&runtime, "perip", ip), 0);
    }

    #[test]
    fn stream_admission_rejects_an_unknown_zone() {
        let runtime = LimitConnRuntime::from_zones(&[]);
        let ip = "127.0.0.1".parse().unwrap();
        assert!(matches!(
            runtime.stream_admit(ip, &[rule("missing", 1)], false),
            Err(super::LimitConnDecision::Reject)
        ));
        // Dry run cannot track an unknown zone either; fail closed instead of
        // silently admitting.
        assert!(matches!(
            runtime.stream_admit(ip, &[rule("missing", 1)], true),
            Err(super::LimitConnDecision::Reject)
        ));
    }

    #[test]
    fn dropping_a_lease_releases_every_admitted_zone() {
        // A location may carry more than one `limit_conn` directive. Both
        // slots must be returned when the response body completes; releasing
        // only the first zone leaked the rest forever.
        let runtime = LimitConnRuntime::from_zones(&[named_zone("perip"), named_zone("perserver")]);
        let rules = [
            LimitConnConfig {
                zone: "perip".to_owned(),
                max_connections: 1,
            },
            LimitConnConfig {
                zone: "perserver".to_owned(),
                max_connections: 1,
            },
        ];
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let lease = runtime.admit(ip, &rules).expect("multi-zone admit");
        assert_eq!(counted(&runtime, "perip", ip), 1);
        assert_eq!(counted(&runtime, "perserver", ip), 1);
        drop(lease);
        assert_eq!(counted(&runtime, "perip", ip), 0);
        assert_eq!(
            counted(&runtime, "perserver", ip),
            0,
            "the second zone's slot must be released too"
        );
        // Both zones admit again after the release.
        runtime.admit(ip, &rules).expect("both zones free again");
    }

    #[test]
    fn a_later_rule_rejection_rolls_back_every_taken_slot() {
        let runtime = LimitConnRuntime::from_zones(&[
            LimitConnZoneConfig {
                name: "tight".to_owned(),
                key: "$binary_remote_addr".to_owned(),
                max_keys: 16,
            },
            LimitConnZoneConfig {
                name: "blocked".to_owned(),
                key: "$binary_remote_addr".to_owned(),
                max_keys: 0,
            },
        ]);
        let ip = IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8));
        let rules = [
            LimitConnConfig {
                zone: "tight".to_owned(),
                max_connections: 8,
            },
            LimitConnConfig {
                zone: "blocked".to_owned(),
                max_connections: 8,
            },
        ];
        // `max_keys == 0` is clamped to 1, so the first key is admitted; a
        // second client is rejected by the zone-capacity rule and must not
        // leave the first zone's permit behind.
        let first = runtime
            .admit(ip, &rules)
            .expect("first key fills the blocked zone");
        let second_ip = IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9));
        assert!(matches!(
            runtime.admit(second_ip, &rules),
            Err(LimitConnDecision::Reject)
        ));
        assert_eq!(
            counted(&runtime, "tight", second_ip),
            0,
            "a rejected later rule must roll back the earlier zone"
        );
        drop(first);
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
