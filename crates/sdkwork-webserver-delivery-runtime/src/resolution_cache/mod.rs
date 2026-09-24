mod model;
mod state;

use std::{
    panic::AssertUnwindSafe,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use futures_util::FutureExt;
use sdkwork_webserver_contract::provider::{
    ResolveWebsiteStaticPathRequest, ResolveWebsiteWikiRouteRequest, WebsiteContentResolution,
    WebsiteProviderError, WebsiteProviderErrorKind, WebsiteProviderResult,
    WebsiteStaticContentProvider, WebsiteWikiProvider, WebsiteWikiRouteResolution,
};
use tokio::{
    sync::watch,
    time::{timeout, Instant},
};

use crate::{
    WebsiteDeliveryExecutorConfigError, WebsiteProviderEventInvalidation,
    WebsiteProviderEventInvalidator, WebsiteProviderEventScope,
};

use model::{
    contract_mismatch, deadline_exceeded, normalize_origin_result, unavailable, CachedResolution,
    ResolutionOrigin,
};
pub(crate) use model::{ResolutionCacheKey, ResolutionCachePolicy};
use state::{CacheLookup, CacheState, FlightResult, FlightStart};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebsiteProviderResolutionCacheSnapshot {
    pub maximum_entries: usize,
    pub entries: usize,
    pub in_flight: usize,
    pub hits: u64,
    pub stale_hits: u64,
    pub negative_hits: u64,
    pub misses: u64,
    pub writes: u64,
    pub evictions: u64,
    pub coalesced: u64,
    pub bypasses: u64,
    pub revalidations: u64,
    pub invalidations: u64,
}

#[derive(Default)]
struct ResolutionCacheMetrics {
    hits: AtomicU64,
    stale_hits: AtomicU64,
    negative_hits: AtomicU64,
    misses: AtomicU64,
    writes: AtomicU64,
    evictions: AtomicU64,
    coalesced: AtomicU64,
    bypasses: AtomicU64,
    revalidations: AtomicU64,
    invalidations: AtomicU64,
}

pub(crate) struct WebsiteProviderResolutionCache {
    maximum_entries: usize,
    state: Mutex<CacheState>,
    metrics: ResolutionCacheMetrics,
}

impl WebsiteProviderResolutionCache {
    pub(crate) fn new(
        maximum_entries: usize,
        hard_maximum_entries: usize,
    ) -> Result<Self, WebsiteDeliveryExecutorConfigError> {
        if maximum_entries == 0 || maximum_entries > hard_maximum_entries {
            return Err(
                WebsiteDeliveryExecutorConfigError::InvalidProviderResolutionCacheCapacity {
                    configured_entries: maximum_entries,
                    maximum_entries: hard_maximum_entries,
                },
            );
        }
        Ok(Self {
            maximum_entries,
            state: Mutex::new(CacheState::default()),
            metrics: ResolutionCacheMetrics::default(),
        })
    }

    pub(crate) async fn resolve_static(
        self: &Arc<Self>,
        key: ResolutionCacheKey,
        policy: ResolutionCachePolicy,
        provider: Arc<dyn WebsiteStaticContentProvider>,
        request: ResolveWebsiteStaticPathRequest,
        deadline_ms: u64,
    ) -> WebsiteProviderResult<WebsiteContentResolution> {
        let value = self
            .resolve(
                key,
                policy,
                ResolutionOrigin::Static { provider, request },
                deadline_ms,
            )
            .await?;
        match value {
            CachedResolution::Static(resolution) => Ok(resolution),
            CachedResolution::Negative => Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            )),
            CachedResolution::Wiki(_) => Err(contract_mismatch()),
        }
    }

    pub(crate) async fn resolve_wiki(
        self: &Arc<Self>,
        key: ResolutionCacheKey,
        policy: ResolutionCachePolicy,
        provider: Arc<dyn WebsiteWikiProvider>,
        request: ResolveWebsiteWikiRouteRequest,
        deadline_ms: u64,
    ) -> WebsiteProviderResult<WebsiteWikiRouteResolution> {
        let value = self
            .resolve(
                key,
                policy,
                ResolutionOrigin::Wiki { provider, request },
                deadline_ms,
            )
            .await?;
        match value {
            CachedResolution::Wiki(resolution) => Ok(resolution),
            CachedResolution::Negative => Err(WebsiteProviderError::new(
                WebsiteProviderErrorKind::NotFound,
            )),
            CachedResolution::Static(_) => Err(contract_mismatch()),
        }
    }

    async fn resolve(
        self: &Arc<Self>,
        key: ResolutionCacheKey,
        policy: ResolutionCachePolicy,
        origin: ResolutionOrigin,
        deadline_ms: u64,
    ) -> WebsiteProviderResult<CachedResolution> {
        if deadline_ms == 0 {
            return Err(deadline_exceeded());
        }
        let lookup = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .lookup_or_start(&key, self.maximum_entries, Instant::now());
        match lookup {
            CacheLookup::Fresh(value) => {
                if matches!(value, CachedResolution::Negative) {
                    self.metrics.negative_hits.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.metrics.hits.fetch_add(1, Ordering::Relaxed);
                }
                Ok(value)
            }
            CacheLookup::Stale { value, refresh } => {
                self.metrics.stale_hits.fetch_add(1, Ordering::Relaxed);
                if let Some(refresh) = refresh {
                    self.metrics.revalidations.fetch_add(1, Ordering::Relaxed);
                    self.spawn_origin(key, policy, origin.revalidation(), deadline_ms, refresh);
                }
                Ok(value)
            }
            CacheLookup::Wait(receiver) => {
                self.metrics.coalesced.fetch_add(1, Ordering::Relaxed);
                wait_for_flight(receiver, deadline_ms).await
            }
            CacheLookup::Start(flight) => {
                self.metrics.misses.fetch_add(1, Ordering::Relaxed);
                let receiver = flight.receiver.clone();
                self.spawn_origin(key, policy, origin, deadline_ms, flight);
                wait_for_flight(receiver, deadline_ms).await
            }
            CacheLookup::Bypass => {
                self.metrics.bypasses.fetch_add(1, Ordering::Relaxed);
                normalize_origin_result(origin.call(deadline_ms).await)
            }
        }
    }

    fn spawn_origin(
        self: &Arc<Self>,
        key: ResolutionCacheKey,
        policy: ResolutionCachePolicy,
        origin: ResolutionOrigin,
        deadline_ms: u64,
        flight: FlightStart,
    ) {
        let cache = Arc::clone(self);
        tokio::spawn(async move {
            let result = AssertUnwindSafe(origin.call(deadline_ms))
                .catch_unwind()
                .await
                .map(normalize_origin_result)
                .unwrap_or_else(|_| Err(unavailable()));
            cache.complete_flight(key, policy, flight, result).await;
        });
    }

    async fn complete_flight(
        &self,
        key: ResolutionCacheKey,
        policy: ResolutionCachePolicy,
        flight: FlightStart,
        result: FlightResult,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = if state.provider_epoch_matches(&key, flight.provider_epoch) {
            result
        } else {
            Err(unavailable())
        };
        state.remove_flight(&key);
        if let Ok(value) = result.as_ref() {
            let evicted = state.insert(
                key,
                value.clone(),
                policy,
                self.maximum_entries,
                Instant::now(),
            );
            if value.is_positive_cacheable() || matches!(value, CachedResolution::Negative) {
                let ttl_enabled = if matches!(value, CachedResolution::Negative) {
                    !policy.negative_ttl.is_zero()
                } else {
                    !policy.metadata_ttl.is_zero()
                };
                if ttl_enabled {
                    self.metrics.writes.fetch_add(1, Ordering::Relaxed);
                }
            }
            if evicted {
                self.metrics.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
        state.cleanup_provider_epochs();
        drop(state);
        flight.sender.send_replace(Some(result));
    }

    pub(crate) async fn snapshot(&self) -> WebsiteProviderResolutionCacheSnapshot {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        WebsiteProviderResolutionCacheSnapshot {
            maximum_entries: self.maximum_entries,
            entries: state.entry_count(),
            in_flight: state.in_flight_count(),
            hits: self.metrics.hits.load(Ordering::Relaxed),
            stale_hits: self.metrics.stale_hits.load(Ordering::Relaxed),
            negative_hits: self.metrics.negative_hits.load(Ordering::Relaxed),
            misses: self.metrics.misses.load(Ordering::Relaxed),
            writes: self.metrics.writes.load(Ordering::Relaxed),
            evictions: self.metrics.evictions.load(Ordering::Relaxed),
            coalesced: self.metrics.coalesced.load(Ordering::Relaxed),
            bypasses: self.metrics.bypasses.load(Ordering::Relaxed),
            revalidations: self.metrics.revalidations.load(Ordering::Relaxed),
            invalidations: self.metrics.invalidations.load(Ordering::Relaxed),
        }
    }
}

#[async_trait]
impl WebsiteProviderEventInvalidator for WebsiteProviderResolutionCache {
    async fn mark_uncertain(&self, scope: &WebsiteProviderEventScope) -> Result<(), String> {
        let removed = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .mark_uncertain(scope.source);
        self.metrics
            .invalidations
            .fetch_add(removed.max(1) as u64, Ordering::Relaxed);
        Ok(())
    }

    async fn invalidate(
        &self,
        invalidations: &[WebsiteProviderEventInvalidation],
    ) -> Result<(), String> {
        if invalidations.is_empty() {
            return Ok(());
        }
        let removed = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .invalidate(invalidations);
        self.metrics
            .invalidations
            .fetch_add(removed.max(1) as u64, Ordering::Relaxed);
        Ok(())
    }
}

async fn wait_for_flight(
    mut receiver: watch::Receiver<Option<FlightResult>>,
    deadline_ms: u64,
) -> WebsiteProviderResult<CachedResolution> {
    timeout(Duration::from_millis(deadline_ms), async move {
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result;
            }
            receiver.changed().await.map_err(|_| unavailable())?;
        }
    })
    .await
    .map_err(|_| deadline_exceeded())?
}
