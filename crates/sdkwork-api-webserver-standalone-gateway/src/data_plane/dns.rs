use std::{
    collections::HashSet, future::Future, io, net::SocketAddr, pin::Pin, sync::Arc, time::Duration,
};

use sdkwork_webserver_core::{upstream_ip_is_allowed, ResolverConfig, UpstreamAddressPolicyConfig};
use tokio::{net::lookup_host, sync::Semaphore, time::timeout};

use super::metrics::{DataPlaneMetrics, DnsResult};

type LookupFuture = Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send>>;
type Lookup = dyn Fn(String, usize) -> LookupFuture + Send + Sync;

#[derive(Clone)]
pub(crate) struct BoundedSystemResolver {
    timeout: Duration,
    maximum_answers: usize,
    permits: Arc<Semaphore>,
    lookup: Arc<Lookup>,
}

impl BoundedSystemResolver {
    pub fn implicit() -> Arc<Self> {
        Arc::new(Self::new(2_000, 16, 64))
    }

    pub fn from_config(config: &ResolverConfig) -> Arc<Self> {
        Arc::new(Self::new(
            config.timeout_ms,
            config.maximum_answers,
            config.max_concurrent_queries,
        ))
    }

    fn new(timeout_ms: u64, maximum_answers: usize, maximum_concurrent: usize) -> Self {
        Self::with_lookup(
            timeout_ms,
            maximum_answers,
            maximum_concurrent,
            Arc::new(|host, retained_limit| {
                Box::pin(async move {
                    let addresses = lookup_host((host.as_str(), 0)).await?;
                    Ok(addresses.take(retained_limit).collect())
                })
            }),
        )
    }

    fn with_lookup(
        timeout_ms: u64,
        maximum_answers: usize,
        maximum_concurrent: usize,
        lookup: Arc<Lookup>,
    ) -> Self {
        Self {
            timeout: Duration::from_millis(timeout_ms),
            maximum_answers,
            permits: Arc::new(Semaphore::new(maximum_concurrent)),
            lookup,
        }
    }
}

#[derive(Clone)]
pub(crate) struct GuardedDnsResolver {
    resolver: Arc<BoundedSystemResolver>,
    policy: UpstreamAddressPolicyConfig,
    metrics: Option<Arc<DataPlaneMetrics>>,
}

impl GuardedDnsResolver {
    #[cfg(test)]
    pub fn new(resolver: Arc<BoundedSystemResolver>, policy: UpstreamAddressPolicyConfig) -> Self {
        Self {
            resolver,
            policy,
            metrics: None,
        }
    }

    pub(crate) fn new_observed(
        resolver: Arc<BoundedSystemResolver>,
        policy: UpstreamAddressPolicyConfig,
        metrics: Arc<DataPlaneMetrics>,
    ) -> Self {
        Self {
            resolver,
            policy,
            metrics: Some(metrics),
        }
    }

    pub(crate) async fn resolve_host(&self, host: String) -> io::Result<Vec<SocketAddr>> {
        let _permit = self
            .resolver
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                if let Some(metrics) = &self.metrics {
                    metrics.record_dns_result(DnsResult::Saturated);
                }
                io::Error::new(io::ErrorKind::WouldBlock, "DNS resolver is saturated")
            })?;
        let lease = self
            .metrics
            .as_ref()
            .map(|metrics| metrics.begin_dns_lookup());
        let result = self.resolve_permitted(host).await;
        if let Some(lease) = lease {
            lease.finish(classify_dns_result(&result));
        }
        result
    }

    async fn resolve_permitted(&self, host: String) -> io::Result<Vec<SocketAddr>> {
        let retained_limit = self.resolver.maximum_answers.saturating_add(1);
        let addresses = timeout(
            self.resolver.timeout,
            (self.resolver.lookup)(host, retained_limit),
        )
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "DNS lookup timed out"))??;
        if addresses.len() > self.resolver.maximum_answers {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DNS answer count exceeds the configured maximum",
            ));
        }
        if addresses.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "DNS lookup returned no addresses",
            ));
        }

        let mut unique = HashSet::with_capacity(addresses.len());
        let mut approved = Vec::with_capacity(addresses.len());
        for address in addresses {
            if !upstream_ip_is_allowed(address.ip(), &self.policy.allowed_cidrs) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "DNS answer is forbidden by the upstream address policy",
                ));
            }
            if unique.insert(address.ip()) {
                approved.push(address);
            }
        }
        Ok(approved)
    }
}

fn classify_dns_result(result: &io::Result<Vec<SocketAddr>>) -> DnsResult {
    match result {
        Ok(_) => DnsResult::Success,
        Err(error) => match error.kind() {
            io::ErrorKind::TimedOut => DnsResult::Timeout,
            io::ErrorKind::InvalidData => DnsResult::AnswerLimit,
            io::ErrorKind::NotFound => DnsResult::Empty,
            io::ErrorKind::PermissionDenied => DnsResult::Forbidden,
            _ => DnsResult::IoFailure,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Notify;

    use super::*;
    use crate::{
        data_plane::metrics::{DataPlaneMetrics, DnsResult},
        metric_dimensions::CanonicalMetricDimensions,
    };

    fn policy(values: &[&str]) -> UpstreamAddressPolicyConfig {
        UpstreamAddressPolicyConfig {
            allowed_cidrs: values
                .iter()
                .map(|value| value.parse().expect("valid test CIDR"))
                .collect(),
        }
    }

    fn address(value: &str) -> SocketAddr {
        format!("{value}:0").parse().expect("valid test address")
    }

    #[tokio::test]
    async fn rechecks_each_resolution_and_rejects_rebinding_to_private_space() {
        let calls = Arc::new(AtomicUsize::new(0));
        let lookup_calls = calls.clone();
        let lookup: Arc<Lookup> = Arc::new(move |_, _| {
            let call = lookup_calls.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                Ok(vec![if call == 0 {
                    address("93.184.216.34")
                } else {
                    address("127.0.0.1")
                }])
            })
        });
        let resolver = Arc::new(BoundedSystemResolver::with_lookup(1_000, 4, 1, lookup));
        let guarded = GuardedDnsResolver::new(resolver, policy(&[]));

        assert_eq!(
            guarded
                .resolve_host("example.test".to_owned())
                .await
                .unwrap()
                .len(),
            1
        );
        let error = guarded
            .resolve_host("example.test".to_owned())
            .await
            .expect_err("private rebound answer must fail");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn explicit_loopback_policy_allows_local_resolution_but_mixed_answers_fail() {
        let allowed = GuardedDnsResolver::new(
            Arc::new(BoundedSystemResolver::with_lookup(
                1_000,
                4,
                1,
                Arc::new(|_, _| Box::pin(async { Ok(vec![address("127.0.0.1")]) })),
            )),
            policy(&["127.0.0.0/8"]),
        );
        assert_eq!(
            allowed
                .resolve_host("localhost".to_owned())
                .await
                .unwrap()
                .len(),
            1
        );

        let mixed = GuardedDnsResolver::new(
            Arc::new(BoundedSystemResolver::with_lookup(
                1_000,
                4,
                1,
                Arc::new(|_, _| {
                    Box::pin(async { Ok(vec![address("93.184.216.34"), address("127.0.0.1")]) })
                }),
            )),
            policy(&[]),
        );
        assert_eq!(
            mixed
                .resolve_host("mixed.test".to_owned())
                .await
                .expect_err("mixed forbidden answer set must fail")
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[tokio::test]
    async fn bounds_answers_timeout_and_concurrency_without_waiter_queues() {
        let oversized = GuardedDnsResolver::new(
            Arc::new(BoundedSystemResolver::with_lookup(
                1_000,
                1,
                1,
                Arc::new(|_, _| {
                    Box::pin(async { Ok(vec![address("93.184.216.34"), address("8.8.8.8")]) })
                }),
            )),
            policy(&[]),
        );
        assert_eq!(
            oversized
                .resolve_host("oversized.test".to_owned())
                .await
                .expect_err("oversized answers must fail")
                .kind(),
            io::ErrorKind::InvalidData
        );

        let timed_out = GuardedDnsResolver::new(
            Arc::new(BoundedSystemResolver::with_lookup(
                10,
                1,
                1,
                Arc::new(|_, _| Box::pin(std::future::pending())),
            )),
            policy(&[]),
        );
        assert_eq!(
            timed_out
                .resolve_host("timeout.test".to_owned())
                .await
                .expect_err("lookup must time out")
                .kind(),
            io::ErrorKind::TimedOut
        );

        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let lookup_started = started.clone();
        let lookup_release = release.clone();
        let saturated = GuardedDnsResolver::new(
            Arc::new(BoundedSystemResolver::with_lookup(
                1_000,
                1,
                1,
                Arc::new(move |_, _| {
                    let started = lookup_started.clone();
                    let release = lookup_release.clone();
                    Box::pin(async move {
                        started.notify_one();
                        release.notified().await;
                        Ok(vec![address("93.184.216.34")])
                    })
                }),
            )),
            policy(&[]),
        );
        let first_resolver = saturated.clone();
        let first =
            tokio::spawn(async move { first_resolver.resolve_host("first.test".to_owned()).await });
        started.notified().await;
        assert_eq!(
            saturated
                .resolve_host("second.test".to_owned())
                .await
                .expect_err("saturated lookup must fail immediately")
                .kind(),
            io::ErrorKind::WouldBlock
        );
        release.notify_one();
        first.await.expect("first lookup task joins").unwrap();
    }

    fn observed(
        timeout_ms: u64,
        maximum_answers: usize,
        maximum_concurrent: usize,
        lookup: Arc<Lookup>,
        metrics: Arc<DataPlaneMetrics>,
    ) -> GuardedDnsResolver {
        GuardedDnsResolver::new_observed(
            Arc::new(BoundedSystemResolver::with_lookup(
                timeout_ms,
                maximum_answers,
                maximum_concurrent,
                lookup,
            )),
            policy(&[]),
            metrics,
        )
    }

    #[tokio::test]
    async fn observed_dns_uses_fixed_results_and_cancellation_releases_active_capacity() {
        let metrics = DataPlaneMetrics::new(CanonicalMetricDimensions::default());
        let cases: [(Arc<Lookup>, DnsResult, io::ErrorKind); 5] = [
            (
                Arc::new(|_, _| Box::pin(async { Ok(vec![address("93.184.216.34")]) })),
                DnsResult::Success,
                io::ErrorKind::Other,
            ),
            (
                Arc::new(|_, _| {
                    Box::pin(async { Ok(vec![address("93.184.216.34"), address("8.8.8.8")]) })
                }),
                DnsResult::AnswerLimit,
                io::ErrorKind::InvalidData,
            ),
            (
                Arc::new(|_, _| Box::pin(async { Ok(Vec::new()) })),
                DnsResult::Empty,
                io::ErrorKind::NotFound,
            ),
            (
                Arc::new(|_, _| Box::pin(async { Ok(vec![address("127.0.0.1")]) })),
                DnsResult::Forbidden,
                io::ErrorKind::PermissionDenied,
            ),
            (
                Arc::new(|_, _| {
                    Box::pin(async {
                        Err(io::Error::new(
                            io::ErrorKind::ConnectionRefused,
                            "test resolver failure",
                        ))
                    })
                }),
                DnsResult::IoFailure,
                io::ErrorKind::ConnectionRefused,
            ),
        ];
        for (lookup, expected_result, expected_error) in cases {
            let resolver = observed(1_000, 1, 1, lookup, metrics.clone());
            let result = resolver.resolve_host("fixed.test".to_owned()).await;
            if expected_result == DnsResult::Success {
                assert_eq!(result.expect("successful DNS lookup").len(), 1);
            } else {
                assert_eq!(
                    result.expect_err("fixed DNS failure").kind(),
                    expected_error
                );
            }
            assert_eq!(metrics.dns_active(), 0);
            assert_eq!(metrics.dns_result_count(expected_result), 1);
        }

        let timeout_resolver = observed(
            10,
            1,
            1,
            Arc::new(|_, _| Box::pin(std::future::pending())),
            metrics.clone(),
        );
        assert_eq!(
            timeout_resolver
                .resolve_host("timeout.test".to_owned())
                .await
                .expect_err("DNS timeout")
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(metrics.dns_active(), 0);
        assert_eq!(metrics.dns_result_count(DnsResult::Timeout), 1);

        let started = Arc::new(Notify::new());
        let lookup_started = started.clone();
        let saturated = observed(
            1_000,
            1,
            1,
            Arc::new(move |_, _| {
                let started = lookup_started.clone();
                Box::pin(async move {
                    started.notify_one();
                    std::future::pending().await
                })
            }),
            metrics.clone(),
        );
        let first_resolver = saturated.clone();
        let first = tokio::spawn(async move {
            first_resolver
                .resolve_host("cancelled.test".to_owned())
                .await
        });
        started.notified().await;
        assert_eq!(metrics.dns_active(), 1);
        assert_eq!(
            saturated
                .resolve_host("saturated.test".to_owned())
                .await
                .expect_err("DNS saturation")
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(metrics.dns_result_count(DnsResult::Saturated), 1);
        first.abort();
        first.await.expect_err("cancelled lookup task");
        assert_eq!(metrics.dns_active(), 0);
        assert_eq!(metrics.dns_result_count(DnsResult::Cancelled), 1);
    }
}
