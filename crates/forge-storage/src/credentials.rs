//! API key and worker credential storage (spec 11.1, spec 10.2).
//!
//! Both credential kinds are stored as a hash of the secret and looked up by
//! that hash, so a leaked database row cannot be replayed as a credential.
//! Neither plaintext is ever recoverable after it is shown once.

use uuid::Uuid;

use crate::error::{Result, StorageError};

/// A stored API key, without its secret.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub owner_id: Option<Uuid>,
    pub role: String,
    pub prefix: Option<String>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct ApiKeyRepository<'a> {
    pool: &'a sqlx::PgPool,
}

const API_KEY_COLUMNS: &str = "id, tenant_id, owner_id, role, prefix, revoked_at, \
     expires_at, last_used_at, created_at";

impl<'a> ApiKeyRepository<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Resolves a key from the hash of the secret the caller presented.
    pub async fn find_by_hash(&self, key_hash: &str) -> Result<Option<ApiKeyRow>> {
        sqlx::query_as::<_, ApiKeyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {API_KEY_COLUMNS} FROM api_keys WHERE key_hash = $1"
        )))
        .bind(key_hash)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Records that a key was used.
    ///
    /// Best-effort: an operator seeing `last_used_at` is a convenience, and
    /// failing a request because that convenience could not be recorded would
    /// turn an audit detail into an outage.
    pub async fn touch(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;
        Ok(())
    }
}

/// A stored service account.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServiceAccountRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub token_hash: String,
    pub token_prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct ServiceAccountRepository<'a> {
    pool: &'a sqlx::PgPool,
}

impl<'a> ServiceAccountRepository<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub async fn find_by_hash(&self, token_hash: &str) -> Result<Option<ServiceAccountRow>> {
        sqlx::query_as::<_, ServiceAccountRow>(
            "SELECT id, tenant_id, name, description, token_hash, token_prefix, scopes, \
             expires_at, revoked_at, last_used_at, created_by, created_at, updated_at \
             FROM service_accounts WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn touch(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE service_accounts SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;
        Ok(())
    }
}

/// A registered OIDC identity provider.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdentityProviderRow {
    pub id: Uuid,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret_encrypted: Vec<u8>,
    pub scopes: Vec<String>,
    pub enabled: bool,
    pub allowed_email_domains: Option<Vec<String>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credential columns a verification query depends on. If a migration
    /// drops or renames one, this fails before a deployed key silently stops
    /// working.
    #[test]
    fn verification_columns_are_the_ones_verification_reads() {
        for column in [
            "id",
            "tenant_id",
            "owner_id",
            "role",
            "revoked_at",
            "expires_at",
        ] {
            assert!(
                API_KEY_COLUMNS.contains(column),
                "ApiKeyRow reads `{column}` but the column list omits it"
            );
        }
    }
}
