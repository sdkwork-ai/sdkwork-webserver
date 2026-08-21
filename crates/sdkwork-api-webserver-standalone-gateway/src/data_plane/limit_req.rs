//! nginx-compatible `limit_req` token-bucket admission.
//!
//! Delay scheduling for non-`nodelay` excess is not queued: requests within
//! `rate + burst` are admitted immediately (nodelay-equivalent). Requests
//! beyond that window are rejected with 503.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::Instant,
};

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

    pub(super) fn admit(
        &self,
        client_ip: IpAddr,
        rules: &[LimitReqConfig],
    ) -> LimitReqDecision {
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
            // Evict the oldest-ish entry by first key when at capacity.
            if let Some(victim) = self.entries.keys().next().copied() {
                self.entries.remove(&victim);
            }
        }
        let rate = self.rate_per_second;
        let bucket = self.entries.entry(client_ip).or_insert_with(|| Bucket {
            excess: 0.0,
            last: now,
        });
        let elapsed = now
            .saturating_duration_since(bucket.last)
            .as_secs_f64();
        bucket.last = now;
        // Drain excess at the configured rate.
        bucket.excess = (bucket.excess - elapsed * rate).max(0.0);
        if bucket.excess > f64::from(burst) {
            return false;
        }
        bucket.excess += 1.0;
        true
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
}
