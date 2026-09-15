//! App publishing domain fallback.
//!
//! When no local virtual host or website runtime binding matches a request
//! host, the data plane resolves the server through the sdkwork-deployments
//! control plane and serves the site's latest compiled website runtime
//! descriptor with the website delivery executor.
//!
//! Both default app domains (`<slug>.app[-<env>].<suffix>` over the
//! configured platform suffixes) and user custom domains resolve through the
//! same lookup (`deploy_app_binding` rows are explicit for both).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use hashlink::LinkedHashMap;
use sdkwork_webserver_contract::provider::{WebsiteProviderError, WebsiteProviderErrorKind};
use sdkwork_webserver_core::config::AppDomainFallbackConfig;
use sdkwork_webserver_core::website_runtime::{
    compile_website_runtime_set_snapshot, website_runtime_descriptor_sha256,
    website_runtime_set_snapshot_sha256, CompiledWebsiteRuntimeSet, WebsiteRuntimeDescriptor,
    WebsiteRuntimeEnvironment, WebsiteRuntimeRegistry, WebsiteRuntimeSetSnapshot,
    WEBSITE_RUNTIME_SET_KIND, WEBSITE_RUNTIME_SET_SCHEMA_VERSION,
};
use sdkwork_webserver_delivery_runtime::{
    WebsiteDeliveryError, WebsiteDeliveryExecutor, WebsiteDeliveryOutcome, WebsiteDeliveryRequest,
    WebsiteProviderRegistry,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;

/// A resolved Deploy server: the app owning the matched hostname together
/// with its latest compiled website runtime descriptor.
#[derive(Clone, Debug)]
pub struct ResolvedDeployServer {
    pub app_uuid: String,
    pub app_slug: String,
    pub hostname: String,
    pub path_prefix: String,
    pub action_type: String,
    /// Owning tenant (usage metering attribution).
    pub tenant_id: i64,
    /// Owning app public uuid when the app is attributable.
    pub app_id: Option<String>,
    /// Matched binding public uuid (per-domain usage attribution).
    pub binding_id: Option<String>,
    /// The single-label prefix the app publishes under: its custom
    /// `appDomainLabel` when set, otherwise its slug (`deploy_app` field
    /// `app_domain_label`, environment-scoped by the binding).
    pub app_domain_label: String,
    /// nginx-compatible site configuration carried on the `deploy_app` row
    /// (`deploy_app.nginx_conf`), or the environment-scoped
    /// `deploy_nginx_config` override when one is registered for this
    /// hostname + environment. `None` when the app declares none.
    pub nginx_conf: Option<String>,
    /// SHA-256 of `nginx_conf`, or `None` when absent. Consumers use it to
    /// apply the configuration idempotently.
    pub nginx_conf_sha256: Option<String>,
    pub descriptor_json: Value,
    pub descriptor_sha256: String,
    pub revision_no: i64,
    pub environment: String,
}

/// Lookup port implemented by the embedded Deploy database adapter
/// (`EmbeddedDeployServerLookup`) or a control-plane HTTP client.
#[async_trait]
pub trait DeployServerLookup: Send + Sync {
    async fn resolve_server(
        &self,
        hostname: &str,
        environment: &str,
    ) -> Option<ResolvedDeployServer>;
}

/// The nginx-compatible site configuration an app publishes with, together
/// with the digest that makes applying it idempotent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NginxConfMaterial {
    pub config: String,
    pub sha256: String,
}

impl NginxConfMaterial {
    /// Build a material from the `deploy_app` / `deploy_nginx_config` pair.
    /// Both halves come from the same row (the lookup `COALESCE`s them
    /// together), so a half-populated pair is a control-plane defect rather
    /// than a normal state: it is reported once and then ignored.
    fn from_lookup(hostname: &str, config: Option<String>, sha256: Option<String>) -> Option<Self> {
        match (config, sha256) {
            (Some(config), Some(sha256)) => {
                if config.trim().is_empty() {
                    return None;
                }
                Some(Self { config, sha256 })
            }
            (None, None) => None,
            (config, sha256) => {
                tracing::warn!(
                    hostname,
                    has_config = config.is_some(),
                    has_sha256 = sha256.is_some(),
                    "app-domain fallback ignored a half-populated nginx configuration"
                );
                None
            }
        }
    }
}

/// Sink that materializes a resolved app's nginx-compatible site
/// configuration onto the edge.
///
/// The Deploy control plane stores the configuration (`deploy_app.nginx_conf`
/// with an environment/hostname-scoped `deploy_nginx_config` override) but
/// cannot write the edge's site files itself, so the data plane that resolved
/// the app hands the configuration to the edge through this port. The
/// implementation must be idempotent per `sha256`: the resolver calls it at
/// most once per `(hostname, sha256)` pair and never blocks serving on its
/// result.
pub trait NginxSiteSink: Send + Sync {
    fn apply_site(&self, hostname: &str, material: &NginxConfMaterial) -> Result<(), String>;
}

/// Host classification for a request authority. This is load-bearing: the
/// data plane uses it to refuse a platform hostname whose encoded lifecycle
/// environment is not the one this node serves, instead of issuing a lookup
/// that can only miss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostClass {
    /// `<appLabel>.app[-<env>].<suffix>` over the configured platform suffix
    /// catalog. `environment` is the lifecycle environment the platform label
    /// encodes (`app` => production).
    DefaultApp {
        app_label: String,
        environment: &'static str,
        suffix: String,
    },
    /// Any other hostname (user custom domain).
    Custom,
}

/// Classify a request host. The platform label must be one of the control
/// plane's labels (`app`, `app-dev`, `app-test`, `app-staging`, `app-demo`)
/// and the suffix must be in the configured catalog; everything else is a
/// custom domain.
///
/// The label set and the label -> environment mapping come from
/// `sdkwork-deployments` (`sdkwork-deploy-core`), the side that provisions the
/// hostnames, so this function can never drift from what the control plane
/// creates.
pub fn classify_host(hostname: &str, suffixes: &[String]) -> HostClass {
    let hostname = hostname.trim().to_ascii_lowercase();
    let mut labels = hostname.split('.');
    let app_label = labels.next().unwrap_or_default();
    let platform_label = labels.next().unwrap_or_default();
    let suffix = labels.collect::<Vec<_>>().join(".");
    let Some(environment) =
        sdkwork_deploy_core::environment_for_app_domain_label(platform_label)
    else {
        return HostClass::Custom;
    };
    if app_label.is_empty() || !suffixes.iter().any(|item| item == &suffix) {
        return HostClass::Custom;
    }
    HostClass::DefaultApp {
        app_label: app_label.to_owned(),
        environment,
        suffix,
    }
}

struct CacheEntry {
    descriptor: Option<Value>,
    descriptor_sha256: Option<String>,
    attribution: Option<crate::usage_metering::UsageAttribution>,
    /// The app's nginx-compatible site configuration, cached so the edge sink
    /// keeps being fed across cache hits (the resolver de-duplicates by
    /// `(hostname, sha256)` and a failed apply is retried on the next hit).
    nginx_conf: Option<NginxConfMaterial>,
    expires_at: Instant,
}

/// Upper bound on concurrently cached compiled fallback sites. Each entry
/// pins one compiled runtime set, so the cache is small and LRU-evicted;
/// beyond the bound a host simply recompiles on its next request.
const MAXIMUM_COMPILED_SITE_CACHE_ENTRIES: usize = 32;

impl Clone for CacheEntry {
    fn clone(&self) -> Self {
        Self {
            descriptor: self.descriptor.clone(),
            descriptor_sha256: self.descriptor_sha256.clone(),
            attribution: self.attribution.clone(),
            nginx_conf: self.nginx_conf.clone(),
            expires_at: self.expires_at,
        }
    }
}

/// Resolves unmatched hosts through the Deploy control plane and serves the
/// resolved site with a dedicated website delivery executor. Built by the
/// website data plane bootstrap when the app config declares an enabled
/// `appDomainFallback` section and the Deploy lookup is available.
pub struct DeployFallbackResolver {
    config: Arc<AppDomainFallbackConfig>,
    lookup: Arc<dyn DeployServerLookup>,
    environment: WebsiteRuntimeEnvironment,
    cache: ArcSwap<HashMap<String, CacheEntry>>,
    /// Compiled sites keyed by descriptor SHA-256 (LRU, bounded). This is
    /// both the recompilation fast path and the per-request correctness
    /// anchor: requests execute against the pinned compiled set, so a
    /// concurrent activation of a different host can never swap the served
    /// runtime set between route selection and provider dispatch.
    compiled_sites: SyncMutex<LinkedHashMap<String, Arc<CompiledWebsiteRuntimeSet>>>,
    /// Serializes compile+activate so the fallback runtime registry always
    /// observes monotonically increasing generations. The guard covers only
    /// the compile/activate section; request execution runs after release.
    activation: Mutex<()>,
    generation: AtomicU64,
    runtime_registry: Arc<WebsiteRuntimeRegistry>,
    executor: Arc<WebsiteDeliveryExecutor>,
    /// Optional edge sink for `deploy_app.nginx_conf`. Absent when the edge
    /// does not materialize site files, in which case the resolved
    /// configuration is still cached and observable but nothing is written.
    nginx_sites: Option<Arc<dyn NginxSiteSink>>,
    /// Last nginx site materialization per hostname, so an unchanged
    /// configuration is applied once instead of on every request.
    applied_nginx_conf: SyncMutex<HashMap<String, NginxSiteApplication>>,
}

/// Outcome of the last `deploy_app.nginx_conf` materialization for one host.
#[derive(Clone, Debug)]
struct NginxSiteApplication {
    /// Digest of the configuration that was handed to the sink.
    sha256: String,
    /// Whether the sink accepted it.
    applied: bool,
    /// When that attempt ran (success or failure), for the retry backoff.
    attempted_at: Instant,
}

/// Minimum interval between two materialization attempts for the same
/// `(hostname, digest)` after a failure.
///
/// Each attempt spawns an `nginx -t` validator process, so an app whose
/// configuration is permanently rejected must not be retried once per request.
/// Thirty seconds keeps the subprocess rate negligible while still letting the
/// edge pick up its configuration after a transient failure without an
/// operator restarting the node.
const NGINX_SITE_APPLY_RETRY_BACKOFF: Duration = Duration::from_secs(30);

impl DeployFallbackResolver {
    pub fn new(
        config: Arc<AppDomainFallbackConfig>,
        lookup: Arc<dyn DeployServerLookup>,
        providers: Arc<WebsiteProviderRegistry>,
        node_uuid: impl Into<String>,
        environment: WebsiteRuntimeEnvironment,
    ) -> Self {
        let runtime_registry = Arc::new(WebsiteRuntimeRegistry::new(node_uuid, environment));
        let executor = Arc::new(WebsiteDeliveryExecutor::new(
            Arc::clone(&runtime_registry),
            providers,
        ));
        Self {
            config,
            lookup,
            environment,
            cache: ArcSwap::from_pointee(HashMap::new()),
            compiled_sites: SyncMutex::new(LinkedHashMap::new()),
            activation: Mutex::new(()),
            generation: AtomicU64::new(0),
            runtime_registry,
            executor,
            nginx_sites: None,
            applied_nginx_conf: SyncMutex::new(HashMap::new()),
        }
    }

    /// Install the edge sink that materializes `deploy_app.nginx_conf`
    /// (`deploy_nginx_config` overrides included) as nginx site files.
    pub fn with_nginx_site_sink(mut self, sink: Arc<dyn NginxSiteSink>) -> Self {
        self.nginx_sites = Some(sink);
        self
    }

    pub fn config(&self) -> &AppDomainFallbackConfig {
        &self.config
    }

    /// Hand the resolved configuration to the edge sink unless this exact
    /// `(hostname, sha256)` pair is already materialized, or its last attempt
    /// failed and is still inside [`NGINX_SITE_APPLY_RETRY_BACKOFF`].
    ///
    /// The apply runs on the blocking pool: an nginx site activation validates
    /// the candidate with `nginx -t` (a subprocess with its own timeout), which
    /// must never occupy a request-serving runtime thread. A failure is logged
    /// and swallowed — site materialization is an optimization of the edge, not
    /// a precondition for serving the request that triggered it — and remains
    /// retriable, so a transient failure (an nginx that was momentarily
    /// unavailable, a filesystem blip) does not strand the edge without the
    /// app's configuration until the next node restart.
    async fn apply_nginx_site_conf(&self, hostname: &str, material: &NginxConfMaterial) {
        let Some(sink) = self.nginx_sites.clone() else {
            return;
        };
        let attempted_at = Instant::now();
        if !self.nginx_site_conf_is_due(hostname, &material.sha256, attempted_at) {
            return;
        }
        // `serve` calls this on every request, cache hit or miss, so the apply
        // owns its own copy of both inputs and the caller's borrows stay alive
        // for the logging below.
        let apply_hostname = hostname.to_owned();
        let apply_material = material.clone();
        let outcome = match tokio::task::spawn_blocking(move || {
            sink.apply_site(&apply_hostname, &apply_material)
        })
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("apply site configuration: {error}")),
            Err(error) => Err(format!("blocking apply task: {error}")),
        };
        self.record_nginx_site_conf_attempt(
            hostname,
            &material.sha256,
            outcome.is_ok(),
            attempted_at,
        );
        match outcome {
            Ok(()) => tracing::info!(
                hostname,
                nginx_conf_sha256 = %material.sha256,
                "app-domain fallback applied the app nginx site configuration"
            ),
            Err(error) => tracing::warn!(
                hostname,
                nginx_conf_sha256 = %material.sha256,
                error = %error,
                retry_after_secs = NGINX_SITE_APPLY_RETRY_BACKOFF.as_secs(),
                "app-domain fallback could not apply the app nginx site configuration"
            ),
        }
    }

    /// Report whether `(hostname, sha256)` still has to be handed to the sink.
    ///
    /// A digest never seen for the host always is; a digest that materialized
    /// is never applied again; a digest whose last attempt failed is due again
    /// once the backoff elapsed. Bounding the retry rate matters because each
    /// attempt spawns an `nginx -t` validator, so an app with a permanently
    /// broken configuration must not cost one subprocess per request.
    fn nginx_site_conf_is_due(&self, hostname: &str, sha256: &str, now: Instant) -> bool {
        let applied = self
            .applied_nginx_conf
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match applied.get(hostname) {
            Some(previous) if previous.sha256 == sha256 => {
                !previous.applied
                    && now.saturating_duration_since(previous.attempted_at)
                        >= NGINX_SITE_APPLY_RETRY_BACKOFF
            }
            _ => true,
        }
    }

    /// Remember the outcome of a materialization attempt for `(hostname,
    /// sha256)`. Kept per host (not per digest) so the map is bounded by the
    /// number of hosts this node has resolved, and a re-published
    /// configuration (a new digest) always supersedes the remembered one.
    fn record_nginx_site_conf_attempt(
        &self,
        hostname: &str,
        sha256: &str,
        applied: bool,
        attempted_at: Instant,
    ) {
        let mut recorded = self
            .applied_nginx_conf
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        recorded.insert(
            hostname.to_owned(),
            NginxSiteApplication {
                sha256: sha256.to_owned(),
                applied,
                attempted_at,
            },
        );
    }

    /// Traffic attribution cached for a hostname resolved through the
    /// Deploy control plane (tenant/app/binding for usage metering).
    pub fn attribution(&self, hostname: &str) -> Option<crate::usage_metering::UsageAttribution> {
        let hostname = hostname.trim().to_ascii_lowercase();
        self.cache
            .load()
            .get(&hostname)
            .and_then(|entry| entry.attribution.clone())
    }

    pub fn environment(&self) -> WebsiteRuntimeEnvironment {
        self.environment
    }

    /// Serve a request whose host did not match any local configuration.
    /// Returns `NotFound` when the host is not registered in Deploy or the
    /// fallback is disabled; propagates delivery errors from the executor.
    pub async fn serve(
        &self,
        request: &WebsiteDeliveryRequest,
    ) -> Result<WebsiteDeliveryOutcome, WebsiteDeliveryError> {
        if !self.config.enabled {
            return Ok(WebsiteDeliveryOutcome::NotFound);
        }
        let Some(hostname) = sdkwork_webserver_core::normalize_authority_host(&request.authority)
        else {
            return Ok(WebsiteDeliveryOutcome::NotFound);
        };
        let hostname = hostname.to_ascii_lowercase();
        let class = classify_host(&hostname, &self.config.suffixes);
        // A platform hostname encodes its lifecycle environment in the platform
        // label (`<appLabel>.app-dev.<suffix>`). This node serves exactly one
        // environment, so a hostname for another one can never resolve here:
        // refuse it explicitly instead of running a lookup that is guaranteed to
        // miss and would, if it ever matched, serve one environment's revision
        // on another environment's hostname.
        if let HostClass::DefaultApp { environment, .. } = &class {
            let node_environment = self.environment.as_str();
            if *environment != node_environment {
                tracing::debug!(
                    hostname = %hostname,
                    host_environment = environment,
                    node_environment,
                    "app-domain fallback refused a platform hostname for another lifecycle environment"
                );
                return Ok(WebsiteDeliveryOutcome::NotFound);
            }
        }
        let now = Instant::now();
        let cached = self.cache.load().get(&hostname).cloned();
        let (descriptor, descriptor_sha256, attribution, nginx_conf) = match cached {
            // Negative cache hit: the host was recently resolved to nothing;
            // do not hit the Deploy lookup again inside the negative TTL.
            Some(entry) if entry.expires_at > now && entry.descriptor.is_none() => {
                return Ok(WebsiteDeliveryOutcome::NotFound);
            }
            Some(entry) if entry.expires_at > now => (
                entry.descriptor.clone(),
                entry.descriptor_sha256.clone(),
                entry.attribution.clone(),
                entry.nginx_conf.clone(),
            ),
            Some(expired) => {
                tracing::debug!(
                    hostname = %hostname,
                    class = %class_label(&class),
                    "app-domain fallback cache expired; re-resolving"
                );
                (
                    expired.descriptor,
                    expired.descriptor_sha256,
                    expired.attribution,
                    expired.nginx_conf,
                )
            }
            None => (None, None, None, None),
        };
        let (descriptor, descriptor_sha256, attribution, nginx_conf) =
            match (descriptor, descriptor_sha256, attribution, nginx_conf) {
                (Some(descriptor), Some(descriptor_sha256), attribution, nginx_conf) => {
                    (
                        Some(descriptor),
                        Some(descriptor_sha256),
                        attribution,
                        nginx_conf,
                    )
                }
                _ => {
                    let environment = self.environment.as_str();
                    let resolved = self.lookup.resolve_server(&hostname, environment).await;
                    let (descriptor, descriptor_sha256, attribution, nginx_conf) = match resolved {
                        Some(server) => {
                            tracing::info!(
                                hostname = %hostname,
                                class = %class_label(&class),
                                app_uuid = %server.app_uuid,
                                app_slug = %server.app_slug,
                                app_domain_label = %server.app_domain_label,
                                has_nginx_conf = server.nginx_conf.is_some(),
                                revision_no = server.revision_no,
                                "app-domain fallback resolved deploy server"
                            );
                            let attribution = Some(crate::usage_metering::UsageAttribution {
                                tenant_id: Some(server.tenant_id),
                                organization_id: None,
                                app_uuid: Some(server.app_uuid.clone()),
                                binding_uuid: server.binding_id.clone(),
                                app_id: server.app_id.clone(),
                                app_slug: Some(server.app_slug.clone()),
                            });
                            let nginx_conf = NginxConfMaterial::from_lookup(
                                &hostname,
                                server.nginx_conf,
                                server.nginx_conf_sha256,
                            );
                            (
                                Some(server.descriptor_json),
                                Some(server.descriptor_sha256),
                                attribution,
                                nginx_conf,
                            )
                        }
                        None => {
                            tracing::debug!(
                                hostname = %hostname,
                                class = %class_label(&class),
                                "app-domain fallback has no deploy server for host"
                            );
                            (None, None, None, None)
                        }
                    };
                    self.cache.store(Arc::new({
                        let mut cache = (**self.cache.load()).clone();
                        cache.insert(
                            hostname.clone(),
                            CacheEntry {
                                descriptor: descriptor.clone(),
                                descriptor_sha256: descriptor_sha256.clone(),
                                attribution: attribution.clone(),
                                nginx_conf: nginx_conf.clone(),
                                expires_at: if descriptor.is_some() {
                                    now + Duration::from_millis(self.config.cache_ttl_ms)
                                } else {
                                    now + Duration::from_millis(self.config.negative_cache_ttl_ms)
                                },
                            },
                        );
                        cache
                    }));
                    (descriptor, descriptor_sha256, attribution, nginx_conf)
                }
            };
        let Some(descriptor) = descriptor else {
            return Ok(WebsiteDeliveryOutcome::NotFound);
        };
        // Attribution is cached for usage metering consumers
        // (`DeployFallbackResolver::attribution`); serve itself does not
        // consume it.
        let _ = &attribution;
        // The app's nginx-compatible site configuration rides along with the
        // resolution so the edge can materialize it. This is deliberately
        // decoupled from serving: it happens at most once per
        // (hostname, configuration digest) and its result never changes the
        // response.
        if let Some(material) = nginx_conf.as_ref() {
            self.apply_nginx_site_conf(&hostname, material).await;
        }
        // Fast path: the descriptor is already compiled — serve the pinned
        // set without taking the activation lock or recompiling.
        let Some(descriptor_sha256) = descriptor_sha256 else {
            return Ok(WebsiteDeliveryOutcome::NotFound);
        };
        if let Some(compiled) = self.compiled_site(&descriptor_sha256) {
            return self
                .executor
                .execute_compiled(request.clone(), compiled)
                .await;
        }
        // Slow path: serialize compile+activate so the fallback runtime
        // registry observes monotonically increasing generations. The guard
        // covers only the compile/activate section — request execution runs
        // against the pinned compiled set after the guard is released, so
        // upstream/provider latency never holds the lock.
        let compiled = {
            let _guard = self.activation.lock().await;
            match self.compiled_site(&descriptor_sha256) {
                Some(compiled) => compiled,
                None => {
                    let compiled = match self
                        .compile_site(descriptor, Some(descriptor_sha256.as_str()))
                    {
                        Ok(compiled) => compiled,
                        Err(error) => {
                            tracing::warn!(hostname = %hostname, error = ?error, "app-domain fallback compile failed");
                            return Err(error);
                        }
                    };
                    self.activate(compiled.clone()).await?;
                    self.remember_compiled_site(descriptor_sha256, compiled.clone());
                    compiled
                }
            }
        };
        self.executor
            .execute_compiled(request.clone(), compiled)
            .await
    }

    /// LRU touch: remove+reinsert moves the entry to the most-recently-used
    /// position in O(1) (no await inside the lock guard).
    fn compiled_site(&self, descriptor_sha256: &str) -> Option<Arc<CompiledWebsiteRuntimeSet>> {
        let mut cache = self
            .compiled_sites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let compiled = cache.remove(descriptor_sha256)?;
        cache.insert(descriptor_sha256.to_owned(), compiled.clone());
        Some(compiled)
    }

    fn remember_compiled_site(
        &self,
        descriptor_sha256: String,
        compiled: Arc<CompiledWebsiteRuntimeSet>,
    ) {
        let mut cache = self
            .compiled_sites
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.len() >= MAXIMUM_COMPILED_SITE_CACHE_ENTRIES
            && !cache.contains_key(&descriptor_sha256)
        {
            cache.pop_front();
        }
        cache.insert(descriptor_sha256, compiled);
    }

    fn compile_site(
        &self,
        descriptor_json: Value,
        expected_sha256: Option<&str>,
    ) -> Result<Arc<CompiledWebsiteRuntimeSet>, WebsiteDeliveryError> {
        let mut descriptor_json = descriptor_json;
        let parsed: WebsiteRuntimeDescriptor = serde_json::from_value(descriptor_json.clone())
            .map_err(|_| contract_error("app-domain fallback descriptor is invalid"))?;
        let calculated = website_runtime_descriptor_sha256(&parsed)
            .map_err(|_| contract_error("app-domain fallback descriptor hash failed"))?;
        if let Some(expected) = expected_sha256 {
            if expected != calculated {
                return Err(contract_error(
                    "app-domain fallback descriptor hash mismatch",
                ));
            }
        }
        descriptor_json["descriptorSha256"] = Value::String(calculated);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generated_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());
        let mut snapshot = json!({
            "schemaVersion": WEBSITE_RUNTIME_SET_SCHEMA_VERSION,
            "kind": WEBSITE_RUNTIME_SET_KIND,
            "snapshotUuid": format!("app-domain-fallback-{generation}"),
            "nodeUuid": self.runtime_registry_node_uuid(),
            "environment": self.environment.as_str(),
            "generation": generation,
            "generatedAt": generated_at,
            "compilerVersion": "sdkwork-webserver-app-domain-fallback/1",
            "snapshotSha256": "0".repeat(64),
            "maximumSites": 1,
            "descriptors": [descriptor_json]
        });
        let parsed: WebsiteRuntimeSetSnapshot = serde_json::from_value(snapshot.clone())
            .map_err(|_| contract_error("app-domain fallback runtime set is invalid"))?;
        let snapshot_sha256 = website_runtime_set_snapshot_sha256(&parsed)
            .map_err(|_| contract_error("app-domain fallback runtime set hash failed"))?;
        snapshot["snapshotSha256"] = Value::String(snapshot_sha256);
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|_| contract_error("app-domain fallback runtime set serialization failed"))?;
        let compiled = compile_website_runtime_set_snapshot(&bytes)
            .map_err(|_| contract_error("app-domain fallback runtime set compile failed"))?;
        Ok(Arc::new(compiled))
    }

    fn runtime_registry_node_uuid(&self) -> String {
        self.runtime_registry
            .current()
            .map(|current| current.node_uuid().to_owned())
            .unwrap_or_default()
    }

    async fn activate(
        &self,
        compiled: Arc<CompiledWebsiteRuntimeSet>,
    ) -> Result<(), WebsiteDeliveryError> {
        if self
            .runtime_registry
            .current()
            .is_some_and(|current| current.snapshot_sha256() == compiled.snapshot_sha256())
        {
            return Ok(());
        }
        self.runtime_registry.activate(compiled).map_err(|error| {
            tracing::warn!(error = ?error, "app-domain fallback activation failed");
            contract_error("app-domain fallback activation failed")
        })?;
        Ok(())
    }
}

fn contract_error(detail: &'static str) -> WebsiteDeliveryError {
    tracing::debug!(detail, "app-domain fallback contract error");
    WebsiteProviderError {
        kind: WebsiteProviderErrorKind::ContractMismatch,
        retry_after_ms: None,
    }
    .into()
}

fn class_label(class: &HostClass) -> &'static str {
    match class {
        HostClass::DefaultApp { .. } => "default-app",
        HostClass::Custom => "custom",
    }
}

/// Embedded lookup: resolves through the shared Deploy database
/// (`deploy_app_binding` / `deploy_app_revision`) using the Deploy
/// repository crate. Requires the process database connection (management
/// feature).
#[cfg(feature = "management")]
pub struct EmbeddedDeployServerLookup {
    repository: sdkwork_api_webserver_assembly::DeployRepository,
}

#[cfg(feature = "management")]
impl EmbeddedDeployServerLookup {
    pub fn new(repository: sdkwork_api_webserver_assembly::DeployRepository) -> Self {
        Self { repository }
    }
}

#[cfg(feature = "management")]
#[async_trait]
impl DeployServerLookup for EmbeddedDeployServerLookup {
    async fn resolve_server(
        &self,
        hostname: &str,
        environment: &str,
    ) -> Option<ResolvedDeployServer> {
        let resolved = self
            .repository
            .resolve_server_by_hostname_lookup(hostname, environment)
            .await
            .ok()?;
        let resolved = resolved?;
        Some(ResolvedDeployServer {
            app_uuid: resolved.app_uuid,
            app_slug: resolved.app_slug,
            hostname: resolved.hostname,
            path_prefix: resolved.path_prefix,
            action_type: resolved.action_type,
            tenant_id: resolved.tenant_id,
            app_id: resolved.app_id,
            binding_id: resolved.binding_id,
            app_domain_label: resolved.app_domain_label,
            nginx_conf: resolved.nginx_conf,
            nginx_conf_sha256: resolved.nginx_conf_sha256,
            descriptor_json: resolved.descriptor_json,
            descriptor_sha256: resolved.descriptor_sha256,
            revision_no: resolved.revision_no,
            environment: resolved.environment,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suffixes() -> Vec<String> {
        [
            "sdkwork.com",
            "sdkwork.cn",
            "birdcoder.com",
            "birdcoder.cn",
            "86offer.com",
            "86offer.cn",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn classifies_default_app_domains_and_custom_domains() {
        let list = suffixes();
        let expected = |app_label: &str, environment: &'static str, suffix: &str| HostClass::DefaultApp {
            app_label: app_label.to_owned(),
            environment,
            suffix: suffix.to_owned(),
        };
        assert_eq!(
            classify_host("myapp.app.sdkwork.com", &list),
            expected("myapp", "production", "sdkwork.com")
        );
        assert_eq!(
            classify_host("shop.app-dev.birdcoder.cn", &list),
            expected("shop", "development", "birdcoder.cn")
        );
        assert_eq!(
            classify_host("MyApp.APP.Sdkwork.COM", &list),
            expected("myapp", "production", "sdkwork.com")
        );
        for (hostname, environment) in [
            ("myapp.app-test.sdkwork.com", "test"),
            ("myapp.app-staging.sdkwork.com", "staging"),
            ("myapp.app-demo.sdkwork.com", "demo"),
        ] {
            assert_eq!(
                classify_host(hostname, &list),
                expected("myapp", environment, "sdkwork.com"),
                "hostname must classify as a default app domain: {hostname}"
            );
        }
        for custom in [
            "mysite.example.com",
            "myapp.app.unknown.com",
            "myapp.other.sdkwork.com",
            "myapp.app-prod.sdkwork.com",
            "localhost",
            "drive-dev.sdkwork.com",
            "",
            "myapp.app.sdkwork.com.",
        ] {
            assert_eq!(
                classify_host(custom, &list),
                HostClass::Custom,
                "hostname must be custom: {custom}"
            );
        }
    }

    /// The platform suffix catalog is owned by `sdkwork-deployments`; the edge
    /// must never accept a suffix the control plane does not provision.
    #[test]
    fn suffix_catalog_is_the_deploy_core_authority() {
        let catalog = sdkwork_deploy_core::platform_app_domain_suffixes();
        assert_eq!(catalog.len(), 14, "platform catalog must stay at 14 suffixes");
        assert!(catalog.iter().any(|item| item == "sdkwork.com"));
        assert!(
            !catalog.iter().any(|item| item == "noaper.com"),
            "noaper.com is not a platform suffix"
        );
        let catalog_labels = [
            "app",
            "app-dev",
            "app-test",
            "app-staging",
            "app-demo",
        ];
        for label in catalog_labels {
            assert!(
                sdkwork_deploy_core::environment_for_app_domain_label(label).is_some(),
                "label must be recognised by the control plane: {label}"
            );
        }
        assert!(sdkwork_deploy_core::environment_for_app_domain_label("app-prod").is_none());
    }

    /// The five lifecycle keys are the platform catalog: they are the
    /// `deploy_app_binding.environment` values the control plane writes, the
    /// `SDKWORK_WEBSERVER_ENVIRONMENT` values the node accepts, and the
    /// `<appLabel>.app[-<env>].<suffix>` label suffixes.
    #[test]
    fn environment_names_match_deploy_catalog() {
        let expected = [
            (WebsiteRuntimeEnvironment::Development, "development"),
            (WebsiteRuntimeEnvironment::Test, "test"),
            (WebsiteRuntimeEnvironment::Staging, "staging"),
            (WebsiteRuntimeEnvironment::Demo, "demo"),
            (WebsiteRuntimeEnvironment::Production, "production"),
        ];
        assert_eq!(expected.len(), WebsiteRuntimeEnvironment::ALL.len());
        for (environment, name) in expected {
            assert_eq!(environment.as_str(), name);
            assert_eq!(WebsiteRuntimeEnvironment::parse(name), Ok(environment));
            assert!(WebsiteRuntimeEnvironment::ALL.contains(&environment));
        }
        // `parse` is the strict catalog; the short aliases are opt-in.
        assert!(WebsiteRuntimeEnvironment::parse("prod").is_err());
        assert_eq!(
            WebsiteRuntimeEnvironment::parse_with_aliases("prod"),
            Ok(WebsiteRuntimeEnvironment::Production)
        );
    }

    /// Lookup that never resolves: the nginx sink tests drive
    /// `apply_nginx_site_conf` directly, so no host has to resolve.
    struct UnusedLookup;

    #[async_trait]
    impl DeployServerLookup for UnusedLookup {
        async fn resolve_server(
            &self,
            _hostname: &str,
            _environment: &str,
        ) -> Option<ResolvedDeployServer> {
            None
        }
    }

    /// Sink that counts attempts, optionally failing all of them.
    struct CountingSink {
        attempts: std::sync::atomic::AtomicUsize,
        fail: bool,
    }

    impl NginxSiteSink for CountingSink {
        fn apply_site(&self, _hostname: &str, _material: &NginxConfMaterial) -> Result<(), String> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                // Mirrors `nginx -t` rejecting the candidate.
                Err("synthetic site validation failure".to_owned())
            } else {
                Ok(())
            }
        }
    }

    fn test_resolver(sink: Option<Arc<dyn NginxSiteSink>>) -> DeployFallbackResolver {
        let resolver = DeployFallbackResolver::new(
            Arc::new(AppDomainFallbackConfig::default()),
            Arc::new(UnusedLookup),
            Arc::new(WebsiteProviderRegistry::new()),
            "node-nginx-sink-test",
            WebsiteRuntimeEnvironment::Production,
        );
        match sink {
            Some(sink) => resolver.with_nginx_site_sink(sink),
            None => resolver,
        }
    }

    fn material(sha256: &str) -> NginxConfMaterial {
        NginxConfMaterial {
            config: format!("server {{ # {sha256}\n}}\n"),
            sha256: sha256.to_owned(),
        }
    }

    /// The state machine that decides whether the sink still has to be fed.
    /// `now` is a parameter precisely so this is testable without sleeping.
    #[test]
    fn nginx_site_conf_due_state_machine() {
        let resolver = test_resolver(None);
        let host = "shop.app.sdkwork.com";
        let t0 = Instant::now();

        assert!(
            resolver.nginx_site_conf_is_due(host, "d1", t0),
            "an unseen digest is due"
        );
        resolver.record_nginx_site_conf_attempt(host, "d1", true, t0);
        assert!(
            !resolver.nginx_site_conf_is_due(host, "d1", t0 + Duration::from_secs(86_400)),
            "a materialized digest is never re-applied, however long the host keeps serving"
        );
        assert!(
            resolver.nginx_site_conf_is_due(host, "d2", t0),
            "a re-published configuration supersedes the remembered digest"
        );

        // A failure stays retriable, but only once the backoff elapsed.
        resolver.record_nginx_site_conf_attempt(host, "d3", false, t0);
        assert!(
            !resolver.nginx_site_conf_is_due(
                host,
                "d3",
                t0 + NGINX_SITE_APPLY_RETRY_BACKOFF - Duration::from_millis(1)
            ),
            "a failed attempt must not be retried inside the backoff (one nginx -t per request)"
        );
        assert!(
            resolver.nginx_site_conf_is_due(host, "d3", t0 + NGINX_SITE_APPLY_RETRY_BACKOFF),
            "a failed attempt is retried once the backoff elapsed"
        );
    }

    /// A successful materialization happens once; a re-published digest is
    /// applied again.
    #[tokio::test]
    async fn successful_nginx_site_conf_is_applied_once_per_digest() {
        let sink = Arc::new(CountingSink {
            attempts: std::sync::atomic::AtomicUsize::new(0),
            fail: false,
        });
        let resolver = test_resolver(Some(sink.clone()));
        let host = "shop.app.sdkwork.com";

        resolver.apply_nginx_site_conf(host, &material("d1")).await;
        resolver.apply_nginx_site_conf(host, &material("d1")).await;
        assert_eq!(
            sink.attempts.load(Ordering::SeqCst),
            1,
            "the sink is fed once per (hostname, digest), not once per request"
        );

        resolver.apply_nginx_site_conf(host, &material("d2")).await;
        assert_eq!(
            sink.attempts.load(Ordering::SeqCst),
            2,
            "re-publishing the configuration applies it again"
        );
    }

    /// A failing sink is not retried per request (the apply spawns an
    /// `nginx -t` process), and never fails the served response.
    #[tokio::test]
    async fn failed_nginx_site_conf_backs_off_and_is_retriable() {
        let sink = Arc::new(CountingSink {
            attempts: std::sync::atomic::AtomicUsize::new(0),
            fail: true,
        });
        let resolver = test_resolver(Some(sink.clone()));
        let host = "shop.app.sdkwork.com";

        resolver.apply_nginx_site_conf(host, &material("d1")).await;
        resolver.apply_nginx_site_conf(host, &material("d1")).await;
        assert_eq!(
            sink.attempts.load(Ordering::SeqCst),
            1,
            "a failure inside the backoff must not spawn a second validator"
        );

        // Age the recorded failure past the backoff and confirm it is due.
        let now = Instant::now();
        resolver.record_nginx_site_conf_attempt(
            host,
            "d1",
            false,
            now - NGINX_SITE_APPLY_RETRY_BACKOFF - Duration::from_secs(1),
        );
        assert!(resolver.nginx_site_conf_is_due(host, "d1", now));
    }

    /// Without an installed sink the whole path is a no-op (the node does not
    /// materialize nginx site files) and nothing is recorded.
    #[tokio::test]
    async fn absent_nginx_site_sink_records_nothing() {
        let resolver = test_resolver(None);
        resolver
            .apply_nginx_site_conf("shop.app.sdkwork.com", &material("d1"))
            .await;
        assert!(resolver.nginx_site_conf_is_due("shop.app.sdkwork.com", "d1", Instant::now()));
    }
}
