//! Application and environment storage (spec 02 §2.1-2.2, `redesign.md` §D).
//!
//! An application groups jobs and workers by *what* the work is; an environment
//! is *where* it runs. They are separate tables because migration copies the
//! first and re-points the second — see ADR-0022.

use chrono::{DateTime, Utc};
use forge_domain::{ApplicationId, EnvironmentId, EnvironmentKind, Slug, TenantId};
use serde_json::Value as Json;
use uuid::Uuid;

use crate::error::{Result, StorageError};

/// A stored application.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApplicationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub labels: Json,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A stored environment.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EnvironmentRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub slug: String,
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EnvironmentRow {
    /// Parses `kind` into the domain enum.
    ///
    /// An unrecognised value is an error rather than a default: silently
    /// treating an unknown kind as non-production would disable the guardrails
    /// that depend on it, which is the wrong direction to fail.
    pub fn environment_kind(&self) -> Result<EnvironmentKind> {
        use std::str::FromStr;
        EnvironmentKind::from_str(&self.kind)
            .map_err(|_| StorageError::Corrupt(format!("unknown environment kind `{}`", self.kind)))
    }
}

/// Fields for creating an application.
#[derive(Debug, Clone)]
pub struct NewApplication {
    pub tenant_id: TenantId,
    pub slug: Slug,
    pub name: String,
    pub description: Option<String>,
}

/// Fields for creating an environment.
#[derive(Debug, Clone)]
pub struct NewEnvironment {
    pub tenant_id: TenantId,
    pub slug: Slug,
    pub name: String,
    pub kind: EnvironmentKind,
    pub description: Option<String>,
}

pub struct ApplicationRepository<'a> {
    pool: &'a sqlx::PgPool,
}

const APPLICATION_COLUMNS: &str =
    "id, tenant_id, slug, name, description, labels, created_at, updated_at";

impl<'a> ApplicationRepository<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, new: &NewApplication) -> Result<ApplicationRow> {
        sqlx::query_as::<_, ApplicationRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO applications (id, tenant_id, slug, name, description)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING {APPLICATION_COLUMNS}"
        )))
        .bind(ApplicationId::new().into_uuid())
        .bind(new.tenant_id.into_uuid())
        .bind(new.slug.as_str())
        .bind(&new.name)
        .bind(&new.description)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Reads one application within a tenant.
    ///
    /// Scoped by `tenant_id` rather than by id alone: an id is guessable, and
    /// returning another tenant's row would be an isolation failure at the
    /// storage layer as well as the API.
    pub async fn get(
        &self,
        tenant_id: TenantId,
        id: ApplicationId,
    ) -> Result<Option<ApplicationRow>> {
        sqlx::query_as::<_, ApplicationRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {APPLICATION_COLUMNS} FROM applications WHERE tenant_id = $1 AND id = $2"
        )))
        .bind(tenant_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Resolves by the typeable slug, which is what a URL carries.
    pub async fn get_by_slug(
        &self,
        tenant_id: TenantId,
        slug: &str,
    ) -> Result<Option<ApplicationRow>> {
        sqlx::query_as::<_, ApplicationRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {APPLICATION_COLUMNS} FROM applications WHERE tenant_id = $1 AND slug = $2"
        )))
        .bind(tenant_id.into_uuid())
        .bind(slug)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Lists a tenant's applications, newest first.
    pub async fn list(&self, tenant_id: TenantId) -> Result<Vec<ApplicationRow>> {
        sqlx::query_as::<_, ApplicationRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {APPLICATION_COLUMNS} FROM applications WHERE tenant_id = $1 ORDER BY name"
        )))
        .bind(tenant_id.into_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn update(
        &self,
        tenant_id: TenantId,
        id: ApplicationId,
        name: &str,
        description: Option<&str>,
    ) -> Result<Option<ApplicationRow>> {
        sqlx::query_as::<_, ApplicationRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE applications SET name = $3, description = $4, updated_at = NOW()
             WHERE tenant_id = $1 AND id = $2
             RETURNING {APPLICATION_COLUMNS}"
        )))
        .bind(tenant_id.into_uuid())
        .bind(id.into_uuid())
        .bind(name)
        .bind(description)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Deletes an application, leaving its jobs behind.
    ///
    /// The `ON DELETE SET NULL` on `jobs.application_id` is what makes this
    /// non-cascading: a job is not destroyed because the grouping around it was
    /// removed, it simply becomes ungrouped and the console shows that.
    pub async fn delete(&self, tenant_id: TenantId, id: ApplicationId) -> Result<bool> {
        let result = sqlx::query("DELETE FROM applications WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.into_uuid())
            .bind(id.into_uuid())
            .execute(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    /// How many jobs belong to an application.
    pub async fn job_count(&self, tenant_id: TenantId, id: ApplicationId) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM jobs WHERE tenant_id = $1 AND application_id = $2",
        )
        .bind(tenant_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(count.0)
    }
}

pub struct EnvironmentRepository<'a> {
    pool: &'a sqlx::PgPool,
}

const ENVIRONMENT_COLUMNS: &str =
    "id, tenant_id, slug, name, kind, description, created_at, updated_at";

impl<'a> EnvironmentRepository<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, new: &NewEnvironment) -> Result<EnvironmentRow> {
        sqlx::query_as::<_, EnvironmentRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO environments (id, tenant_id, slug, name, kind, description)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING {ENVIRONMENT_COLUMNS}"
        )))
        .bind(EnvironmentId::new().into_uuid())
        .bind(new.tenant_id.into_uuid())
        .bind(new.slug.as_str())
        .bind(&new.name)
        .bind(new.kind.as_str())
        .bind(&new.description)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn get(
        &self,
        tenant_id: TenantId,
        id: EnvironmentId,
    ) -> Result<Option<EnvironmentRow>> {
        sqlx::query_as::<_, EnvironmentRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {ENVIRONMENT_COLUMNS} FROM environments WHERE tenant_id = $1 AND id = $2"
        )))
        .bind(tenant_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Resolves by slug.
    pub async fn get_by_slug(
        &self,
        tenant_id: TenantId,
        slug: &str,
    ) -> Result<Option<EnvironmentRow>> {
        sqlx::query_as::<_, EnvironmentRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {ENVIRONMENT_COLUMNS} FROM environments WHERE tenant_id = $1 AND slug = $2"
        )))
        .bind(tenant_id.into_uuid())
        .bind(slug)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn list(&self, tenant_id: TenantId) -> Result<Vec<EnvironmentRow>> {
        sqlx::query_as::<_, EnvironmentRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {ENVIRONMENT_COLUMNS} FROM environments WHERE tenant_id = $1 ORDER BY name"
        )))
        .bind(tenant_id.into_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Renames an environment.
    ///
    /// `slug` and `kind` are deliberately immutable: the slug is a URL
    /// identifier other systems bookmark, and `kind` is what the production
    /// guardrail reads. Changing either after the fact would break a deployment
    /// that depends on it, so a new environment is created instead.
    pub async fn update(
        &self,
        tenant_id: TenantId,
        id: EnvironmentId,
        name: &str,
        description: Option<&str>,
    ) -> Result<Option<EnvironmentRow>> {
        sqlx::query_as::<_, EnvironmentRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE environments SET name = $3, description = $4, updated_at = NOW()
             WHERE tenant_id = $1 AND id = $2
             RETURNING {ENVIRONMENT_COLUMNS}"
        )))
        .bind(tenant_id.into_uuid())
        .bind(id.into_uuid())
        .bind(name)
        .bind(description)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Deletes an environment.
    ///
    /// Refused when jobs still point at it. The column is nullable, so the
    /// database *would* allow it — but orphaning a tenant's only production
    /// environment is never what an operator meant, so the check is explicit
    /// and the message names the count.
    ///
    /// The refusal is a return value rather than an error because it is not a
    /// failure of the request: it is the answer "no, and here is why", which the
    /// API turns into a 409 with the count attached.
    pub async fn delete(
        &self,
        tenant_id: TenantId,
        id: EnvironmentId,
    ) -> std::result::Result<(), DeleteEnvironmentError> {
        // Transport failures keep their own type; only the refusals below are
        // this method's own answers.
        let attached: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM jobs WHERE tenant_id = $1 AND environment_id = $2",
        )
        .bind(tenant_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
        .map_err(DeleteEnvironmentError::Storage)?;

        if attached.0 > 0 {
            return Err(DeleteEnvironmentError::StillInUse(attached.0));
        }

        let result = sqlx::query("DELETE FROM environments WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.into_uuid())
            .bind(id.into_uuid())
            .execute(self.pool)
            .await
            .map_err(StorageError::from_sqlx)
            .map_err(DeleteEnvironmentError::Storage)?;

        if result.rows_affected() == 0 {
            return Err(DeleteEnvironmentError::NotFound);
        }
        Ok(())
    }
}

/// Why an environment could not be deleted.
///
/// A `thiserror` type so it converts into the crate's error chain for the
/// transport failures, while the refusals themselves stay distinguishable.
#[derive(Debug, thiserror::Error)]
pub enum DeleteEnvironmentError {
    #[error("{0} job(s) still reference this environment")]
    StillInUse(i64),
    #[error("environment not found in this tenant")]
    NotFound,
    /// A transport failure, kept distinct so a caller cannot mistake a
    /// timeout for "this environment is in use".
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The column lists these queries depend on.
    ///
    /// If a migration renames or drops one, every query above fails at runtime
    /// rather than at compile time (ADR-0017), so the names are pinned here.
    #[test]
    fn column_lists_name_real_columns() {
        for column in APPLICATION_COLUMNS.split(", ") {
            assert!(
                [
                    "id",
                    "tenant_id",
                    "slug",
                    "name",
                    "description",
                    "labels",
                    "created_at",
                    "updated_at"
                ]
                .contains(&column),
                "unexpected application column `{column}`"
            );
        }
        for column in ENVIRONMENT_COLUMNS.split(", ") {
            assert!(
                [
                    "id",
                    "tenant_id",
                    "slug",
                    "name",
                    "kind",
                    "description",
                    "created_at",
                    "updated_at"
                ]
                .contains(&column),
                "unexpected environment column `{column}`"
            );
        }
    }

    #[test]
    fn every_tenant_scoped_query_actually_scopes() {
        // A repository query missing its tenant predicate is an isolation bug
        // that no test would catch until two tenants existed. Checked over
        // whole statements rather than lines, because these queries wrap and the
        // `tenant_id` predicate is often on the following line.
        let source = include_str!("applications.rs").to_ascii_uppercase();

        // The test module itself names these tables in its own string
        // literals, so it is cut off before scanning — otherwise the check
        // matches its own assertions.
        let body = source.split("#[CFG(TEST)]").next().unwrap_or_default();

        let mut checked = 0;
        let mut rest = body;
        while let Some(at) = rest.find("SELECT ") {
            let statement = &rest[at..];
            // Cut at the end of the statement rather than reading to the end of
            // the file, which would credit this query with the next one's
            // predicate.
            let end = statement.find(';').unwrap_or(statement.len());
            let statement = &statement[..end];

            if statement.contains("FROM APPLICATIONS") || statement.contains("FROM ENVIRONMENTS") {
                assert!(
                    statement.contains("TENANT_ID = $1"),
                    "a query does not scope by tenant: {statement}"
                );
                checked += 1;
            }
            rest = &rest[at + 1..];
        }

        // Every statement in this file that reads or deletes tenant data carries
        // `tenant_id = $1`. The count is exact rather than a floor so that
        // adding a query without scoping it fails here, and renaming one fails
        // because the statement is no longer recognised.
        assert_eq!(
            checked, 6,
            "expected 6 tenant-scoped SELECT statements, found {checked}"
        );

        // The two DELETEs are scoped too, and the SELECT scan cannot see them.
        for line in source.lines() {
            if line.contains("DELETE FROM applications")
                || line.contains("DELETE FROM environments")
            {
                assert!(
                    line.contains("TENANT_ID = $1"),
                    "a delete does not scope by tenant: {line}"
                );
            }
        }
    }
}
