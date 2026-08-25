//! App publishing domain fallback.
//!
//! When no local virtual host or website runtime binding matches a request
//! host, the data plane resolves the server through the sdkwork-deployments
//! control plane and serves the site's latest compiled website runtime
//! descriptor with the website delivery executor.
//!
//! Both default app domains (`<slug>.app[-<env>].<suffix>` over the
//! configured platform suffixes) and user custom domains resolve through the
//! same lookup (`deploy_site_binding` rows are explicit for both).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use serde_json::{json, Value};
use sdkwork_webserver_contract::provider::{
    WebsiteProviderError, WebsiteProviderErrorKind,
};
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
use tokio::sync::Mutex;

/// A resolved Deploy server: the site owning the matched hostname together
/// with its latest compiled website runtime descriptor.
#[derive(Clone, Debug)]
pub struct ResolvedDeployServer {
    pub site_uuid: String,
    pub site_slug: String,
    pub hostname: String,
    pub path_prefix: String,
    pub action_type: String,
    /// Owning tenant (usage metering attribution).
    pub tenant_id: i64,
    /// Owning app public uuid when the site belongs to an app.
    pub app_id: Option<String>,
    /// Matched binding public uuid (per-domain usage attribution).
    pub binding_id: Option<String>,
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

/// Host classification for a request authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostClass {
    /// `<slug>.app[-<env>].<suffix>` over a configured platform suffix.
    DefaultApp { slug: String, suffix: String },
    /// Any other hostname (user custom domain).
    Custom,
}

/// Classify a request host. The app label must be one of the platform labels
/// (`app`, `app-dev`, `app-test`, `app-staging`) and the suffix must be in
/// the configured catalog; everything else is a custom domain.
pub fn classify_host(hostname: &str, suffixes: &[String]) -> HostClass {
    let hostname = hostname.trim().to_ascii_lowercase();
    let mut labels = hostname.split('.');
    let slug = labels.next().unwrap_or_default();
    let label = labels.next().unwrap_or_default();
    let suffix = labels.collect::<Vec<_>>().join(".");
    let is_platform_label = matches!(label, "app" | "app-dev" | "app-test" | "app-staging");
    if !slug.is_empty() && is_platform_label && suffixes.iter().any(|item| item == &suffix) {
        return HostClass::DefaultApp {
            slug: slug.to_owned(),
            suffix,
        };
    }
    HostClass::Custom
}

pub(crate) fn environment_name(environment: &WebsiteRuntimeEnvironment) -> &'static str {
    match environment {
        WebsiteRuntimeEnvironment::Development => "development",
        WebsiteRuntimeEnvironment::Test => "test",
        WebsiteRuntimeEnvironment::Staging => "staging",
        WebsiteRuntimeEnvironment::Production => "production",
    }
}

struct CacheEntry {
    descriptor: Option<Value>,
    descriptor_sha256: Option<String>,
    attribution: Option<crate::usage_metering::UsageAttribution>,
    expires_at: Instant,
}

impl Clone for CacheEntry {
    fn clone(&self) -> Self {
        Self {
            descriptor: self.descriptor.clone(),
            descriptor_sha256: self.descriptor_sha256.clone(),
            attribution: self.attribution.clone(),
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
    /// Serializes compile+activate so the fallback runtime registry always
    /// observes monotonically increasing generations.
    activation: Mutex<()>,
    generation: AtomicU64,
    runtime_registry: Arc<WebsiteRuntimeRegistry>,
    executor: Arc<WebsiteDeliveryExecutor>,
}

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
            activation: Mutex::new(()),
            generation: AtomicU64::new(0),
            runtime_registry,
            executor,
        }
    }

    pub fn config(&self) -> &AppDomainFallbackConfig {
        &self.config
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
        let now = Instant::now();
        let cached = self.cache.load().get(&hostname).cloned();
        let (descriptor, descriptor_sha256, attribution) = match cached {
            Some(entry) if entry.expires_at > now => (
                entry.descriptor.clone(),
                entry.descriptor_sha256.clone(),
                entry.attribution.clone(),
            ),
            Some(expired) => {
                tracing::debug!(
                    hostname = %hostname,
                    class = %class_label(&class),
                    "app-domain fallback cache expired; re-resolving"
                );
                (expired.descriptor, expired.descriptor_sha256, expired.attribution)
            }
            None => (None, None, None),
        };
        let (descriptor, descriptor_sha256, attribution) =
            match (descriptor, descriptor_sha256, attribution) {
                (Some(descriptor), Some(descriptor_sha256), attribution) => {
                    (Some(descriptor), Some(descriptor_sha256), attribution)
                }
                _ => {
                    let environment = environment_name(&self.environment);
                    let resolved = self.lookup.resolve_server(&hostname, environment).await;
                    let (descriptor, descriptor_sha256, attribution) = match resolved {
                        Some(server) => {
                            tracing::info!(
                                hostname = %hostname,
                                class = %class_label(&class),
                                site_uuid = %server.site_uuid,
                                site_slug = %server.site_slug,
                                revision_no = server.revision_no,
                                "app-domain fallback resolved deploy server"
                            );
                            let attribution = Some(crate::usage_metering::UsageAttribution {
                                tenant_id: Some(server.tenant_id),
                                organization_id: None,
                                site_uuid: Some(server.site_uuid.clone()),
                                binding_uuid: server.binding_id.clone(),
                                app_id: server.app_id.clone(),
                                app_slug: Some(server.site_slug.clone()),
                            });
                            (
                                Some(server.descriptor_json),
                                Some(server.descriptor_sha256),
                                attribution,
                            )
                        }
                        None => {
                            tracing::debug!(
                                hostname = %hostname,
                                class = %class_label(&class),
                                "app-domain fallback has no deploy server for host"
                            );
                            (None, None, None)
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
                                expires_at: if descriptor.is_some() {
                                    now + Duration::from_millis(self.config.cache_ttl_ms)
                                } else {
                                    now + Duration::from_millis(self.config.negative_cache_ttl_ms)
                                },
                            },
                        );
                        cache
                    }));
                    (descriptor, descriptor_sha256, attribution)
                }
            };
        let Some(descriptor) = descriptor else {
            return Ok(WebsiteDeliveryOutcome::NotFound);
        };
        let _guard = self.activation.lock().await;
        let compiled = match self.compile_site(descriptor, descriptor_sha256.as_deref()) {
            Ok(compiled) => compiled,
            Err(error) => {
                tracing::warn!(hostname = %hostname, error = ?error, "app-domain fallback compile failed");
                return Err(error);
            }
        };
        self.activate(compiled).await?;
        self.executor.execute(request.clone()).await
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
                return Err(contract_error("app-domain fallback descriptor hash mismatch"));
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
            "environment": environment_name(&self.environment),
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
/// (`deploy_site_binding` / `deploy_site_revision`) using the Deploy
/// repository crate. Requires the process database connection (management
/// feature).
#[cfg(feature = "management")]
pub struct EmbeddedDeployServerLookup {
    repository: sdkwork_intelligence_deploy_repository_sqlx::DeployRepository,
}

#[cfg(feature = "management")]
impl EmbeddedDeployServerLookup {
    pub fn new(
        repository: sdkwork_intelligence_deploy_repository_sqlx::DeployRepository,
    ) -> Self {
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
            site_uuid: resolved.site_uuid,
            site_slug: resolved.site_slug,
            hostname: resolved.hostname,
            path_prefix: resolved.path_prefix,
            action_type: resolved.action_type,
            tenant_id: resolved.tenant_id,
            app_id: resolved.app_id,
            binding_id: resolved.binding_id,
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
        assert_eq!(
            classify_host("myapp.app.sdkwork.com", &list),
            HostClass::DefaultApp {
                slug: "myapp".to_owned(),
                suffix: "sdkwork.com".to_owned(),
            }
        );
        assert_eq!(
            classify_host("shop.app-dev.birdcoder.cn", &list),
            HostClass::DefaultApp {
                slug: "shop".to_owned(),
                suffix: "birdcoder.cn".to_owned(),
            }
        );
        assert_eq!(
            classify_host("MyApp.APP.Sdkwork.COM", &list),
            HostClass::DefaultApp {
                slug: "myapp".to_owned(),
                suffix: "sdkwork.com".to_owned(),
            }
        );
        for custom in [
            "mysite.example.com",
            "myapp.app.unknown.com",
            "myapp.other.sdkwork.com",
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

    #[test]
    fn environment_names_match_deploy_catalog() {
        assert_eq!(
            environment_name(&WebsiteRuntimeEnvironment::Production),
            "production"
        );
        assert_eq!(
            environment_name(&WebsiteRuntimeEnvironment::Development),
            "development"
        );
        assert_eq!(environment_name(&WebsiteRuntimeEnvironment::Test), "test");
        assert_eq!(
            environment_name(&WebsiteRuntimeEnvironment::Staging),
            "staging"
        );
    }
}
