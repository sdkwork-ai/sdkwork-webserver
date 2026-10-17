use sdkwork_database_id::SnowflakeIdGenerator;
use sdkwork_webserver_contract::{WebServiceError, WebServiceResult};
use sqlx::{Database, Pool, Row};

mod resolution_cache;
mod runtime;

pub use resolution_cache::{resolution_cache_from_shared_pool, SqlxResolutionCache};
pub use runtime::{bootstrap_web_runtime_from_env, WebRuntime};

/// The WeChat verification file a request hostname resolves to, via its root
/// domain. Exposed for the standalone gateway's data plane: the edge answers
/// `https://<hostname>/<fileName>` from here for the apex and every registered
/// subdomain of the root domain.
///
/// Self-contained on purpose: it needs only a PostgreSQL pool, so it does not
/// depend on the domain-repository engine surface.
pub async fn wechat_verification_by_hostname_lookup(
    pool: &sqlx::PgPool,
    hostname: &str,
) -> WebServiceResult<Option<(String, String)>> {
    let normalized = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    let row = sqlx::query(
        "SELECT v.file_name, v.content
         FROM webserver_domain_wechat_verification v
         JOIN webserver_root_domain r ON r.id = v.root_domain_id
         LEFT JOIN webserver_domain d
           ON d.root_domain_id = r.id AND d.hostname = $1 AND d.deleted_at IS NULL
         WHERE r.deleted_at IS NULL AND (r.hostname = $1 OR d.id IS NOT NULL)
         ORDER BY r.id DESC
         LIMIT 1",
    )
    .bind(&normalized)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        WebServiceError::Internal(format!(
            "read webserver_domain_wechat_verification by hostname: {error}"
        ))
    })?;
    let Some(row) = row else {
        return Ok(None);
    };
    let file_name: String = row
        .try_get("file_name")
        .map_err(|error| WebServiceError::Internal(format!("map file name: {error}")))?;
    let content: String = row
        .try_get("content")
        .map_err(|error| WebServiceError::Internal(format!("map content: {error}")))?;
    Ok(Some((file_name, content)))
}

/// Marks a dynamically assembled SQL statement as audited for sqlx 0.9's
/// compile-time injection check (`SqlSafeStr`).
///
/// Repository statements are assembled exclusively from fixed clauses and
/// validated enum constants; every request-controlled value enters through
/// `$N` bind parameters. Call sites keep this contract: never interpolate a
/// value derived from request input into the SQL text itself.
pub(crate) fn audited_sql(sql: &str) -> sqlx::AssertSqlSafe<&str> {
    sqlx::AssertSqlSafe(sql)
}

/// AES-256 key used to protect environment-variable secrets at rest.
pub type SecretEncryptionKey = [u8; 32];

#[derive(Clone)]
pub struct TypedWebRepository<DB: Database> {
    pool: Pool<DB>,
    id_generator: SnowflakeIdGenerator,
    secret_key: SecretEncryptionKey,
}

impl<DB: Database> TypedWebRepository<DB> {
    pub fn new(
        pool: Pool<DB>,
        id_generator: SnowflakeIdGenerator,
        secret_key: SecretEncryptionKey,
    ) -> Self {
        Self {
            pool,
            id_generator,
            secret_key,
        }
    }

    pub fn pool(&self) -> &Pool<DB> {
        &self.pool
    }

    pub fn id_generator(&self) -> &SnowflakeIdGenerator {
        &self.id_generator
    }

    pub fn secret_key(&self) -> &SecretEncryptionKey {
        &self.secret_key
    }
}

pub type PostgresWebRepository = TypedWebRepository<sqlx::Postgres>;

macro_rules! repository_engine {
    ($module:ident, $database:ty, $pool:ty, $row:ty, $arguments:ty) => {
        mod $module {
            type WebRepository = crate::TypedWebRepository<$database>;
            type EnginePool = $pool;
            type EngineRow = $row;
            type EngineDatabase = $database;
            type EngineArguments<'q> = $arguments;

            mod support {
                include!("support.rs");
            }
            mod agents {
                include!("agents.rs");
            }
            mod audit {
                include!("audit.rs");
            }
            mod certificates {
                include!("certificates.rs");
            }
            mod certificate_operations {
                include!("certificate_operations.rs");
            }
            mod certificate_secrets {
                include!("certificate_secrets.rs");
            }
            mod certificate_activation {
                include!("certificate_activation.rs");
            }
            mod certificate_bindings {
                include!("certificate_bindings.rs");
            }
            mod tls_material {
                include!("tls_material.rs");
            }
            mod deployments {
                include!("deployments.rs");
            }
            mod domains {
                include!("domains.rs");
            }
            mod domain_dns_records {
                include!("domain_dns_records.rs");
            }
            mod env_variables {
                include!("env_variables.rs");
            }
            mod health_checks {
                include!("health_checks.rs");
            }
            mod nginx_configs {
                include!("nginx_configs.rs");
            }
            mod root_domains {
                include!("root_domains.rs");
            }
            mod runtime_assignments {
                include!("runtime_assignments.rs");
            }
            mod servers {
                include!("servers.rs");
            }
            mod cluster {
                include!("cluster.rs");
            }
            mod applications {
                include!("applications.rs");
            }
            mod platform_targets {
                include!("platform_targets.rs");
            }
            mod source_versions {
                include!("source_versions.rs");
            }
            mod port {
                include!("port.rs");
            }
        }
    };
}

repository_engine!(
    postgres,
    sqlx::Postgres,
    sqlx::PgPool,
    sqlx::postgres::PgRow,
    sqlx::postgres::PgArguments
);
