//! Application surface registry reconciliation: the imported-module
//! applications this edge actually materializes, folded into the tenant-facing
//! `webserver_application` / `webserver_application_host` inventory.
//!
//! Every module import the edge materializes is one application — a PC desktop
//! surface, an H5 mobile browser surface, or both (`SDKWORK_WEBSERVER_SPEC.md`
//! §17.3.2 universal import plane). Nothing else in the system derives that
//! inventory: before this module existed, the imported module edges answered
//! for dozens of hostnames without a single `webserver_application` row
//! recording which application, on which server block, answers for which
//! hostname in which lifecycle environment.
//!
//! The module follows the three rules `served_domains` established, because
//! both reconcile the same authority:
//!
//! * **The edge config is the authority, not a source of suggestions.** Every
//!   row written here corresponds to an import the configuration materializes
//!   and a `server_name` one of its server blocks declares. The module never
//!   invents an application or a hostname.
//! * **Reconcile, not append.** Rows this module created in a previous run —
//!   keyed by `metadata.source = "served-config"`, `metadata.importId` and the
//!   deterministic `imported-<importId>` slug — are retired when the
//!   configuration stops declaring them. Rows anybody else created are
//!   structurally out of reach: every upsert adopts an existing row only when
//!   it carries this module's `metadata.source`, and a foreign row is reported
//!   through the summary instead of being silently taken over.
//! * **An incomplete observation never writes.** One unreadable import aborts
//!   the whole pass (the merged data plane the edge would serve is built from
//!   the same imports, so a partial observation is not a truth the inventory
//!   may record), and an empty derived set leaves everything untouched — the
//!   same "must never be recorded as nothing is served" rule as
//!   `served_domains`.
//!
//! `admin_reset.rs` and `served_domains.rs` set the precedent this module
//! follows: an operator/startup provisioning concern in the gateway crate
//! writes its own statements, because it is not part of any tenant-facing API
//! surface.

use std::collections::BTreeMap;

use sdkwork_database_id::{uuid_v4, SnowflakeIdGenerator};
use sdkwork_utils_rust::slugify;
use sdkwork_webserver_core::{
    configured_module_imports, is_nginx_conf_path, load_module_import_app_config,
    normalize_tls_server_name, resolve_import_environment, resolve_import_profile,
    sidecar_profile_environment, ResourceConfig, WebServerAppConfig, WebserverModuleImport,
};
use sqlx::PgPool;

use crate::reconcile_support::SERVED_CONFIG_METADATA_SOURCE;

/// Opt-out switch. Reconciliation is on by default: an edge that materializes
/// module imports but has no application inventory is the defect this module
/// exists to remove.
pub const APPLICATION_REGISTRY_RECONCILE_ENV: &str =
    "SDKWORK_WEBSERVER_APPLICATION_REGISTRY_RECONCILE";

/// `metadata.source` of every row this module owns — the same keyspace
/// `served_domains` retires against, so an operator- or application-created
/// row is out of reach for both passes alike.
///
/// Lifecycle environments an imported surface can be materialized for, and the
/// access surfaces an application can declare. Closed vocabularies pinned by
/// the database (`chk_webserver_application_host_environment`,
/// `chk_webserver_application_access_surfaces` element shape); the constants
/// keep the Rust side from drifting from them.
pub const ACCESS_SURFACE_PC: &str = "PC";
pub const ACCESS_SURFACE_H5: &str = "H5";

/// One server block of one import answering for one hostname.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObservedHost {
    /// Server block (virtual host) id inside the import configuration.
    pub vhost_id: String,
    /// Normalized lowercase ASCII, wildcard prefix preserved (`*.a.example.com`).
    pub hostname: String,
    /// `EXACT` or `WILDCARD`.
    pub hostname_type: &'static str,
}

/// One observed imported application and everything it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedApplication {
    /// The configured module import id (for the universal import plane,
    /// `<module>-<profile>-<environment>`).
    pub import_id: String,
    /// Deterministic per-tenant application slug: `imported-<importId>`.
    pub slug: String,
    /// Display name derived from the module segment of the import id.
    pub name: String,
    /// `SPA_WEB` when a static resource declares an SPA fallback,
    /// `STATIC_WEB` when it only serves files, `API` when the import only
    /// proxies — the classification reads declared configuration facts only.
    pub application_kind: &'static str,
    /// Deployment profile the import materializes for (`standalone`/`cloud`).
    pub profile: String,
    /// Lifecycle environment the import materializes for.
    pub environment: String,
    /// `["PC"]`, `["PC", "H5"]`, `["H5"]` or empty, in that order.
    pub access_surfaces: Vec<String>,
    /// Server blocks and the hostnames each answers for, deduplicated by
    /// hostname, ordered deterministically.
    pub hosts: Vec<ObservedHost>,
}

/// What one reconciliation pass did. Created and already-present counts are
/// separate so a repeated startup is visibly a no-op rather than an assumed
/// one, mirroring [`crate::served_domains::ServedDomainsSummary`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplicationRegistrySummary {
    pub observed_imports: usize,
    pub observed_hosts: usize,
    pub applications_created: usize,
    pub applications_existing: usize,
    pub applications_retired: usize,
    pub hosts_created: usize,
    pub hosts_existing: usize,
    pub hosts_retired: usize,
    /// Live rows the configuration collided with that this module did not
    /// create (a slug or hostname an operator's own row already holds).
    /// Reported rather than adopted silently: taking over somebody else's row
    /// would be indistinguishable from a config bug.
    pub rows_skipped_foreign_owner: usize,
}

/// `metadata` document for a reconciled application row.
fn application_metadata(import_id: &str, profile: &str, environment: &str) -> String {
    serde_json::json!({
        "source": SERVED_CONFIG_METADATA_SOURCE,
        "importId": import_id,
        "profile": profile,
        "environment": environment,
    })
    .to_string()
}

/// `metadata` document for a reconciled application-host row.
fn host_metadata(import_id: &str) -> String {
    serde_json::json!({
        "source": SERVED_CONFIG_METADATA_SOURCE,
        "importId": import_id,
    })
    .to_string()
}

/// The access surfaces and application kind of one import, read off its
/// declared static resources.
fn access_surfaces_and_kind(app: &WebServerAppConfig) -> (Vec<String>, &'static str) {
    let mut surfaces: Vec<String> = Vec::new();
    let mut has_static = false;
    let mut has_spa_fallback = false;
    for resource in &app.resources {
        let ResourceConfig::Static {
            root,
            h5_root,
            spa_fallback,
            ..
        } = resource
        else {
            continue;
        };
        has_static = true;
        if spa_fallback.is_some() {
            has_spa_fallback = true;
        }
        if !root.trim().is_empty() && !surfaces.iter().any(|surface| surface == ACCESS_SURFACE_PC) {
            surfaces.push(ACCESS_SURFACE_PC.to_owned());
        }
        if h5_root
            .as_deref()
            .is_some_and(|root| !root.trim().is_empty())
            && !surfaces.iter().any(|surface| surface == ACCESS_SURFACE_H5)
        {
            surfaces.push(ACCESS_SURFACE_H5.to_owned());
        }
    }
    let kind = if has_spa_fallback {
        "SPA_WEB"
    } else if has_static {
        "STATIC_WEB"
    } else {
        "API"
    };
    (surfaces, kind)
}

/// The module segment of a qualified import id: `<module>-<profile>-<environment>`
/// folds back to `<module>` for display; anything else stays whole.
fn module_segment<'a>(import_id: &'a str, profile: &str, environment: &str) -> &'a str {
    let suffix = format!("-{profile}-{environment}");
    import_id.strip_suffix(&suffix).unwrap_or(import_id)
}

/// Derive one import's registry facts from its materialized configuration.
///
/// Pure on purpose: the surface/kind classification, the slug rule, and the
/// hostname typing are the decisions worth testing, and none of them needs a
/// database to be exercised. The caller is responsible for having loaded `app`
/// from `import` (the same loader the data plane uses).
pub fn application_from_import(
    import: &WebserverModuleImport,
    app: &WebServerAppConfig,
    profile: &str,
    environment: &str,
) -> ObservedApplication {
    let (access_surfaces, application_kind) = access_surfaces_and_kind(app);
    let module = module_segment(&import.id, profile, environment);
    // `display_name`-style truncations are defensive: import ids are
    // `^[a-z0-9_-]+$` and short, but the columns are bounded and the
    // reconciler must never fail a pass over presentation text.
    let name = {
        let candidate = format!("Imported module {module}");
        candidate.chars().take(100).collect::<String>()
    };
    let mut hosts = BTreeMap::new();
    for virtual_host in &app.virtual_hosts {
        for raw in &virtual_host.server_names {
            // The same normalization the domain inventory and the certificate
            // index use, so a hostname recorded here is a name the data plane
            // can actually select on. `_` (nginx catch-all) and IP literals
            // are legitimate `server_name`s with no host row to record.
            let Some(normalized) = normalize_tls_server_name(raw) else {
                continue;
            };
            let wildcard = normalized.starts_with("*.");
            hosts.entry(normalized.clone()).or_insert(ObservedHost {
                vhost_id: virtual_host.id.clone(),
                hostname: normalized,
                hostname_type: if wildcard { "WILDCARD" } else { "EXACT" },
            });
        }
    }
    ObservedApplication {
        import_id: import.id.clone(),
        slug: slugify(&format!("imported-{}", import.id)),
        name,
        application_kind,
        profile: profile.to_owned(),
        environment: environment.to_owned(),
        access_surfaces,
        hosts: hosts.into_values().collect(),
    }
}

/// The profile and lifecycle environment one import materializes for.
///
/// A stock nginx sidecar declares both in its `nginx.<profile>.<environment>.conf`
/// name (the universal import plane commissions every environment's sidecar in
/// one process); a layout v3 TOML import resolves them from the process
/// configuration exactly as the import loader does.
fn import_profile_environment(import: &WebserverModuleImport) -> Result<(String, String), String> {
    if is_nginx_conf_path(&import.path) {
        return sidecar_profile_environment(&import.path)
            .map(|(profile, environment)| (profile.to_owned(), environment.to_owned()))
            .ok_or_else(|| {
                format!(
                    "module import `{}`: the sidecar name does not declare `<profile>.<environment>`",
                    import.id
                )
            });
    }
    let profile = resolve_import_profile(import).map_err(|error| error.to_string())?;
    let environment = resolve_import_environment(import).map_err(|error| error.to_string())?;
    Ok((profile, environment))
}

/// Enumerate every enabled import the edge materializes and derive its
/// registry facts, in deterministic (slug) order.
///
/// All-or-nothing: one unreadable or undeclarative import aborts the pass,
/// because the merged data plane is built from the same imports — recording a
/// partial observation would write an inventory the edge does not back.
pub fn derive_applications(
    imports: &[WebserverModuleImport],
) -> Result<Vec<ObservedApplication>, String> {
    let mut applications = Vec::new();
    for import in imports.iter().filter(|import| import.enabled) {
        let app = load_module_import_app_config(import).map_err(|error| error.to_string())?;
        let (profile, environment) = import_profile_environment(import)?;
        applications.push(application_from_import(
            import,
            &app,
            &profile,
            &environment,
        ));
    }
    applications.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(applications)
}

fn next_id(generator: &SnowflakeIdGenerator) -> Result<i64, String> {
    generator
        .generate()
        .map_err(|error| format!("snowflake id generation failed: {error}"))
}

/// Reconcile the observed imports into `webserver_application` /
/// `webserver_application_host` for one tenant, in one transaction.
///
/// # Why the empty case returns instead of writing
///
/// The retirement steps are only reachable with a non-empty observed set. An
/// edge that derived no applications at all would otherwise retire every row
/// it ever created — and since a missing runtime config file also derives as
/// "no imports", empty must mean "leave everything alone", never "serve
/// nothing" (the same rule as `served_domains`).
pub async fn reconcile_application_registry(
    pool: &PgPool,
    id_generator: &SnowflakeIdGenerator,
    tenant_id: i64,
    applications: &[ObservedApplication],
) -> Result<ApplicationRegistrySummary, String> {
    let mut summary = ApplicationRegistrySummary {
        observed_imports: applications.len(),
        observed_hosts: applications.iter().map(|app| app.hosts.len()).sum(),
        ..ApplicationRegistrySummary::default()
    };
    if applications.is_empty() {
        return Ok(summary);
    }

    let observed_slugs = applications
        .iter()
        .map(|application| application.slug.clone())
        .collect::<Vec<_>>();

    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| format!("begin application registry reconciliation: {error}"))?;

    for application in applications {
        let metadata = application_metadata(
            &application.import_id,
            &application.profile,
            &application.environment,
        );
        // The slug is the deterministic ownership key (`uk_webserver_application_slug`),
        // so the upsert resurrects a row the reconciler itself soft-deleted in a
        // previous pass — the same config-round-trip rule the host uniqueness
        // comment documents. The `WHERE` predicate on the update refuses to
        // adopt a row this module did not create.
        let application_row: Option<(i64, bool)> = sqlx::query_as(
            "INSERT INTO webserver_application (
                 id, uuid, tenant_id, organization_id, data_scope, user_id,
                 name, slug, description, application_kind, status, site_id,
                 default_environment, access_surfaces, metadata,
                 created_at, updated_at, version
             ) VALUES ($1, $2, $3, 0, 1, NULL,
                 $4, $5, $6, $7, 1, NULL,
                 $8, $9::jsonb, $10::jsonb,
                 NOW(), NOW(), 0)
             ON CONFLICT (tenant_id, slug) DO UPDATE SET
                 application_kind = EXCLUDED.application_kind,
                 default_environment = EXCLUDED.default_environment,
                 access_surfaces = EXCLUDED.access_surfaces,
                 metadata = EXCLUDED.metadata,
                 status = 1,
                 deleted_at = NULL,
                 deleted_by = NULL,
                 updated_at = NOW(),
                 version = webserver_application.version + 1
                 WHERE webserver_application.metadata->>'source' = $11
             RETURNING id, (xmax = 0) AS inserted",
        )
        .bind(next_id(id_generator)?)
        .bind(uuid_v4())
        .bind(tenant_id)
        .bind(&application.name)
        .bind(&application.slug)
        .bind(format!(
            "Reconciled from the served edge import `{}` ({}.{}).",
            application.import_id, application.profile, application.environment
        ))
        .bind(application.application_kind)
        .bind(&application.environment)
        .bind(
            serde_json::to_value(&application.access_surfaces)
                .map_err(|error| format!("encode access surfaces: {error}"))?,
        )
        .bind(metadata)
        .bind(SERVED_CONFIG_METADATA_SOURCE)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            format!(
                "upsert reconciled application {}: {error}",
                application.import_id
            )
        })?;

        let Some((application_id, application_inserted)) = application_row else {
            summary.rows_skipped_foreign_owner += 1;
            tracing::warn!(
                tenant_id,
                import_id = %application.import_id,
                slug = %application.slug,
                "a live application row with this slug was not created by the reconciler; leaving it and skipping its host inventory"
            );
            continue;
        };
        if application_inserted {
            summary.applications_created += 1;
        } else {
            summary.applications_existing += 1;
        }

        let observed_environments = vec![application.environment.clone(); application.hosts.len()];
        let observed_hostnames = application
            .hosts
            .iter()
            .map(|host| host.hostname.clone())
            .collect::<Vec<_>>();
        let host_metadata = host_metadata(&application.import_id);

        for host in &application.hosts {
            // One live host row per (application, environment, hostname);
            // the partial unique index releases the name on soft delete so a
            // hostname can return after a config round-trip. The `WHERE`
            // predicate refuses to adopt a foreign row.
            let host_row: Option<(i64, bool)> = sqlx::query_as(
                "INSERT INTO webserver_application_host (
                     id, uuid, tenant_id, organization_id, data_scope, user_id,
                     app_id, hostname, hostname_type, environment,
                     vhost_id, path_prefix, status, metadata,
                     created_at, updated_at, version
                 ) VALUES ($1, $2, $3, 0, 1, NULL,
                     $4, $5, $6, $7,
                     $8, '/', 1, $9::jsonb,
                     NOW(), NOW(), 0)
                 ON CONFLICT (tenant_id, app_id, environment, hostname) WHERE deleted_at IS NULL
                 DO UPDATE SET
                     vhost_id = EXCLUDED.vhost_id,
                     hostname_type = EXCLUDED.hostname_type,
                     status = 1,
                     metadata = EXCLUDED.metadata,
                     updated_at = NOW(),
                     version = webserver_application_host.version + 1
                     WHERE webserver_application_host.metadata->>'source' = $10
                 RETURNING id, (xmax = 0) AS inserted",
            )
            .bind(next_id(id_generator)?)
            .bind(uuid_v4())
            .bind(tenant_id)
            .bind(application_id)
            .bind(&host.hostname)
            .bind(host.hostname_type)
            .bind(&application.environment)
            .bind(&host.vhost_id)
            .bind(host_metadata.as_str())
            .bind(SERVED_CONFIG_METADATA_SOURCE)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                format!(
                    "upsert reconciled application host {} for {}: {error}",
                    host.hostname, application.import_id
                )
            })?;
            match host_row {
                Some((_, true)) => summary.hosts_created += 1,
                Some((_, false)) => summary.hosts_existing += 1,
                None => summary.rows_skipped_foreign_owner += 1,
            }
        }

        // Retire this application's reconciled host rows whose (environment,
        // hostname) is no longer observed: removing a `server_name` removes
        // its host row, and never a row somebody else created.
        summary.hosts_retired += sqlx::query(
            "UPDATE webserver_application_host host
             SET deleted_at = NOW(), updated_at = NOW(), version = host.version + 1
             WHERE host.tenant_id = $1
               AND host.app_id = $2
               AND host.deleted_at IS NULL
               AND host.metadata->>'source' = $3
               AND NOT EXISTS (
                   SELECT 1
                   FROM unnest($4::text[], $5::text[]) AS observed(environment, hostname)
                   WHERE observed.environment = host.environment
                     AND observed.hostname = host.hostname
               )",
        )
        .bind(tenant_id)
        .bind(application_id)
        .bind(SERVED_CONFIG_METADATA_SOURCE)
        .bind(&observed_environments)
        .bind(&observed_hostnames)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("retire stale reconciled hosts: {error}"))?
        .rows_affected() as usize;
    }

    // Retire reconciled applications whose import is no longer materialized,
    // then their remaining host rows (a soft delete does not cascade).
    let retired_application_ids = sqlx::query_scalar::<_, i64>(
        "UPDATE webserver_application app
         SET deleted_at = NOW(), updated_at = NOW(), version = app.version + 1
         WHERE app.tenant_id = $1
           AND app.deleted_at IS NULL
           AND app.metadata->>'source' = $2
           AND NOT (app.slug = ANY($3))
         RETURNING app.id",
    )
    .bind(tenant_id)
    .bind(SERVED_CONFIG_METADATA_SOURCE)
    .bind(&observed_slugs)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| format!("retire stale reconciled applications: {error}"))?;
    summary.applications_retired = retired_application_ids.len();

    if !retired_application_ids.is_empty() {
        summary.hosts_retired += sqlx::query(
            "UPDATE webserver_application_host host
             SET deleted_at = NOW(), updated_at = NOW(), version = host.version + 1
             WHERE host.tenant_id = $1
               AND host.deleted_at IS NULL
               AND host.metadata->>'source' = $2
               AND host.app_id = ANY($3)",
        )
        .bind(tenant_id)
        .bind(SERVED_CONFIG_METADATA_SOURCE)
        .bind(&retired_application_ids)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("retire hosts of stale reconciled applications: {error}"))?
        .rows_affected() as usize;
    }

    transaction
        .commit()
        .await
        .map_err(|error| format!("commit application registry reconciliation: {error}"))?;
    Ok(summary)
}

fn reconcile_enabled() -> bool {
    crate::reconcile_support::reconcile_enabled_for(APPLICATION_REGISTRY_RECONCILE_ENV)
}

/// Startup hook: derive the imported-application inventory from the effective
/// module-import configuration and reconcile it.
///
/// Never fails the process, for the same reason `served_domains` does not: an
/// edge whose application inventory cannot be reconciled must still serve.
/// Every outcome is logged with its counts, so an inventory that stayed empty
/// is diagnosable from the log alone.
#[cfg(feature = "management")]
pub async fn reconcile_application_registry_at_startup() {
    if !reconcile_enabled() {
        tracing::info!(
            env = APPLICATION_REGISTRY_RECONCILE_ENV,
            "application registry reconciliation is disabled"
        );
        return;
    }
    let Some(context) =
        crate::reconcile_support::startup_reconcile_context("the application surface registry")
            .await
    else {
        return;
    };

    let imports = match configured_module_imports() {
        Ok(imports) => imports,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "application registry reconciliation could not enumerate the module imports; the inventory was left untouched"
            );
            return;
        }
    };
    let applications = match derive_applications(&imports) {
        Ok(applications) => applications,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "application registry reconciliation could not materialize every import; the inventory was left untouched"
            );
            return;
        }
    };
    if applications.is_empty() {
        // Explicitly not a wipe: no materialized imports is either the truth
        // or a configuration-resolution miss, and the latter must never be
        // recorded as "nothing is served".
        tracing::info!(
            "no module imports are materialized; the application surface registry was left untouched"
        );
        return;
    }

    match reconcile_application_registry(
        &context.pool,
        &context.id_generator,
        context.tenant_id,
        &applications,
    )
    .await
    {
        Ok(summary) => tracing::info!(
            tenant_id = context.tenant_id,
            observed_imports = summary.observed_imports,
            observed_hosts = summary.observed_hosts,
            applications_created = summary.applications_created,
            applications_existing = summary.applications_existing,
            applications_retired = summary.applications_retired,
            hosts_created = summary.hosts_created,
            hosts_existing = summary.hosts_existing,
            hosts_retired = summary.hosts_retired,
            rows_skipped_foreign_owner = summary.rows_skipped_foreign_owner,
            "reconciled the application surface registry"
        ),
        Err(error) => tracing::warn!(
            error = %error,
            "application registry reconciliation failed; the edge keeps serving with the previous inventory"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_core::config::{CertificateConfig, CertificateSource, VirtualHostConfig};

    fn import(id: &str) -> WebserverModuleImport {
        WebserverModuleImport {
            id: id.to_owned(),
            path: std::path::PathBuf::from("/srv/module/deployments/webserver"),
            profile: None,
            enabled: true,
            required: false,
            probe_upstreams: false,
        }
    }

    /// A materializable nginx sidecar whose file name controls the
    /// `<profile>.<environment>` the import loader derives.
    fn write_conf_sidecar(dir: &std::path::Path, file_name: &str) -> std::path::PathBuf {
        std::fs::create_dir_all(dir).expect("materialize sidecar directory");
        let path = dir.join(file_name);
        std::fs::write(
            &path,
            "user sdkwork;\nevents {}\nhttp { server { listen 80; server_name sidecar.example.com; location / { return 200; } } }\n",
        )
        .expect("write sidecar");
        path
    }

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "sdkwork-appreg-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    fn static_resource(
        id: &str,
        root: &str,
        h5_root: Option<&str>,
        spa_fallback: Option<&str>,
    ) -> ResourceConfig {
        ResourceConfig::Static {
            id: id.to_owned(),
            root: root.to_owned(),
            h5_root: h5_root.map(str::to_owned),
            index_files: vec!["index.html".to_owned()],
            spa_fallback: spa_fallback.map(str::to_owned),
            follow_symlinks: false,
            strip_prefix: false,
        }
    }

    fn virtual_host(id: &str, server_names: &[&str]) -> VirtualHostConfig {
        VirtualHostConfig {
            id: id.to_owned(),
            listener_refs: Vec::new(),
            server_names: server_names.iter().map(|name| name.to_string()).collect(),
            routes: Vec::new(),
            security_headers: None,
            compression: None,
            error_pages: Vec::new(),
            recursive_error_pages: false,
            cache_policy: None,
        }
    }

    fn app(
        resources: Vec<ResourceConfig>,
        virtual_hosts: Vec<VirtualHostConfig>,
    ) -> WebServerAppConfig {
        WebServerAppConfig {
            tunnel: None,
            schema_version: 1,
            kind: "test".to_owned(),
            app_key: "test".to_owned(),
            nginx: Default::default(),
            gzip: Default::default(),
            limit_req_zones: Vec::new(),
            limit_conn_zones: Vec::new(),
            resolution_cache: None,
            limits: Default::default(),
            listeners: Vec::new(),
            certificates: Vec::<CertificateConfig>::new(),
            tls_policies: Vec::new(),
            resolvers: Vec::new(),
            resources,
            upstreams: Vec::new(),
            virtual_hosts,
            streams: Vec::new(),
            proxy_cache: Default::default(),
            app_domain_fallback: None,
            usage_metering: None,
            observability: Default::default(),
            deployment: Default::default(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn an_adaptive_web_import_is_spa_web_with_both_surfaces() {
        let module = import("sdkwork-im-cloud-production");
        let observed = application_from_import(
            &module,
            &app(
                vec![static_resource(
                    "web",
                    "/usr/share/sdkwork/im/web/pc",
                    Some("/usr/share/sdkwork/im/web/h5"),
                    Some("/index.html"),
                )],
                vec![virtual_host(
                    "im",
                    &["im.sdkwork.com", "*.im-dev.sdkwork.com"],
                )],
            ),
            "cloud",
            "production",
        );
        assert_eq!(observed.application_kind, "SPA_WEB");
        assert_eq!(observed.access_surfaces, vec!["PC", "H5"]);
        assert_eq!(observed.slug, "imported-sdkwork-im-cloud-production");
        assert_eq!(observed.name, "Imported module sdkwork-im");
        assert_eq!(observed.hosts.len(), 2);
        assert_eq!(observed.hosts[0].hostname, "*.im-dev.sdkwork.com");
        assert_eq!(observed.hosts[0].hostname_type, "WILDCARD");
        assert_eq!(observed.hosts[1].hostname, "im.sdkwork.com");
        assert_eq!(observed.hosts[1].hostname_type, "EXACT");
        assert_eq!(observed.hosts[1].vhost_id, "im");
    }

    #[test]
    fn a_pc_only_static_import_is_static_web_with_one_surface() {
        let module = import("docs");
        let observed = application_from_import(
            &module,
            &app(
                vec![static_resource("site", "/srv/docs", None, None)],
                vec![virtual_host("docs", &["docs.example.com"])],
            ),
            "standalone",
            "production",
        );
        assert_eq!(observed.application_kind, "STATIC_WEB");
        assert_eq!(observed.access_surfaces, vec!["PC"]);
        assert_eq!(observed.name, "Imported module docs");
    }

    #[test]
    fn a_proxy_only_import_is_api_with_no_surfaces_and_no_hosts_from_invalid_names() {
        let module = import("sdkwork-gateway-standalone-development");
        let observed = application_from_import(
            &module,
            &app(Vec::new(), vec![virtual_host("api", &["_", "127.0.0.1"])]),
            "standalone",
            "development",
        );
        assert_eq!(observed.application_kind, "API");
        assert!(observed.access_surfaces.is_empty());
        assert!(observed.hosts.is_empty());
    }

    #[test]
    fn duplicate_server_names_collapse_to_one_host_keeping_the_first_vhost() {
        let module = import("im");
        let observed = application_from_import(
            &module,
            &app(
                Vec::new(),
                vec![
                    virtual_host("first", &["im.example.com"]),
                    virtual_host("second", &["IM.example.com", "im.example.com."]),
                ],
            ),
            "cloud",
            "test",
        );
        assert_eq!(observed.hosts.len(), 1);
        assert_eq!(observed.hosts[0].vhost_id, "first");
    }

    #[test]
    fn derive_skips_disabled_imports_and_sorts_by_slug() {
        let temp = scratch_dir("derive");
        let _ = std::fs::remove_dir_all(&temp);
        let z_path = write_conf_sidecar(&temp.join("z"), "nginx.cloud.production.conf");
        let a_path = write_conf_sidecar(&temp.join("a"), "nginx.cloud.test.conf");
        let mut z = import("z-module");
        z.path = z_path;
        let mut disabled = import("m-module");
        disabled.enabled = false;
        let mut a = import("a-module");
        a.path = a_path;
        let derived =
            derive_applications(&[z, disabled, a]).expect("derive must not fail on skips");
        assert_eq!(derived.len(), 2, "disabled imports must be skipped");
        assert_eq!(
            derived[0].import_id, "a-module",
            "slug order must be deterministic"
        );
        assert_eq!(derived[0].environment, "test");
        assert_eq!(derived[1].import_id, "z-module");
        assert_eq!(derived[1].environment, "production");
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn a_conf_sidecar_without_profile_environment_aborts_the_derive() {
        let temp = scratch_dir("undeclared");
        let _ = std::fs::remove_dir_all(&temp);
        let mut module = import("legacy");
        module.path = write_conf_sidecar(&temp.join("legacy"), "sidecar.conf");
        let error = derive_applications(&[module]).expect_err("undeclared env must abort");
        assert!(error.contains("does not declare"), "{error}");
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn an_unloadable_import_aborts_the_derive() {
        let mut module = import("broken");
        module.path = std::path::PathBuf::from("/srv/does-not-exist/deployments/webserver");
        let error = derive_applications(&[module]).expect_err("unloadable import must abort");
        assert!(!error.is_empty());
    }
}
