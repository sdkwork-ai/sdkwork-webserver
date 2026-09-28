//! nginx-compatible `limit_req` token-bucket admission.
//!
//! Delay scheduling for non-`nodelay` excess is not queued: requests within
//! `rate + burst` are admitted immediately (nodelay-equivalent). Requests
//! beyond that window are rejected with 503.

use std::{collections::HashMap, net::IpAddr, sync::Mutex, time::Instant};

use sdkwork_webserver_core::{LimitReqConfig, LimitReqZoneConfig};

pub(super) struct LimitReqRuntime {
    zones: HashMap<String, Mutex<ZoneState>>,
}

struct ZoneState {
    max_keys: u32,
    rate_per_second: f64,
    entries: HashMap<IpAddr, Bucket>,
}

struct Bucket {
    /// Excess tokens above the sustained rate (can grow up to `burst`).
    excess: f64,
    last: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LimitReqDecision {
    Allow,
    Reject,
}

impl LimitReqRuntime {
    pub(super) fn from_zones(zones: &[LimitReqZoneConfig]) -> Self {
        let mut map = HashMap::new();
        for zone in zones {
            map.insert(
                zone.name.clone(),
                Mutex::new(ZoneState {
                    max_keys: zone.max_keys.max(1),
                    rate_per_second: zone.rate_per_second.max(f64::MIN_POSITIVE),
                    entries: HashMap::new(),
                }),
            );
        }
        Self { zones: map }
    }

    pub(super) fn admit(&self, client_ip: IpAddr, rules: &[LimitReqConfig]) -> LimitReqDecision {
        for rule in rules {
            let Some(zone) = self.zones.get(&rule.zone) else {
                // Semantic validation should reject unknown zones; fail closed.
                return LimitReqDecision::Reject;
            };
            let Ok(mut state) = zone.lock() else {
                return LimitReqDecision::Reject;
            };
            if !state.try_acquire(client_ip, rule.burst) {
                return LimitReqDecision::Reject;
            }
        }
        LimitReqDecision::Allow
    }
}

impl ZoneState {
    fn try_acquire(&mut self, client_ip: IpAddr, burst: u32) -> bool {
        let now = Instant::now();
        if !self.entries.contains_key(&client_ip) && self.entries.len() as u32 >= self.max_keys {
            // The zone is saturated with distinct clients. Idle buckets are
            // reaped before refusing: a bucket idle longer than one burst
            // drain window has fully replenished at the configured rate, so
            // removing it cannot reset any client's tokens — it only clears
            // departed clients. Evicting *active* victims instead would let
            // an IP flood continuously reset legitimate clients' buckets;
            // a zone still full of active clients rejects the new key.
            let idle_limit = self.burst_idle_window(burst);
            self.reap_idle_buckets(now, idle_limit);
            if self.entries.len() as u32 >= self.max_keys {
                return false;
            }
        }
        let rate = self.rate_per_second;
        let bucket = self.entries.entry(client_ip).or_insert_with(|| Bucket {
            excess: 0.0,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.last = now;
        // Drain excess at the configured rate.
        bucket.excess = (bucket.excess - elapsed * rate).max(0.0);
        if bucket.excess > f64::from(burst) {
            return false;
        }
        bucket.excess += 1.0;
        true
    }

    /// How long a bucket must stay idle for its excess to have fully drained
    /// (drain at `rate`, plus a margin so borderline buckets are kept).
    fn burst_idle_window(&self, burst: u32) -> std::time::Duration {
        std::time::Duration::from_secs_f64(f64::from(burst) / self.rate_per_second + 60.0)
    }

    fn reap_idle_buckets(&mut self, now: Instant, idle_limit: std::time::Duration) {
        self.entries
            .retain(|_, bucket| now.saturating_duration_since(bucket.last) < idle_limit);
    }
}

#[cfg(test)]
mod tests {
    use super::{LimitReqDecision, LimitReqRuntime};
    use sdkwork_webserver_core::{LimitReqConfig, LimitReqZoneConfig};
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn rejects_beyond_burst() {
        let runtime = LimitReqRuntime::from_zones(&[LimitReqZoneConfig {
            name: "one".to_owned(),
            key: "$binary_remote_addr".to_owned(),
            max_keys: 16,
            rate_per_second: 1.0,
        }]);
        let rules = [LimitReqConfig {
            zone: "one".to_owned(),
            burst: 1,
            nodelay: true,
        }];
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(runtime.admit(ip, &rules), LimitReqDecision::Allow);
        assert_eq!(runtime.admit(ip, &rules), LimitReqDecision::Allow);
        assert_eq!(runtime.admit(ip, &rules), LimitReqDecision::Reject);
    }

    #[test]
    fn saturated_zone_rejects_new_keys_without_evicting_established_clients() {
        let runtime = LimitReqRuntime::from_zones(&[LimitReqZoneConfig {
            name: "zone".to_owned(),
            key: "$binary_remote_addr".to_owned(),
            max_keys: 2,
            rate_per_second: 1.0,
        }]);
        let rules = [LimitReqConfig {
            zone: "zone".to_owned(),
            burst: 1_000,
            nodelay: true,
        }];
        let first = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let second = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        let flooder = IpAddr::V4(Ipv4Addr::new(10, 9, 9, 9));
        assert_eq!(runtime.admit(first, &rules), LimitReqDecision::Allow);
        assert_eq!(runtime.admit(second, &rules), LimitReqDecision::Allow);

        // The flood cannot push a new key past the capacity: both buckets
        // are fresh, so the idle reap keeps them and the zone stays full.
        assert_eq!(runtime.admit(flooder, &rules), LimitReqDecision::Reject);
        // ...and the flood must not evict the established clients' buckets:
        // both keep their state and remain admissible within burst.
        assert_eq!(runtime.admit(first, &rules), LimitReqDecision::Allow);
        assert_eq!(runtime.admit(second, &rules), LimitReqDecision::Allow);
    }

    #[test]
    fn idle_buckets_are_reaped_so_saturation_does_not_outlive_the_flood() {
        let mut zone = super::ZoneState {
            max_keys: 2,
            rate_per_second: 1.0,
            entries: std::collections::HashMap::new(),
        };
        let now = std::time::Instant::now();
        zone.entries.insert(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            super::Bucket {
                excess: 0.0,
                last: now,
            },
        );
        let departed = now - std::time::Duration::from_secs(3_600);
        zone.entries.insert(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            super::Bucket {
                excess: 0.0,
                last: departed,
            },
        );

        // A short window keeps the active bucket and reaps the departed one.
        zone.reap_idle_buckets(now, std::time::Duration::from_secs(60));
        assert_eq!(zone.entries.len(), 1);
        assert!(zone
            .entries
            .contains_key(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));

        // A window longer than any idleness keeps everything.
        let mut zone = super::ZoneState {
            max_keys: 2,
            rate_per_second: 1.0,
            entries: std::collections::HashMap::new(),
        };
        zone.entries.insert(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            super::Bucket {
                excess: 0.0,
                last: now,
            },
        );
        zone.entries.insert(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            super::Bucket {
                excess: 0.0,
                last: departed,
            },
        );
        zone.reap_idle_buckets(now, std::time::Duration::from_secs(86_400));
        assert_eq!(zone.entries.len(), 2);
    }
}
