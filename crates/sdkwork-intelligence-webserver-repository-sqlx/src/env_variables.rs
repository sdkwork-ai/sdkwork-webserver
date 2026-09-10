use crate::audited_sql;
use sdkwork_utils_rust::{aes_gcm_encrypt, derive_aes_256_key};

use crate::SecretEncryptionKey;

/// Key-derivation context binding environment-variable secret ciphertext to
/// its tenant and variable identity (same discipline as certificate secret
/// bundles): a ciphertext copied to another tenant or row in the database
/// cannot be decrypted there.
const ENV_VARIABLE_SECRET_KEY_CONTEXT: &[u8] = b"sdkwork-web-env-variable-secret-bound-v1";

fn env_variable_secret_key(
    master_key: &SecretEncryptionKey,
    tenant_id: i64,
    variable_uuid: &str,
) -> SecretEncryptionKey {
    let salt = format!("{tenant_id}:{variable_uuid}");
    derive_aes_256_key(master_key, salt.as_bytes(), ENV_VARIABLE_SECRET_KEY_CONTEXT)
}
use sdkwork_webserver_contract::{
    CreateEnvVariableRequest, EnvVariablePage, EnvVariableResponse, UpdateEnvVariableRequest,
    WebServiceError, WebServiceResult,
};
use sqlx::Row;

use super::support::{
    bool_from_row, instant_write_expression, new_uuid, next_id, now_rfc3339,
    resolve_site_internal_id, store_error,
};
use super::{EngineRow, WebRepository};

/// 机密值在 list/retrieve 响应中的掩码占位符。
/// 真实值仅通过 create 接口接收并加密落库，永不在查询响应中返回明文。
const MAX_SITE_ENV_VARIABLES: i64 = 100;
const SECRET_VALUE_MASK: &str = "***";

impl WebRepository {
    pub(super) async fn list_env_variables_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        environment: Option<&str>,
    ) -> WebServiceResult<EnvVariablePage> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;

        let (count_row, rows) = if let Some(environment) = environment {
            let count_row = sqlx::query(
                "SELECT COUNT(*) AS total FROM web_env_variable
                 WHERE tenant_id = $1 AND site_id = $2 AND environment = $3 AND status = 1",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(environment)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count web_env_variable", error))?;

            let rows = sqlx::query(
                "SELECT uuid, key, value_encrypted, environment, is_secret
                 FROM web_env_variable
                 WHERE tenant_id = $1 AND site_id = $2 AND environment = $3 AND status = 1
                 ORDER BY key ASC
                 LIMIT 100",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(environment)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_env_variable", error))?;

            (count_row, rows)
        } else {
            let count_row = sqlx::query(
                "SELECT COUNT(*) AS total FROM web_env_variable
                 WHERE tenant_id = $1 AND site_id = $2 AND status = 1",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error("count web_env_variable", error))?;

            let rows = sqlx::query(
                "SELECT uuid, key, value_encrypted, environment, is_secret
                 FROM web_env_variable
                 WHERE tenant_id = $1 AND site_id = $2 AND status = 1
                 ORDER BY environment ASC, key ASC
                 LIMIT 100",
            )
            .bind(tenant_id)
            .bind(site_internal_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error("list web_env_variable", error))?;

            (count_row, rows)
        };

        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_env_variable count", error))?;
        if total > MAX_SITE_ENV_VARIABLES {
            tracing::error!(
                tenant_id,
                site_id,
                total,
                maximum = MAX_SITE_ENV_VARIABLES,
                "web environment-variable cardinality invariant violated"
            );
            return Err(WebServiceError::Internal(
                "environment-variable collection exceeds its configured capacity".to_string(),
            ));
        }
        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            items.push(map_env_variable_row(row).map_err(|error| {
                WebServiceError::Internal(format!("map web_env_variable row: {error}"))
            })?);
        }

        Ok(EnvVariablePage { items, total })
    }

    pub(super) async fn create_env_variable_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        request: &CreateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let id = next_id(self.id_generator())?;
        let uuid = new_uuid();
        let now = now_rfc3339();

        // 机密值必须加密后落库，非机密值原样存储以保持可读性与查询效率。
        // 密钥按 tenant + 变量派生：密文被移植到其他租户/行后无法解密。
        let derived = env_variable_secret_key(self.secret_key(), tenant_id, &uuid);
        let stored_value = if request.is_secret {
            aes_gcm_encrypt(&derived, request.value.as_bytes()).map_err(|error| {
                WebServiceError::Internal(format!("encrypt env variable: {error}"))
            })?
        } else {
            request.value.clone()
        };

        let now_expression = instant_write_expression("$9");
        let insert_sql = format!(
            "INSERT INTO web_env_variable (
                id, uuid, tenant_id, site_id, environment, key, value_encrypted, is_secret,
                status, created_at, updated_at, version
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, 1,
                {now_expression}, {now_expression}, 0
             )"
        );

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error("begin create web_env_variable transaction", error))?;
        let locked = sqlx::query(
            "UPDATE web_site SET version = version
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error("lock web_env_variable site capacity", error))?;
        if locked.rows_affected() != 1 {
            return Err(WebServiceError::not_found("site not found"));
        }

        let count_row = sqlx::query(
            "SELECT COUNT(*) AS total FROM web_env_variable
             WHERE tenant_id = $1 AND site_id = $2 AND status = 1",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error("count web_env_variable capacity", error))?;
        let total: i64 = count_row
            .try_get("total")
            .map_err(|error| store_error("map web_env_variable capacity", error))?;
        if total >= MAX_SITE_ENV_VARIABLES {
            transaction
                .rollback()
                .await
                .map_err(|error| store_error("rollback full web_env_variable collection", error))?;
            return Err(WebServiceError::conflict(
                "a site supports at most 100 active environment variables",
            ));
        }

        sqlx::query(audited_sql(&insert_sql))
            .bind(id)
            .bind(&uuid)
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(&request.environment)
            .bind(&request.key)
            .bind(&stored_value)
            .bind(request.is_secret)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error("insert web_env_variable", error))?;

        transaction
            .commit()
            .await
            .map_err(|error| store_error("commit create web_env_variable transaction", error))?;

        // 响应中机密值返回掩码，不回传明文/密文，避免泄漏。
        Ok(EnvVariableResponse {
            id: uuid,
            key: request.key.clone(),
            value: if request.is_secret {
                SECRET_VALUE_MASK.to_string()
            } else {
                request.value.clone()
            },
            environment: request.environment.clone(),
            is_secret: request.is_secret,
        })
    }

    /// Rotates an active variable's value in place (encrypted when secret).
    pub(super) async fn update_env_variable_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        variable_id: &str,
        request: &UpdateEnvVariableRequest,
    ) -> WebServiceResult<EnvVariableResponse> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let row = sqlx::query(
            "SELECT uuid, key, environment, is_secret, version
             FROM web_env_variable
             WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3 AND status = 1",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(variable_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("retrieve web_env_variable for update", error))?
        .ok_or_else(|| WebServiceError::not_found("environment variable not found"))?;
        let key: String = row
            .try_get("key")
            .map_err(|error| store_error("map web_env_variable key", error))?;
        let environment: String = row
            .try_get("environment")
            .map_err(|error| store_error("map web_env_variable environment", error))?;
        let stored_version: i64 = row
            .try_get("version")
            .map_err(|error| store_error("map web_env_variable version", error))?;
        let derived = env_variable_secret_key(self.secret_key(), tenant_id, variable_id);
        let stored_value = if request.is_secret {
            aes_gcm_encrypt(&derived, request.value.as_bytes()).map_err(|error| {
                WebServiceError::Internal(format!("encrypt env variable: {error}"))
            })?
        } else {
            request.value.clone()
        };
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$5");
        let update_sql = format!(
            "UPDATE web_env_variable
             SET value_encrypted = $4, is_secret = $6,
                 updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3 AND status = 1 AND version = $7"
        );
        let result = sqlx::query(audited_sql(&update_sql))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(variable_id)
            .bind(&stored_value)
            .bind(&now)
            .bind(request.is_secret)
            .bind(stored_version)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("update web_env_variable", error))?;
        if result.rows_affected() == 0 {
            // The variable was modified concurrently or removed between the
            // read and the compare-and-swap update: distinguish the two
            // truthfully instead of silently losing an update.
            return self
                .conflict_or_missing_env_variable(tenant_id, site_internal_id, variable_id)
                .await;
        }
        Ok(EnvVariableResponse {
            id: variable_id.to_string(),
            key,
            value: if request.is_secret {
                SECRET_VALUE_MASK.to_string()
            } else {
                request.value.clone()
            },
            environment,
            is_secret: request.is_secret,
        })
    }

    /// Soft-deletes an active variable so its key can be re-created.
    pub(super) async fn delete_env_variable_repo(
        &self,
        tenant_id: i64,
        site_id: &str,
        variable_id: &str,
    ) -> WebServiceResult<()> {
        let site_internal_id = resolve_site_internal_id(&self.pool, tenant_id, site_id).await?;
        let now = now_rfc3339();

        let now_expression = instant_write_expression("$4");
        let update_sql = format!(
            "UPDATE web_env_variable
             SET status = 0, updated_at = {now_expression}, version = version + 1
             WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3 AND status = 1"
        );
        let result = sqlx::query(audited_sql(&update_sql))
            .bind(tenant_id)
            .bind(site_internal_id)
            .bind(variable_id)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error("delete web_env_variable", error))?;
        if result.rows_affected() == 0 {
            return Err(WebServiceError::not_found("environment variable not found"));
        }
        Ok(())
    }

    /// Distinguishes a concurrent-write conflict from a missing variable
    /// after a compare-and-swap update affected zero rows.
    async fn conflict_or_missing_env_variable(
        &self,
        tenant_id: i64,
        site_internal_id: i64,
        variable_id: &str,
    ) -> WebServiceResult<EnvVariableResponse> {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM web_env_variable
             WHERE tenant_id = $1 AND site_id = $2 AND uuid = $3 AND status = 1",
        )
        .bind(tenant_id)
        .bind(site_internal_id)
        .bind(variable_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error("recheck web_env_variable existence", error))?;
        if exists.is_some() {
            return Err(WebServiceError::conflict(
                "environment variable was modified concurrently; reload and retry",
            ));
        }
        Err(WebServiceError::not_found("environment variable not found"))
    }
}

fn map_env_variable_row(row: &EngineRow) -> Result<EnvVariableResponse, sqlx::Error> {
    let is_secret = bool_from_row(row, "is_secret")?;
    Ok(EnvVariableResponse {
        id: row.try_get("uuid")?,
        key: row.try_get("key")?,
        // 机密值在查询响应中始终返回掩码，永不明文回传。
        // value_encrypted 列对 is_secret=true 存储的是 base64(nonce||ciphertext)，不可直接展示。
        value: if is_secret {
            SECRET_VALUE_MASK.to_string()
        } else {
            row.try_get("value_encrypted")?
        },
        environment: row.try_get("environment")?,
        is_secret,
    })
}
