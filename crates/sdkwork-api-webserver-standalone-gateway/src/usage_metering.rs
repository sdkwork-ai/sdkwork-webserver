//! SaaS traffic usage metering.
//!
//! Every served request is counted per domain (hostname) and per server IP
//! (`transport_peer`), attributed to the serving tenant/app when known, and
//! aggregated into fixed windows. Windows are flushed to the
//! sdkwork-deployments billing tables through the configured channel
//! (`embedded`: shared Deploy database; `http`: control-plane ingest
//! endpoint). Facts are deduplicated on a deterministic window key so retried
//! or overlapping flushes never double-bill.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use sdkwork_webserver_core::config::UsageMeteringConfig;

pub const USAGE_DIMENSION_REQUESTS: &str = "traffic.requests";
pub const USAGE_DIMENSION_INGRESS_BYTES: &str = "traffic.ingress_bytes";
pub const USAGE_DIMENSION_EGRESS_BYTES: &str = "traffic.egress_bytes";

/// Traffic attribution known at the data plane. `None` fields mean the
/// request was not attributable (locally configured host without Deploy
/// metadata); the control plane resolves tenant/app/binding from the
/// binding uuid when possible.
#[derive(Clone, Debug, Default)]
pub struct UsageAttribution {
    pub tenant_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub app_uuid: Option<String>,
    pub binding_uuid: Option<String>,
    pub app_id: Option<String>,
    pub app_slug: Option<String>,
}

/// Metering context threaded into the website delivery path: the local
/// server address (per-server-IP dimension) plus the aggregator.
#[derive(Clone)]
pub struct MeteringContext {
    pub server_ip: IpAddr,
    pub server_port: u16,
    pub listener_id: String,
    pub meter: Arc<UsageMeteringAggregator>,
    /// Count 404 (NotFound) outcomes in the delivery layer. False when an
    /// app-domain fallback resolver exists: the fallback path records the
    /// final outcome itself, so the intermediate 404 must not be counted.
    pub count_not_found: bool,
}

/// One counted request delivered to the aggregator.
pub struct MeteredRequest<'a> {
    pub hostname: &'a str,
    pub server_ip: IpAddr,
    pub server_port: u16,
    pub listener_id: &'a str,
    pub attribution: &'a UsageAttribution,
    pub ingress_bytes: u64,
    pub egress_bytes: u64,
    pub status_class: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BucketKey {
    hostname: String,
    server_ip: String,
    server_port: u16,
    listener_id: String,
    tenant_id: i64,
    organization_id: i64,
    app_uuid: String,
    binding_uuid: String,
    app_id: String,
    status_class: String,
    window_start_epoch: u64,
}

#[derive(Clone, Debug, Default)]
struct Counters {
    requests: u64,
    ingress_bytes: u64,
    egress_bytes: u64,
}

/// One aggregated window ready for ingest.
#[derive(Clone, Debug)]
pub struct UsageWindow {
    /// Node that observed the traffic (dedup scope: two nodes serving the
    /// same host in the same window must not deduplicate each other).
    pub node_uuid: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub app_uuid: Option<String>,
    pub binding_uuid: Option<String>,
    pub hostname: String,
    pub server_ip: String,
    pub server_port: u16,
    pub listener_id: String,
    pub app_id: Option<String>,
    pub app_slug: Option<String>,
    pub status_class: String,
    pub window_start: String,
    pub requests: u64,
    pub ingress_bytes: u64,
    pub egress_bytes: u64,
}

impl UsageWindow {
    fn deduplication_key(&self, dimension: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.node_uuid.as_bytes());
        hasher.update(b"|");
        hasher.update(self.window_start.as_bytes());
        hasher.update(b"|");
        hasher.update(self.tenant_id.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(self.app_uuid.as_deref().unwrap_or_default().as_bytes());
        hasher.update(b"|");
        hasher.update(self.binding_uuid.as_deref().unwrap_or_default().as_bytes());
        hasher.update(b"|");
        hasher.update(self.hostname.as_bytes());
        hasher.update(b"|");
        hasher.update(self.server_ip.as_bytes());
        hasher.update(b"|");
        hasher.update(self.server_port.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(self.listener_id.as_bytes());
        hasher.update(b"|");
        hasher.update(self.status_class.as_bytes());
        hasher.update(b"|");
        hasher.update(dimension.as_bytes());
        let digest = hasher.finalize();
        let fingerprint = digest
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("traffic:{}:{}:{fingerprint}", self.window_start, dimension)
    }
}

/// Ingest channel implemented by the shared Deploy database adapter
/// (`EmbeddedUsageIngestChannel`) or the control-plane HTTP client
/// (`HttpUsageIngestChannel`).
#[async_trait]
pub trait UsageIngestChannel: Send + Sync {
    async fn ingest(&self, node_uuid: &str, windows: Vec<UsageWindow>) -> Result<(), String>;
}

/// In-memory window aggregator. `record` is lock-free-ish (one mutex per
/// aggregator; buckets are few); the flush task drains windows and ingests
/// them through the channel, re-merging on failure so facts are never lost
/// to transient ingest errors.
pub struct UsageMeteringAggregator {
    config: Arc<UsageMeteringConfig>,
    channel: Arc<dyn UsageIngestChannel>,
    node_uuid: String,
    buckets: std::sync::Mutex<HashMap<BucketKey, Counters>>,
    ingested_windows: AtomicU64,
    dropped_windows: AtomicU64,
}

impl UsageMeteringAggregator {
    pub fn new(
        config: Arc<UsageMeteringConfig>,
        channel: Arc<dyn UsageIngestChannel>,
        node_uuid: impl Into<String>,
    ) -> Self {
        Self {
            config,
            channel,
            node_uuid: node_uuid.into(),
            buckets: std::sync::Mutex::new(HashMap::new()),
            ingested_windows: AtomicU64::new(0),
            dropped_windows: AtomicU64::new(0),
        }
    }

    pub fn config(&self) -> &UsageMeteringConfig {
        &self.config
    }

    pub fn node_uuid(&self) -> &str {
        &self.node_uuid
    }

    pub fn ingested_windows(&self) -> u64 {
        self.ingested_windows.load(Ordering::Relaxed)
    }

    pub fn dropped_windows(&self) -> u64 {
        self.dropped_windows.load(Ordering::Relaxed)
    }

    /// Count one served request into its window bucket.
    pub fn record(&self, request: MeteredRequest<'_>) {
        if !self.config.enabled {
            return;
        }
        let window_start_epoch =
            Self::window_start_epoch(SystemTime::now(), self.config.window_seconds.max(1));
        let key = BucketKey {
            hostname: request.hostname.trim().to_ascii_lowercase(),
            server_ip: request.server_ip.to_string(),
            server_port: request.server_port,
            listener_id: request.listener_id.to_owned(),
            tenant_id: request.attribution.tenant_id.unwrap_or(0),
            organization_id: request.attribution.organization_id.unwrap_or(0),
            app_uuid: request.attribution.app_uuid.clone().unwrap_or_default(),
            binding_uuid: request.attribution.binding_uuid.clone().unwrap_or_default(),
            app_id: request.attribution.app_id.clone().unwrap_or_default(),
            status_class: request.status_class.to_owned(),
            window_start_epoch,
        };
        let Ok(mut buckets) = self.buckets.lock() else {
            tracing::warn!("usage metering bucket lock poisoned; dropping one request fact");
            return;
        };
        let counters = buckets.entry(key).or_default();
        counters.requests = counters.requests.saturating_add(1);
        counters.ingress_bytes = counters.ingress_bytes.saturating_add(request.ingress_bytes);
        counters.egress_bytes = counters.egress_bytes.saturating_add(request.egress_bytes);
    }

    fn window_start_epoch(now: SystemTime, window_seconds: u64) -> u64 {
        let seconds = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        seconds / window_seconds * window_seconds
    }

    /// Drain all current windows and ingest them. Failed ingests are
    /// re-merged into the buckets so the next flush retries them.
    pub async fn flush(&self) {
        if !self.config.enabled {
            return;
        }
        let window_seconds = self.config.window_seconds.max(1);
        let now_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Only windows that have fully closed are flushed: a mid-window flush
        // would split one window into two events with the same deduplication
        // key, and the second one would be dropped as a duplicate (data
        // loss). Open windows stay bucketed for the next flush.
        let drained = {
            let Ok(mut buckets) = self.buckets.lock() else {
                tracing::warn!("usage metering bucket lock poisoned; skipping flush");
                return;
            };
            let mut drained = HashMap::new();
            let mut kept = HashMap::new();
            for (key, counters) in std::mem::take(&mut *buckets) {
                if key.window_start_epoch + window_seconds <= now_epoch {
                    drained.insert(key, counters);
                } else {
                    kept.insert(key, counters);
                }
            }
            *buckets = kept;
            drained
        };
        if drained.is_empty() {
            return;
        }
        let mut windows = Vec::with_capacity(drained.len());
        for (key, counters) in drained.iter() {
            let window_start = epoch_to_rfc3339(key.window_start_epoch);
            windows.push(UsageWindow {
                node_uuid: self.node_uuid.clone(),
                tenant_id: key.tenant_id,
                organization_id: key.organization_id,
                app_uuid: (!key.app_uuid.is_empty()).then_some(key.app_uuid.clone()),
                binding_uuid: (!key.binding_uuid.is_empty()).then_some(key.binding_uuid.clone()),
                hostname: key.hostname.clone(),
                server_ip: key.server_ip.clone(),
                server_port: key.server_port,
                listener_id: key.listener_id.clone(),
                app_id: (!key.app_id.is_empty()).then_some(key.app_id.clone()),
                app_slug: None,
                status_class: key.status_class.clone(),
                window_start,
                requests: counters.requests,
                ingress_bytes: counters.ingress_bytes,
                egress_bytes: counters.egress_bytes,
            });
        }
        match self.channel.ingest(&self.node_uuid, windows).await {
            Ok(()) => {
                self.ingested_windows
                    .fetch_add(drained.len() as u64, Ordering::Relaxed);
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    windows = drained.len(),
                    "usage metering ingest failed; windows re-queued"
                );
                self.dropped_windows
                    .fetch_add(drained.len() as u64, Ordering::Relaxed);
                let Ok(mut buckets) = self.buckets.lock() else {
                    return;
                };
                for (key, counters) in drained {
                    let entry = buckets.entry(key).or_default();
                    entry.requests = entry.requests.saturating_add(counters.requests);
                    entry.ingress_bytes =
                        entry.ingress_bytes.saturating_add(counters.ingress_bytes);
                    entry.egress_bytes = entry.egress_bytes.saturating_add(counters.egress_bytes);
                }
            }
        }
    }

    /// Spawn the background flush loop. The task holds a `Weak` reference
    /// and exits once the aggregator is dropped (data plane shutdown).
    pub fn spawn_flush(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let interval = Duration::from_millis(self.config.flush_interval_ms.max(1_000));
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let Some(aggregator) = weak.upgrade() else {
                    return;
                };
                aggregator.flush().await;
            }
        });
    }
}

fn epoch_to_rfc3339(epoch_seconds: u64) -> String {
    let seconds = i64::try_from(epoch_seconds).unwrap_or(0);
    let datetime = time::OffsetDateTime::from_unix_timestamp(seconds)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    datetime
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

/// Embedded channel: writes windows into the shared Deploy database through
/// the Deploy repository (management feature).
#[cfg(feature = "management")]
pub struct EmbeddedUsageIngestChannel {
    repository: sdkwork_api_webserver_assembly::DeployRepository,
}

#[cfg(feature = "management")]
impl EmbeddedUsageIngestChannel {
    pub fn new(repository: sdkwork_api_webserver_assembly::DeployRepository) -> Self {
        Self { repository }
    }
}

#[cfg(feature = "management")]
#[async_trait]
impl UsageIngestChannel for EmbeddedUsageIngestChannel {
    async fn ingest(&self, _node_uuid: &str, windows: Vec<UsageWindow>) -> Result<(), String> {
        use sdkwork_deploy_contract::{
            UsageEventAttribution, UsageEventIngestItem, USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES,
            USAGE_DIMENSION_TRAFFIC_REQUESTS,
        };
        let mut events = Vec::with_capacity(windows.len() * 3);
        let observed_at = epoch_to_rfc3339(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        for window in &windows {
            let attribution = UsageEventAttribution {
                hostname: Some(window.hostname.clone()),
                server_ip: Some(window.server_ip.clone()),
                server_port: Some(window.server_port),
                listener_id: Some(window.listener_id.clone()),
                app_id: window.app_id.clone(),
                app_slug: window.app_slug.clone(),
                app_uuid: window.app_uuid.clone(),
                binding_uuid: window.binding_uuid.clone(),
                status_class: (!window.status_class.is_empty())
                    .then_some(window.status_class.clone()),
            };
            events.push(UsageEventIngestItem {
                tenant_id: window.tenant_id,
                organization_id: window.organization_id,
                app_uuid: window.app_uuid.clone(),
                binding_uuid: window.binding_uuid.clone(),
                period_start: window.window_start.clone(),
                dimension: USAGE_DIMENSION_TRAFFIC_REQUESTS.to_owned(),
                quantity: i64::try_from(window.requests).unwrap_or(i64::MAX),
                unit: "REQUEST".to_owned(),
                deduplication_key: window.deduplication_key(USAGE_DIMENSION_TRAFFIC_REQUESTS),
                attribution: attribution.clone(),
                observed_at: observed_at.clone(),
            });
            if window.ingress_bytes > 0 {
                events.push(UsageEventIngestItem {
                    tenant_id: window.tenant_id,
                    organization_id: window.organization_id,
                    app_uuid: window.app_uuid.clone(),
                    binding_uuid: window.binding_uuid.clone(),
                    period_start: window.window_start.clone(),
                    dimension: USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES.to_owned(),
                    quantity: i64::try_from(window.ingress_bytes).unwrap_or(i64::MAX),
                    unit: "BYTE".to_owned(),
                    deduplication_key: window
                        .deduplication_key(USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES),
                    attribution: attribution.clone(),
                    observed_at: observed_at.clone(),
                });
            }
            if window.egress_bytes > 0 {
                events.push(UsageEventIngestItem {
                    tenant_id: window.tenant_id,
                    organization_id: window.organization_id,
                    app_uuid: window.app_uuid.clone(),
                    binding_uuid: window.binding_uuid.clone(),
                    period_start: window.window_start.clone(),
                    dimension: sdkwork_deploy_contract::USAGE_DIMENSION_TRAFFIC_EGRESS_BYTES
                        .to_owned(),
                    quantity: i64::try_from(window.egress_bytes).unwrap_or(i64::MAX),
                    unit: "BYTE".to_owned(),
                    deduplication_key: window.deduplication_key(
                        sdkwork_deploy_contract::USAGE_DIMENSION_TRAFFIC_EGRESS_BYTES,
                    ),
                    attribution,
                    observed_at: observed_at.clone(),
                });
            }
        }
        let result = self
            .repository
            .ingest_usage_events_lookup(&events)
            .await
            .map_err(|error| format!("deploy usage ingest failed: {error}"))?;
        tracing::debug!(
            ingested = result.ingested,
            duplicates = result.duplicates,
            rejected = result.rejected,
            "usage metering windows ingested"
        );
        Ok(())
    }
}

/// HTTP channel: posts windows to the Deploy control-plane ingest endpoint
/// (`POST /backend/v3/api/usage/ingest`).
pub struct HttpUsageIngestChannel {
    endpoint: String,
    auth_token: Option<String>,
    client: reqwest::Client,
}

impl HttpUsageIngestChannel {
    pub fn new(endpoint: String, auth_token: Option<String>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| format!("usage ingest http client failed: {error}"))?;
        Ok(Self {
            endpoint,
            auth_token,
            client,
        })
    }
}

#[async_trait]
impl UsageIngestChannel for HttpUsageIngestChannel {
    async fn ingest(&self, node_uuid: &str, windows: Vec<UsageWindow>) -> Result<(), String> {
        use sdkwork_deploy_contract::{
            IngestUsageEventsRequest, UsageEventAttribution, UsageEventIngestItem,
            USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES, USAGE_DIMENSION_TRAFFIC_REQUESTS,
        };
        let observed_at = epoch_to_rfc3339(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        let mut events = Vec::with_capacity(windows.len() * 3);
        for window in &windows {
            let attribution = UsageEventAttribution {
                hostname: Some(window.hostname.clone()),
                server_ip: Some(window.server_ip.clone()),
                server_port: Some(window.server_port),
                listener_id: Some(window.listener_id.clone()),
                app_id: window.app_id.clone(),
                app_slug: window.app_slug.clone(),
                app_uuid: window.app_uuid.clone(),
                binding_uuid: window.binding_uuid.clone(),
                status_class: (!window.status_class.is_empty())
                    .then_some(window.status_class.clone()),
            };
            events.push(UsageEventIngestItem {
                tenant_id: window.tenant_id,
                organization_id: window.organization_id,
                app_uuid: window.app_uuid.clone(),
                binding_uuid: window.binding_uuid.clone(),
                period_start: window.window_start.clone(),
                dimension: USAGE_DIMENSION_TRAFFIC_REQUESTS.to_owned(),
                quantity: i64::try_from(window.requests).unwrap_or(i64::MAX),
                unit: "REQUEST".to_owned(),
                deduplication_key: window.deduplication_key(USAGE_DIMENSION_TRAFFIC_REQUESTS),
                attribution: attribution.clone(),
                observed_at: observed_at.clone(),
            });
            if window.ingress_bytes > 0 {
                events.push(UsageEventIngestItem {
                    tenant_id: window.tenant_id,
                    organization_id: window.organization_id,
                    app_uuid: window.app_uuid.clone(),
                    binding_uuid: window.binding_uuid.clone(),
                    period_start: window.window_start.clone(),
                    dimension: USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES.to_owned(),
                    quantity: i64::try_from(window.ingress_bytes).unwrap_or(i64::MAX),
                    unit: "BYTE".to_owned(),
                    deduplication_key: window
                        .deduplication_key(USAGE_DIMENSION_TRAFFIC_INGRESS_BYTES),
                    attribution: attribution.clone(),
                    observed_at: observed_at.clone(),
                });
            }
            if window.egress_bytes > 0 {
                events.push(UsageEventIngestItem {
                    tenant_id: window.tenant_id,
                    organization_id: window.organization_id,
                    app_uuid: window.app_uuid.clone(),
                    binding_uuid: window.binding_uuid.clone(),
                    period_start: window.window_start.clone(),
                    dimension: sdkwork_deploy_contract::USAGE_DIMENSION_TRAFFIC_EGRESS_BYTES
                        .to_owned(),
                    quantity: i64::try_from(window.egress_bytes).unwrap_or(i64::MAX),
                    unit: "BYTE".to_owned(),
                    deduplication_key: window.deduplication_key(
                        sdkwork_deploy_contract::USAGE_DIMENSION_TRAFFIC_EGRESS_BYTES,
                    ),
                    attribution,
                    observed_at: observed_at.clone(),
                });
            }
        }
        let request = IngestUsageEventsRequest {
            node_uuid: Some(node_uuid.to_owned()),
            events,
        };
        let mut builder = self.client.post(&self.endpoint).json(&request);
        if let Some(token) = &self.auth_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| format!("usage ingest request failed: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".to_owned());
        if !status.is_success() {
            return Err(format!(
                "usage ingest endpoint returned {status}: {}",
                body.chars().take(300).collect::<String>()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Arc<UsageMeteringConfig> {
        Arc::new(UsageMeteringConfig {
            enabled: true,
            window_seconds: 1,
            flush_interval_ms: 30_000,
            channel: sdkwork_webserver_core::config::UsageMeteringChannel::Embedded,
        })
    }

    struct FakeChannel {
        ingested: std::sync::Mutex<Vec<UsageWindow>>,
    }

    #[async_trait]
    impl UsageIngestChannel for FakeChannel {
        async fn ingest(&self, _node_uuid: &str, windows: Vec<UsageWindow>) -> Result<(), String> {
            self.ingested.lock().unwrap().extend(windows);
            Ok(())
        }
    }

    #[tokio::test]
    async fn aggregates_requests_per_domain_server_and_attribution() {
        let channel = Arc::new(FakeChannel {
            ingested: std::sync::Mutex::new(Vec::new()),
        });
        let meter = Arc::new(UsageMeteringAggregator::new(
            config(),
            channel.clone(),
            "node-1",
        ));
        let attribution = UsageAttribution {
            tenant_id: Some(7),
            organization_id: Some(9),
            app_uuid: Some("site-10".to_owned()),
            binding_uuid: Some("binding-1".to_owned()),
            app_id: Some("app-20".to_owned()),
            app_slug: Some("shop".to_owned()),
        };
        let ip: IpAddr = "10.0.0.5".parse().unwrap();
        for _ in 0..3 {
            meter.record(MeteredRequest {
                hostname: "shop.app.sdkwork.com",
                server_ip: ip,
                server_port: 8080,
                listener_id: "http",
                attribution: &attribution,
                ingress_bytes: 100,
                egress_bytes: 200,
                status_class: "2xx",
            });
        }
        meter.record(MeteredRequest {
            hostname: "mysite.example.com",
            server_ip: ip,
            server_port: 8080,
            listener_id: "http",
            attribution: &UsageAttribution::default(),
            ingress_bytes: 0,
            egress_bytes: 50,
            status_class: "4xx",
        });
        // Open windows stay bucketed: a mid-window flush must not split a
        // window into two events with the same deduplication key.
        meter.flush().await;
        assert_eq!(channel.ingested.lock().unwrap().len(), 0);
        tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
        meter.flush().await;
        let windows = channel.ingested.lock().unwrap();
        assert_eq!(windows.len(), 2);
        let shop = windows
            .iter()
            .find(|window| window.hostname == "shop.app.sdkwork.com")
            .expect("shop window");
        assert_eq!(shop.requests, 3);
        assert_eq!(shop.egress_bytes, 600);
        assert_eq!(shop.ingress_bytes, 300);
        assert_eq!(shop.tenant_id, 7);
        assert_eq!(shop.binding_uuid.as_deref(), Some("binding-1"));
        assert_eq!(shop.app_id.as_deref(), Some("app-20"));
        let custom = windows
            .iter()
            .find(|window| window.hostname == "mysite.example.com")
            .expect("custom window");
        assert_eq!(
            custom.tenant_id, 0,
            "unmanaged traffic attributes to tenant 0"
        );
        assert_eq!(custom.requests, 1);
        assert_eq!(custom.egress_bytes, 50);
    }

    #[test]
    fn deduplication_keys_are_deterministic_and_bounded() {
        let window = UsageWindow {
            node_uuid: "node-1".to_owned(),
            tenant_id: 7,
            organization_id: 9,
            app_uuid: Some("site-10".to_owned()),
            binding_uuid: Some("binding-1".to_owned()),
            hostname: "shop.app.sdkwork.com".to_owned(),
            server_ip: "10.0.0.5".to_owned(),
            server_port: 8080,
            listener_id: "http".to_owned(),
            app_id: None,
            app_slug: None,
            status_class: String::new(),
            window_start: "2026-08-25T10:00:00Z".to_owned(),
            requests: 1,
            ingress_bytes: 0,
            egress_bytes: 0,
        };
        let key = window.deduplication_key(USAGE_DIMENSION_REQUESTS);
        assert_eq!(key, window.deduplication_key(USAGE_DIMENSION_REQUESTS));
        assert!(key.len() <= 200, "dedup key must fit the column: {key}");
        assert!(key.starts_with("traffic:2026-08-25T10:00:00Z:traffic.requests:"));
    }
}
