//! The topology refresher: the auto-discovery loop.
//!
//! Pulls from the [`DiscoverySource`] on a bounded interval, builds a fresh
//! [`ClusterTopology`], and swaps it into an [`ArcSwap`] handle readers use
//! for routing. A failed refresh keeps the previous snapshot (stale-but-
//! usable beats empty), and consecutive failures are logged with a warning
//! so operators see a discovery outage.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;

use super::discovery::SharedDiscoverySource;
use super::topology::ClusterTopology;

/// A shared, atomically swappable topology handle.
pub type SharedTopology = Arc<ArcSwap<ClusterTopology>>;

/// Creates an empty topology handle (auto-routing disabled until the first
/// successful discovery).
pub fn empty_topology() -> SharedTopology {
    Arc::new(ArcSwap::from_pointee(ClusterTopology::default()))
}

/// Runs one discovery pass and swaps the snapshot in. Returns `true` when a
/// snapshot was published.
pub async fn refresh_once(
    source: &SharedDiscoverySource,
    handle: &SharedTopology,
    cluster_code: &str,
) -> bool {
    if let Some(discovered) = source.discover(cluster_code).await {
        let snapshot = ClusterTopology::build(
            discovered.cluster_code,
            now_millis(),
            discovered.strategy,
            discovered.members,
        );
        handle.store(Arc::new(snapshot));
        true
    } else {
        false
    }
}

/// The discovery loop. Stops when `stop` fires; logs consecutive failures.
pub async fn run(
    source: SharedDiscoverySource,
    handle: SharedTopology,
    cluster_code: String,
    interval: Duration,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval.max(Duration::from_secs(1)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut consecutive_failures = 0_u32;
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => break,
            _ = ticker.tick() => {
                if refresh_once(&source, &handle, &cluster_code).await {
                    consecutive_failures = 0;
                } else {
                    consecutive_failures += 1;
                    if consecutive_failures == 1 || consecutive_failures % 10 == 0 {
                        tracing::warn!(
                            cluster = %cluster_code,
                            failures = consecutive_failures,
                            "cluster discovery refresh failing"
                        );
                    }
                }
            }
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::{DiscoveredCluster, StaticDiscoverySource};
    use crate::instance::DiscoveredInstance;
    use crate::strategy::LoadBalancingStrategy;
    use crate::topology::ServiceMembership;

    fn discovered() -> DiscoveredCluster {
        DiscoveredCluster {
            cluster_code: "edge".to_owned(),
            strategy: LoadBalancingStrategy::RoundRobin,
            members: vec![ServiceMembership {
                instance: Arc::new(DiscoveredInstance::new("node-1", "https://node-1:443")),
                served_domains: vec!["svc.cluster.test".to_owned()],
                strategy: None,
            }],
        }
    }

    #[tokio::test]
    async fn refresh_once_publishes_snapshot() {
        let source: SharedDiscoverySource = Arc::new(StaticDiscoverySource::always(discovered()));
        let handle = empty_topology();
        assert!(handle.load().groups.is_empty());
        assert!(refresh_once(&source, &handle, "edge").await);
        let snapshot = handle.load();
        assert_eq!(snapshot.cluster_code, "edge");
        assert!(snapshot.group_for_host("svc.cluster.test").is_some());
    }

    #[tokio::test]
    async fn failed_refresh_keeps_previous_snapshot() {
        let handle = empty_topology();
        {
            let source: SharedDiscoverySource =
                Arc::new(StaticDiscoverySource::always(discovered()));
            refresh_once(&source, &handle, "edge").await;
        }
        // A source that discovers nothing no longer wipes the snapshot.
        let stale: SharedDiscoverySource = Arc::new(StaticDiscoverySource::never());
        assert!(!refresh_once(&stale, &handle, "edge").await);
        assert!(handle.load().group_for_host("svc.cluster.test").is_some());
    }

    #[tokio::test]
    async fn refresher_loop_stops_on_signal() {
        let source: SharedDiscoverySource = Arc::new(StaticDiscoverySource::always(discovered()));
        let handle = empty_topology();
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let loop_task = tokio::spawn(run(
            source,
            Arc::clone(&handle),
            "edge".to_owned(),
            Duration::from_millis(20),
            stop_rx,
        ));
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(handle.load().group_for_host("svc.cluster.test").is_some());
        let _ = stop_tx.send(true);
        tokio::time::timeout(Duration::from_secs(2), loop_task)
            .await
            .expect("loop stops")
            .expect("loop task");
    }
}
