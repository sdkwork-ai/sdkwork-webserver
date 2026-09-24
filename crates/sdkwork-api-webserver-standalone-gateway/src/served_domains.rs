//! Served-domain reconciliation: the hostnames this edge actually serves, folded
//! into the tenant-level root-domain / subdomain inventory.
//!
//! The edge is the only component that knows which names it answers for: they
//! are the `server_name` list of the nginx sidecar it materializes plus the
//! `server_name` lists of every configured module import. Nothing else in the
//! system derives them, so before this module existed the operator-visible
//! domain inventory started empty and stayed empty even though the edge was
//! serving dozens of hostnames.
//!
//! Three rules shape the whole module:
//!
//! * **The edge config is the authority, not a source of suggestions.** Every
//!   row written here corresponds to a name some configured listener answers
//!   for. The module never invents a name — in particular it does not enumerate
//!   "all subdomains" of a root, because that needs a DNS zone transfer and the
//!   edge is not a DNS server. What the configuration serves is what exists.
//! * **Tenant-level means nobody owns it.** Rows are written with
//!   `tenant_id = <platform operator tenant>` and `user_id IS NULL`, which is
//!   what makes them the tenant's infrastructure rather than one operator's
//!   asset, and what makes the backend admin surface — not the tenant console —
//!   the only place they are managed from.
//! * **Reconcile, not append.** Rows this module created in a previous run whose
//!   hostname is no longer served are retired, so removing a `server_name`
//!   removes its domain row. Rows this module did not create (an operator's own
//!   root domain, an application's hostname) are never touched, which is why
//!   every row carries `metadata.source = "served-config"` and only that
//!   keyspace is ever retired.
//!
//! `admin_reset.rs` sets the precedent this follows: an operator/startup
//! provisioning concern in the gateway crate writes its own statements, because
//! it is not part of any tenant-facing API surface.

use std::{collections::BTreeMap, path::PathBuf};

use sdkwork_database_id::{uuid_v4, SnowflakeIdGenerator};
use sdkwork_webserver_core::{
    merged_imports_app_config, normalize_tls_server_name, resolve_nginx_sidecar_path,
    web_platform_operator_tenant_id, ConfigFormat, ConfigLoadOptions, VirtualHostConfig,
    WebServerAppConfig, WebServerConfigLoader,
};
use sqlx::PgPool;

/// Opt-out switch. Reconciliation is on by default: an edge that serves names
/// but has no domain inventory is the defect this module exists to remove.
pub const SERVED_DOMAIN_RECONCILE_ENV: &str = "SDKWORK_WEBSERVER_SERVED_DOMAIN_RECONCILE";

/// `metadata.source` of every row this module owns. Retirement is scoped to
/// exactly this value, so a row an operator or an application created is
/// structurally out of reach.
const SERVED_DOMAIN_SOURCE: &str = "served-config";

/// One observed hostname and whether it is a wildcard.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServedHostname {
    /// Normalized lowercase ASCII, wildcard prefix preserved (`*.a.example.com`).
    pub hostname: String,
    /// `true` when the name starts with `*.`.
    pub wildcard: bool,
}

/// A registrable root domain and the hostnames observed under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedRootDomain {
    pub root: String,
    pub hostnames: Vec<ServedHostname>,
}

/// What one reconciliation pass did. Created and already-present counts are
/// separate so a repeated startup is visibly a no-op rather than an assumed one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServedDomainsSummary {
    pub observed_hostnames: usize,
    pub roots_created: usize,
    pub roots_existing: usize,
    pub hostnames_created: usize,
    pub hostnames_existing: usize,
    pub hostnames_retired: usize,
    pub roots_retired: usize,
    /// Roots whose apex is already registered by a *different* tenant. Reported
    /// rather than skipped silently: apex uniqueness is global, the operator has
    /// to decide, and a silent skip would be indistinguishable from a config bug.
    pub roots_skipped_foreign_tenant: usize,
}

/// Where the observed names came from, for the startup log line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServedNameSources {
    pub sidecar: Option<PathBuf>,
    pub sidecar_names: usize,
    pub import_names: usize,
}

/// The `server_name`s of every virtual host in one configuration document.
fn server_names_of(app: &WebServerAppConfig) -> impl Iterator<Item = &str> {
    app.virtual_hosts
        .iter()
        .flat_map(|VirtualHostConfig { server_names, .. }| server_names.iter())
        .map(String::as_str)
}

/// Collect the served `server_name`s from the effective nginx sidecar plus every
/// configured module import.
///
/// A missing or unreadable source is reported, never fatal: an edge whose
/// sidecar path is unresolvable may still be serving merged module imports, and
/// refusing to reconcile would be a worse outcome than reconciling on the subset
/// that did load. The caller decides what an empty result means — and it must
/// treat it as "leave everything alone", never as "serve nothing".
pub fn collect_served_server_names() -> (Vec<String>, ServedNameSources) {
    let mut names = Vec::new();
    let mut sources = ServedNameSources::default();

    match resolve_nginx_sidecar_path(None) {
        Ok(path) => {
            let loader = WebServerConfigLoader::new();
            let options = ConfigLoadOptions {
                format: Some(ConfigFormat::NginxConf),
                ..ConfigLoadOptions::default()
            };
            match loader.load(&path, &options) {
                Ok(loaded) => {
                    let found = server_names_of(&loaded.app)
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    sources.sidecar_names = found.len();
                    names.extend(found);
                    sources.sidecar = Some(path);
                }
                Err(error) => tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "served-domain reconciliation could not load the nginx sidecar"
                ),
            }
        }
        Err(error) => tracing::warn!(
            error = %error,
            "served-domain reconciliation found no nginx sidecar"
        ),
    }

    match merged_imports_app_config() {
        Ok(Some(app)) => {
            let found = server_names_of(&app).map(str::to_owned).collect::<Vec<_>>();
            sources.import_names = found.len();
            names.extend(found);
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(
            error = %error,
            "served-domain reconciliation could not load the merged module imports"
        ),
    }

    (names, sources)
}

/// The registrable root of a normalized (wildcard-free) hostname, or `None` when
/// the name cannot be one.
///
/// `psl::domain_str` answers with the public-suffix-list apex, which is what
/// makes `server-dev.example.co.uk` fold to `example.co.uk` rather than to
/// `co.uk`. A name that *is* a public suffix (`co.uk`) is rejected: it is not a
/// registrable root, and folding it to its last two labels would create a zone
/// nobody can own. A name whose suffix the list does not carry (an internal
/// domain, a list that lags a new TLD) falls back to the last two labels — a
/// pre-check must never refuse what a later pass would accept.
fn registrable_root(hostname: &str) -> Option<String> {
    if !hostname.contains('.') {
        return None;
    }
    if psl::suffix_str(hostname).is_some_and(|suffix| suffix == hostname) {
        return None;
    }
    match psl::domain_str(hostname) {
        Some(root) if !root.is_empty() => Some(root.to_owned()),
        _ => {
            let mut labels = hostname.rsplit('.');
            let last = labels.next().unwrap_or_default();
            let previous = labels.next()?;
            Some(format!("{previous}.{last}"))
        }
    }
}

/// Fold raw `server_name` values into root domains and their subdomains.
///
/// Pure on purpose: normalization, the root-domain rule, and the wildcard typing
/// are the three decisions worth testing, and none of them needs a database or a
/// file system to be exercised. Values that cannot be a domain (`_`, the nginx
/// catch-all; an IP literal, which no domain row can represent; a malformed
/// label; a single label with no root) are dropped — they are legitimate
/// `server_name`s that simply have no domain to record.
pub fn derive_served_domains<I, S>(names: I) -> Vec<ServedRootDomain>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // BTreeMap keyed by root then hostname: the output order is deterministic,
    // so a reconcile pass over the same configuration writes the same rows in
    // the same order and two startup logs are comparable.
    let mut roots: BTreeMap<String, BTreeMap<String, ServedHostname>> = BTreeMap::new();
    for raw in names {
        // `normalize_tls_server_name` is the same function configuration
        // validation and the certificate index use, so a name that reaches a
        // domain row here is a name the data plane can already select on. It
        // rejects IP literals, which is exactly the set a domain row cannot hold.
        let Some(normalized) = normalize_tls_server_name(raw.as_ref()) else {
            continue;
        };
        let bare = normalized
            .strip_prefix("*.")
            .unwrap_or(normalized.as_str())
            .to_owned();
        // A name is a wildcard exactly when the `*.` prefix was stripped.
        let wildcard = bare.len() != normalized.len();
        let Some(root) = registrable_root(&bare) else {
            continue;
        };
        roots.entry(root).or_default().insert(
            normalized.clone(),
            ServedHostname {
                hostname: normalized,
                wildcard,
            },
        );
    }

    roots
        .into_iter()
        .map(|(root, hostnames)| ServedRootDomain {
            root,
            hostnames: hostnames.into_values().collect(),
        })
        .collect()
}

/// Whether a `SDKWORK_WEBSERVER_SERVED_DOMAIN_RECONCILE` value disables the pass.
///
/// Split out from the environment read so the accepted spellings are testable
/// without mutating a process-global.
fn reconcile_disabled_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "no" | "disabled"
    )
}

fn reconcile_enabled() -> bool {
    std::env::var(SERVED_DOMAIN_RECONCILE_ENV)
        .map(|value| !reconcile_disabled_value(&value))
        .unwrap_or(true)
}

/// Reconcile the served hostnames into `webserver_root_domain` /
/// `webserver_domain` for one tenant.
///
/// # Why the empty case returns instead of writing
///
/// The retirement step is only reachable with a non-empty observed set. An edge
/// that loaded no configuration at all would otherwise retire every row it ever
/// created, which turns a transient config-resolution failure into data loss.
/// The same reasoning is why a root whose apex belongs to another tenant is
/// left whole: global apex uniqueness means adopting it would either overwrite
/// somebody's zone or strand its subdomains on a foreign parent.
pub async fn reconcile_served_domains(
    pool: &PgPool,
    id_generator: &SnowflakeIdGenerator,
    tenant_id: i64,
    roots: &[ServedRootDomain],
) -> Result<ServedDomainsSummary, String> {
    let mut summary = ServedDomainsSummary {
        observed_hostnames: roots.iter().map(|root| root.hostnames.len()).sum(),
        ..ServedDomainsSummary::default()
    };
    if roots.is_empty() {
        return Ok(summary);
    }

    let metadata = serde_json::json!({ "source": SERVED_DOMAIN_SOURCE }).to_string();
    let observed_roots = roots
        .iter()
        .map(|root| root.root.clone())
        .collect::<Vec<_>>();
    let observed_hostnames = roots
        .iter()
        .flat_map(|root| {
            root.hostnames
                .iter()
                .map(|item| item.hostname.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| format!("begin served-domain reconciliation: {error}"))?;

    for root in roots {
        // The apex index is global rather than per tenant, so an existing row for
        // this hostname may belong to somebody else. The `DO UPDATE ... WHERE`
        // predicate makes that case return no row instead of silently adopting
        // or overwriting a foreign root.
        let root_row: Option<(i64, bool)> = sqlx::query_as(
            "INSERT INTO webserver_root_domain (
                 id, uuid, tenant_id, organization_id, hostname, display_name,
                 status, metadata, created_at, updated_at, version
             ) VALUES ($1, $2, $3, 0, $4, $5, 1, $6::jsonb, NOW(), NOW(), 0)
             ON CONFLICT (hostname) WHERE deleted_at IS NULL
             DO UPDATE SET updated_at = NOW(),
                           version = webserver_root_domain.version + 1
                 WHERE webserver_root_domain.tenant_id = $3
             RETURNING id, (xmax = 0) AS inserted",
        )
        .bind(next_id(id_generator)?)
        .bind(uuid_v4())
        .bind(tenant_id)
        .bind(&root.root)
        .bind(format!("Served root domain {}", root.root))
        .bind(&metadata)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| format!("upsert served root domain {}: {error}", root.root))?;

        let Some((root_id, root_inserted)) = root_row else {
            summary.roots_skipped_foreign_tenant += 1;
            tracing::warn!(
                tenant_id,
                root = %root.root,
                "served root domain is already registered by another tenant; leaving it and its subdomains alone"
            );
            continue;
        };
        if root_inserted {
            summary.roots_created += 1;
        } else {
            summary.roots_existing += 1;
        }

        for hostname in &root.hostnames {
            // `user_id` stays NULL: a served subdomain is tenant-level
            // infrastructure, not an operator's personal asset. `VERIFIED` is
            // not an assumption here — the edge is the thing answering for the
            // name, so its own configuration is the proof, and `verified_at` is
            // written in the same statement because the schema ties the two
            // together.
            let hostname_row: Option<(i64, bool)> = sqlx::query_as(
                "INSERT INTO webserver_domain (
                     id, uuid, tenant_id, organization_id, user_id, root_domain_id,
                     hostname, hostname_type, verification_status, verified_at,
                     status, metadata, created_at, updated_at, version
                 ) VALUES ($1, $2, $3, 0, NULL, $4, $5, $6, 'VERIFIED', NOW(), 1,
                     $7::jsonb, NOW(), NOW(), 0)
                 ON CONFLICT (hostname) WHERE deleted_at IS NULL
                 DO UPDATE SET updated_at = NOW(),
                               version = webserver_domain.version + 1
                     WHERE webserver_domain.tenant_id = $3
                 RETURNING id, (xmax = 0) AS inserted",
            )
            .bind(next_id(id_generator)?)
            .bind(uuid_v4())
            .bind(tenant_id)
            .bind(root_id)
            .bind(&hostname.hostname)
            .bind(if hostname.wildcard {
                "WILDCARD"
            } else {
                "EXACT"
            })
            .bind(&metadata)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| format!("upsert served subdomain {}: {error}", hostname.hostname))?;
            match hostname_row {
                Some((_, true)) => summary.hostnames_created += 1,
                // Already present under this tenant: kept as it is, including the
                // `user_id` an application binding may have given it.
                Some((_, false)) => summary.hostnames_existing += 1,
                // Live under another tenant: reported through the counts, not taken.
                None => {}
            }
        }
    }

    summary.hostnames_retired = sqlx::query(
        "UPDATE webserver_domain
         SET deleted_at = NOW(), updated_at = NOW(), version = version + 1
         WHERE tenant_id = $1
           AND deleted_at IS NULL
           AND metadata->>'source' = $2
           AND NOT (hostname = ANY($3))",
    )
    .bind(tenant_id)
    .bind(SERVED_DOMAIN_SOURCE)
    .bind(&observed_hostnames)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("retire stale served subdomains: {error}"))?
    .rows_affected() as usize;

    // The root is retired last, and only once nothing lives under it: retiring
    // the apex first would strand its subdomains on a soft-deleted parent.
    summary.roots_retired = sqlx::query(
        "UPDATE webserver_root_domain root
         SET deleted_at = NOW(), updated_at = NOW(), version = root.version + 1
         WHERE root.tenant_id = $1
           AND root.deleted_at IS NULL
           AND root.metadata->>'source' = $2
           AND NOT (root.hostname = ANY($3))
           AND NOT EXISTS (
               SELECT 1 FROM webserver_domain domain
               WHERE domain.root_domain_id = root.id
                 AND domain.deleted_at IS NULL
           )",
    )
    .bind(tenant_id)
    .bind(SERVED_DOMAIN_SOURCE)
    .bind(&observed_roots)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("retire stale served root domains: {error}"))?
    .rows_affected() as usize;

    transaction
        .commit()
        .await
        .map_err(|error| format!("commit served-domain reconciliation: {error}"))?;
    Ok(summary)
}

fn next_id(generator: &SnowflakeIdGenerator) -> Result<i64, String> {
    generator
        .generate()
        .map_err(|error| format!("snowflake id generation failed: {error}"))
}

/// Startup hook: read the effective edge configuration, fold it into root
/// domains and subdomains, and reconcile the tenant-level inventory.
///
/// Never fails the process. An edge whose domain inventory cannot be reconciled
/// must still serve, and refusing to boot over a bookkeeping row would turn a
/// missing inventory into an outage — the opposite of what this module is for.
/// Every outcome is logged with its counts, so an inventory that stayed empty is
/// diagnosable from the log alone.
#[cfg(feature = "management")]
pub async fn reconcile_served_domains_at_startup() {
    if !reconcile_enabled() {
        tracing::info!(
            env = SERVED_DOMAIN_RECONCILE_ENV,
            "served-domain reconciliation is disabled"
        );
        return;
    }
    let Some(pool) = sdkwork_database_sqlx::process_shared_database_pool() else {
        tracing::warn!(
            "the shared database pool is unavailable; the served-domain inventory was not reconciled"
        );
        return;
    };
    let sdkwork_database_sqlx::DatabasePool::Postgres(pool, _) = pool;
    let tenant_id = match web_platform_operator_tenant_id().parse::<i64>() {
        Ok(tenant_id) => tenant_id,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "the platform operator tenant id is not numeric; the served-domain inventory was not reconciled"
            );
            return;
        }
    };
    let id_generator = match std::env::var("SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID") {
        Ok(value) => match value.parse::<u16>() {
            Ok(node_id) => match SnowflakeIdGenerator::new(node_id) {
                Ok(generator) => generator,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "invalid snowflake node id; the served-domain inventory was not reconciled"
                    );
                    return;
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID is not a node id; the served-domain inventory was not reconciled"
                );
                return;
            }
        },
        Err(_) => {
            tracing::warn!(
                "SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID is required to allocate domain ids; the served-domain inventory was not reconciled"
            );
            return;
        }
    };

    let (names, sources) = collect_served_server_names();
    let roots = derive_served_domains(names);
    if roots.is_empty() {
        // Explicitly not an error, and explicitly not a wipe: "I could not read
        // the configuration" must never be recorded as "nothing is served".
        tracing::warn!(
            sidecar = ?sources.sidecar,
            sidecar_names = sources.sidecar_names,
            import_names = sources.import_names,
            "no served server names were observed; the domain inventory was left untouched"
        );
        return;
    }

    match reconcile_served_domains(&pool, &id_generator, tenant_id, &roots).await {
        Ok(summary) => tracing::info!(
            tenant_id,
            sidecar = ?sources.sidecar,
            sidecar_names = sources.sidecar_names,
            import_names = sources.import_names,
            root_domains = roots.len(),
            observed_hostnames = summary.observed_hostnames,
            roots_created = summary.roots_created,
            roots_existing = summary.roots_existing,
            hostnames_created = summary.hostnames_created,
            hostnames_existing = summary.hostnames_existing,
            hostnames_retired = summary.hostnames_retired,
            roots_retired = summary.roots_retired,
            roots_skipped_foreign_tenant = summary.roots_skipped_foreign_tenant,
            "reconciled the served-domain inventory"
        ),
        Err(error) => tracing::warn!(
            error = %error,
            "served-domain reconciliation failed; the edge keeps serving with the previous inventory"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derived(names: &[&str]) -> Vec<(String, Vec<String>)> {
        derive_served_domains(names.iter().copied())
            .into_iter()
            .map(|root| {
                (
                    root.root,
                    root.hostnames
                        .into_iter()
                        .map(|item| item.hostname)
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn folds_served_names_onto_their_registrable_root() {
        let folded = derived(&[
            "server-dev.sdkwork.com",
            "server-app-dev.sdkwork.com",
            "server-admin-dev.sdkwork.com",
        ]);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].0, "sdkwork.com");
        assert_eq!(
            folded[0].1,
            vec![
                "server-admin-dev.sdkwork.com",
                "server-app-dev.sdkwork.com",
                "server-dev.sdkwork.com",
            ]
        );
    }

    #[test]
    fn a_multi_label_public_suffix_folds_to_the_registrable_apex() {
        // `co.uk` is a public suffix: the apex is `example.co.uk`, not `co.uk`.
        let folded = derived(&["www.example.co.uk"]);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].0, "example.co.uk");
    }

    #[test]
    fn a_bare_public_suffix_is_not_a_root_domain() {
        assert!(derived(&["co.uk"]).is_empty());
    }

    #[test]
    fn the_root_itself_is_a_hostname_when_it_is_served() {
        let folded = derived(&["sdkwork.com", "www.sdkwork.com"]);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].0, "sdkwork.com");
        assert_eq!(folded[0].1, vec!["sdkwork.com", "www.sdkwork.com"]);
    }

    #[test]
    fn a_wildcard_keeps_its_prefix_and_is_typed_wildcard() {
        let roots = derive_served_domains(["*.shop.example.com"]);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].root, "example.com");
        assert_eq!(roots[0].hostnames.len(), 1);
        assert_eq!(roots[0].hostnames[0].hostname, "*.shop.example.com");
        assert!(roots[0].hostnames[0].wildcard);
    }

    #[test]
    fn a_plain_hostname_is_not_typed_wildcard() {
        let roots = derive_served_domains(["shop.example.com"]);
        assert!(!roots[0].hostnames[0].wildcard);
    }

    #[test]
    fn values_that_cannot_be_a_domain_are_dropped() {
        // `_` is nginx's catch-all server_name; an IP literal can be served but
        // cannot be a domain; a single label has no root; `a..b` is malformed.
        assert!(derived(&["_", "127.0.0.1", "localhost", "a..b.example.com"]).is_empty());
    }

    #[test]
    fn duplicate_names_collapse_to_one_hostname() {
        let folded = derived(&["www.example.com", "WWW.EXAMPLE.COM", "www.example.com."]);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].1, vec!["www.example.com"]);
    }

    #[test]
    fn an_unknown_suffix_falls_back_to_the_last_two_labels() {
        // `psl` does not carry `.internal`, so the apex is the last two labels.
        let folded = derived(&["api.example.internal"]);
        assert_eq!(folded.len(), 1);
        assert_eq!(folded[0].0, "example.internal");
    }

    #[test]
    fn several_roots_stay_separate_and_ordered() {
        let folded = derived(&["b.example.cn", "a.example.com", "api.example.com"]);
        assert_eq!(folded.len(), 2);
        assert_eq!(folded[0].0, "example.cn");
        assert_eq!(folded[1].0, "example.com");
        assert_eq!(folded[1].1, vec!["a.example.com", "api.example.com"]);
    }

    #[test]
    fn the_reconcile_switch_accepts_the_usual_spellings_of_off() {
        for value in ["0", "false", "FALSE", " off ", "no", "disabled", ""] {
            assert!(reconcile_disabled_value(value), "{value:?} should disable");
        }
        for value in ["1", "true", "on", "yes", "enabled"] {
            assert!(!reconcile_disabled_value(value), "{value:?} should enable");
        }
    }
}
