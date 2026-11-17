//! Application surface registry: the imported-module surfaces this edge
//! materializes, once folded into the tenant-facing `webserver_application` /
//! `webserver_application_host` inventory.
//!
//! **RETIRED WRITER.** The derivation below is still the accurate reading of
//! which application surfaces an edge materializes, but the module no longer
//! writes the inventory. `webserver_application` was downgraded to a
//! site-carrier record that must reference the Deploy application it carries
//! (`deploy_application_id`, unique per live row, migration `0031`);
//! `sdkwork-deployments` owns application identity and publishing. An imported
//! module configuration cannot know that reference, so the startup hook reports
//! the retirement instead of creating rows, and
//! [`reconcile_application_registry`] refuses. The `webserver_application_host`
//! inventory moves with it: its rows hang off the carrier row.
//!
//! What remains here is the pure observation helpers
//! ([`derive_applications`], [`application_from_import`]) that turn a
//! materialized import into the facts an inventory would record. They are
//! exported and tested so the Deploy-side reconciliation can adopt the same
//! reading of an import instead of re-deriving it.

use std::collections::BTreeMap;

use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_utils_rust::slugify;
use sdkwork_webserver_core::{
    is_nginx_conf_path, load_module_import_app_config, normalize_tls_server_name,
    resolve_import_environment, resolve_import_profile, sidecar_profile_environment,
    ResourceConfig, WebServerAppConfig, WebserverModuleImport,
};
use sqlx::PgPool;

/// Opt-out switch for the (retired) reconciliation pass.
///
/// It still gates whether the startup hook logs its retirement notice; the pass
/// itself writes nothing either way. Kept so an operator's existing
/// `SDKWORK_WEBSERVER_APPLICATION_REGISTRY_RECONCILE=disabled` setting keeps
/// behaving as a mute switch.
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
///
/// RETIRED writer: the summary type survives because it is part of the exported
/// surface of [`reconcile_application_registry`], which now always refuses. No
/// caller observes non-zero counts any more.
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

/// Refusal text returned by the retired inventory writer.
///
/// Kept as a constant so the startup hook and any out-of-tree caller report the
/// same sentence, and so the retirement is greppable from the log alone.
pub const APPLICATION_REGISTRY_RETIRED: &str = "the webserver application surface registry no longer writes webserver_application rows: application identity and publishing are owned by sdkwork-deployments (deploy_app)";

/// RETIRED: reconcile the observed imports into `webserver_application` /
/// `webserver_application_host` for one tenant.
///
/// `webserver_application` is downgraded to a site-carrier record that MUST
/// reference the Deploy application it carries (`deploy_application_id`,
/// migration `0031`). An edge that derives its inventory from imported module
/// configuration has no way to know that reference, so it must not create
/// carrier rows at all — doing so is exactly the "webserver owns an application
/// identity" defect this retirement removes. The imported-surface inventory now
/// belongs to the Deploy application bindings (`deploy_app_binding`), which the
/// data plane already resolves per request in `deploy_fallback`.
///
/// The function is kept — and refuses — instead of being deleted so a caller
/// compiled against the previous behaviour gets a named error instead of
/// silently writing orphaned rows. Every write statement it used to run is gone.
pub async fn reconcile_application_registry(
    _pool: &PgPool,
    _id_generator: &SnowflakeIdGenerator,
    _tenant_id: i64,
    _applications: &[ObservedApplication],
) -> Result<ApplicationRegistrySummary, String> {
    Err(APPLICATION_REGISTRY_RETIRED.to_owned())
}

fn reconcile_enabled() -> bool {
    crate::reconcile_support::reconcile_enabled_for(APPLICATION_REGISTRY_RECONCILE_ENV)
}

/// Startup hook: report that the imported-application inventory is no longer
/// written by this edge.
///
/// The previous implementation derived one `webserver_application` row (plus its
/// `webserver_application_host` rows) per materialized module import. That is an
/// application identity minted by the edge, which the module no longer owns:
/// every carrier row must reference its Deploy application
/// (`deploy_application_id`), and an imported module configuration cannot supply
/// that reference. Writing the rows would either invent an identity or write an
/// orphan, so the hook is a no-op that says where the inventory moved.
///
/// Never fails the process: an edge that cannot reconcile an inventory must
/// still serve, and this pass now touches no data at all.
#[cfg(feature = "management")]
pub async fn reconcile_application_registry_at_startup() {
    if !reconcile_enabled() {
        tracing::info!(
            env = APPLICATION_REGISTRY_RECONCILE_ENV,
            "application registry reconciliation is disabled"
        );
        return;
    }
    tracing::warn!(
        env = APPLICATION_REGISTRY_RECONCILE_ENV,
        reason = APPLICATION_REGISTRY_RETIRED,
        "application surface registry reconciliation is retired; nothing was written. \
         Application identity and publishing belong to sdkwork-deployments (deploy_app); \
         reconcile imported surfaces against deploy_app_binding there"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_webserver_core::config::{CertificateConfig, VirtualHostConfig};

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
            dynamic_root: None,
        }
    }

    fn virtual_host(id: &str, server_names: &[&str]) -> VirtualHostConfig {
        VirtualHostConfig {
            id: id.to_owned(),
            listener_refs: Vec::new(),
            server_names: server_names.iter().map(|name| name.to_string()).collect(),
            routes: Vec::new(),
            rewrite_phase: Vec::new(),
            security_headers: None,
            compression: None,
            mime_types: None,
            default_type: None,
            error_pages: Vec::new(),
            recursive_error_pages: false,
            cache_policy: None,
            redirect_policy: None,
            limit_req_status: None,
            limit_conn_status: None,
            max_ranges: None,
            merge_slashes: true,
            underscores_in_headers: false,
            ignore_invalid_headers: true,
            charset: None,
            autoindex: None,
            http2_chunk_size: None,
            referer: None,
            userid: None,
            browser: None,
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
            maps: Vec::new(),
            geo: Vec::new(),
            geoip: None,
            stream_geoip: None,
            split_clients: Vec::new(),
            mime_types: None,
            default_type: None,
            gzip: Default::default(),
            limit_req_zones: Vec::new(),
            limit_conn_zones: Vec::new(),
            stream_limit_conn_zones: Vec::new(),
            cache_zone_names: Vec::new(),
            stream_maps: Vec::new(),
            stream_split_clients: Vec::new(),
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
            mail: Default::default(),
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
