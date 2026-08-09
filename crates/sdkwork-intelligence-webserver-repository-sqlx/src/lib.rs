use sdkwork_database_id::SnowflakeIdGenerator;
use sqlx::{Database, Pool};

mod runtime;

pub use runtime::{bootstrap_web_runtime_from_env, WebRuntime};

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
