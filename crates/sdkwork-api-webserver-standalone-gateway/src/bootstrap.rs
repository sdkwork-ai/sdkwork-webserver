use axum::Router;
use sdkwork_database_sqlx::process_shared_database_pool;
use sdkwork_iam_web_adapter::{iam_web_request_context_resolver_from_env, IamAuthorizationPolicy};
use sdkwork_web_bootstrap::{ComposedApiAssembly, CompositeReadinessCheck, WebFrameworkBuilder};
use sdkwork_web_core::{
    HttpMetricsRegistry, IdempotencyStore, WebEnvironment, WebFrameworkOptionalFeatures,
    WebRequestContextProfile,
};
use sdkwork_web_store_sqlx::{bootstrap_webstore_database, SqlxIdempotencyStore};
use sdkwork_webserver_http_host::{
    web_framework_runtime_policy_from_env, with_problem_correlation,
    MachineCredentialResolverDecorator, WebServerTenantIsolationPolicy,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use crate::{app_shell::AdaptiveAppShellConfig, profile::assemble_standalone_profile};

/// Business handler deadline. Requests that exceed this budget (stuck
/// databases, hung git imports, stalled ACME work) fail with a bounded
/// timeout instead of occupying a worker indefinitely.
const DEFAULT_BUSINESS_HANDLER_TIMEOUT: Duration = Duration::from_secs(60);

pub async fn build_router() -> Result<Router, String> {
    let app_shell = AdaptiveAppShellConfig::from_env()?;
    let profile = assemble_standalone_profile()
        .await
        .map_err(|error| error.to_string())?;
    // IAM database lifecycle: the standalone gateway owns the IAM module
    // (SDKWORK_IAM_APP_ROOT). Bootstrap performs init, auto-migrate, and the
    // drift check, and installs the process-shared pool for the IAM web
    // adapter (tenant/session lookups).
    let iam_pool = process_shared_database_pool().ok_or_else(|| {
        "standalone gateway requires the process-shared database pool; bootstrap the database lifecycle first".to_string()
    })?;
    sdkwork_api_iam_assembly::bootstrap_database_with_pool(iam_pool)
        .await
        .map_err(|error| format!("IAM database bootstrap failed: {error}"))?;
    let metrics = HttpMetricsRegistry::new();
    let resolver = MachineCredentialResolverDecorator::new_standalone_iam(
        iam_web_request_context_resolver_from_env().await,
        profile.machine_authenticator.clone(),
    );
    let (environment, security_policy) = web_framework_runtime_policy_from_env();
    let request_profile = WebRequestContextProfile {
        environment,
        ..WebRequestContextProfile::default()
    };
    profile
        .assembly
        .route_manifest
        .validate_route_auth_for_surfaces(&request_profile)
        .map_err(|error| format!("standalone route auth validation failed: {error}"))?;
    let readiness_check = match app_shell.as_ref() {
        Some(app_shell) => Arc::new(CompositeReadinessCheck::new(vec![
            profile.assembly.readiness_check.clone(),
            app_shell.readiness_check(),
        ])) as Arc<dyn sdkwork_web_bootstrap::ReadinessCheck>,
        None => profile.assembly.readiness_check.clone(),
    };
    // Durable idempotency: replace the process-local memory store with the
    // PostgreSQL-backed store so replay deduplication survives restarts and
    // multiple gateway replicas sharing the database.
    let store_pool = process_shared_database_pool().ok_or_else(|| {
        "standalone gateway requires the process-shared database pool; bootstrap the database lifecycle first".to_string()
    })?;
    let store_host = bootstrap_webstore_database(store_pool)
        .await
        .map_err(|error| format!("web store bootstrap failed: {error}"))?;
    let pg_pool = store_host
        .pool()
        .as_postgres()
        .ok_or_else(|| "standalone gateway web store requires a PostgreSQL pool".to_string())?
        .clone();
    let idempotency_store: Arc<dyn IdempotencyStore> = Arc::new(SqlxIdempotencyStore::new(
        sdkwork_web_store_sqlx::WebStorePool::Postgres(pg_pool),
    ));
    let mut framework_builder = WebFrameworkBuilder::new(resolver);
    if request_profile.environment == WebEnvironment::Prod {
        framework_builder = framework_builder.production_defaults().optional_features(
            WebFrameworkOptionalFeatures::production_sqlx().control_plane_standalone(),
        );
    }
    framework_builder = framework_builder
        .idempotency_store(idempotency_store)
        .request_timeout(DEFAULT_BUSINESS_HANDLER_TIMEOUT)
        .profile(request_profile)
        .security_policy(security_policy)
        .authorization_policy(Arc::new(IamAuthorizationPolicy::new(
            profile.assembly.route_manifest.clone(),
        )))
        .tenant_isolation_policy(Arc::new(WebServerTenantIsolationPolicy))
        .metrics_registry(metrics.clone())
        .audit_emitter(profile.audit_emitter.clone())
        .security_event_emitter(profile.security_event_emitter.clone());
    let mut assembly: ComposedApiAssembly = profile.assembly;
    assembly.readiness_check = readiness_check;
    assembly.router = with_problem_correlation(assembly.router);
    let hosted = assembly.into_hosted(framework_builder);
    info!(
        route_count = hosted.route_manifest.routes().len(),
        permission_count = hosted.permission_catalog.len(),
        "assembled Web Server standalone API profile"
    );
    let router = hosted.router;
    Ok(match app_shell {
        Some(config) => config.mount(router),
        None => router,
    })
}

pub async fn run_database_migrate_only() -> Result<(), String> {
    sdkwork_api_webserver_assembly::migrate_database_from_env()
        .await
        .map_err(|error| error.to_string())?;
    info!("Web database migration completed");
    // The IAM module is owned by the standalone gateway; its lifecycle
    // (init + auto-migrate + drift) runs through the same process-shared pool.
    let iam_pool = process_shared_database_pool().ok_or_else(|| {
        "standalone gateway requires the process-shared database pool; bootstrap the database lifecycle first".to_string()
    })?;
    sdkwork_api_iam_assembly::bootstrap_database_with_pool(iam_pool)
        .await
        .map_err(|error| format!("IAM database bootstrap failed: {error}"))?;
    info!("IAM database migration completed");
    Ok(())
}
