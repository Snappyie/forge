//! Job revisions, and the read model a cross-environment migration is planned
//! from.
//!
//! A *revision* is a job's editable desired state, distinct from the immutable
//! published `job_versions` that executions point back to. A migration needs
//! the former: copying the immutable version would carry whatever an operator
//! published months ago, not what the job is now.

use chrono::{DateTime, Utc};
use forge_domain::{JobId, JobRevisionId, MigratableField, TenantId};
use serde_json::Value as Json;
use uuid::Uuid;

use crate::error::Result;

/// A stored job revision.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JobRevisionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub revision_number: i32,
    pub config: Json,
    pub change_summary: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Fields for recording a revision.
#[derive(Debug, Clone)]
pub struct NewJobRevision {
    pub tenant_id: TenantId,
    pub job_id: JobId,
    pub config: Json,
    pub change_summary: Option<String>,
    pub created_by: Option<Uuid>,
}

/// A job as the migration planner sees it.
///
/// This is the source side of a plan: everything here is environment-independent
/// by construction, because the bindings that are *not* portable are reported
/// separately as unresolved.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MigrationCandidate {
    pub id: Uuid,
    pub key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub priority: String,
    pub application_id: Option<Uuid>,
    pub application_slug: Option<String>,
    pub expression: Option<String>,
    pub timezone: Option<String>,
    pub misfire_policy: Option<String>,
    pub enabled: Option<bool>,
}

pub struct JobRevisionRepository<'a> {
    pool: &'a sqlx::PgPool,
}

const REVISION_COLUMNS: &str =
    "id, tenant_id, job_id, revision_number, config, change_summary, created_by, created_at";

impl<'a> JobRevisionRepository<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Records a revision, numbering it as the next integer for that job.
    ///
    /// The number is derived inside the insert rather than passed in, so two
    /// concurrent editors cannot both claim revision N — the unique index on
    /// `(job_id, revision_number)` would reject one anyway, but failing there
    /// would surface as a constraint violation rather than as "try again".
    pub async fn record(&self, new: &NewJobRevision) -> Result<JobRevisionRow> {
        sqlx::query_as::<_, JobRevisionRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO job_revisions
                 (id, tenant_id, job_id, revision_number, config, change_summary, created_by)
             VALUES ($1, $2, $3,
                     (SELECT COALESCE(MAX(revision_number), 0) + 1
                        FROM job_revisions WHERE job_id = $3),
                     $4, $5, $6)
             RETURNING {REVISION_COLUMNS}"
        )))
        .bind(JobRevisionId::new().into_uuid())
        .bind(new.tenant_id.into_uuid())
        .bind(new.job_id.into_uuid())
        .bind(&new.config)
        .bind(&new.change_summary)
        .bind(new.created_by)
        .fetch_one(self.pool)
        .await
        .map_err(crate::error::StorageError::from_sqlx)
    }

    /// A job's revisions, newest first.
    pub async fn list(&self, tenant_id: TenantId, job_id: JobId) -> Result<Vec<JobRevisionRow>> {
        sqlx::query_as::<_, JobRevisionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {REVISION_COLUMNS} FROM job_revisions
             WHERE tenant_id = $1 AND job_id = $2
             ORDER BY revision_number DESC"
        )))
        .bind(tenant_id.into_uuid())
        .bind(job_id.into_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(crate::error::StorageError::from_sqlx)
    }

    /// The most recent revision of a job, if it has one.
    pub async fn latest(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
    ) -> Result<Option<JobRevisionRow>> {
        sqlx::query_as::<_, JobRevisionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {REVISION_COLUMNS} FROM job_revisions
             WHERE tenant_id = $1 AND job_id = $2
             ORDER BY revision_number DESC LIMIT 1"
        )))
        .bind(tenant_id.into_uuid())
        .bind(job_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(crate::error::StorageError::from_sqlx)
    }

    /// Reads a migration's source jobs.
    ///
    /// Two queries rather than one join: a job may have no schedule (it runs
    /// only when triggered) and an inner join would silently drop it from the
    /// plan, so the operator would not be told a job exists and is not moving.
    pub async fn migration_candidates(
        &self,
        tenant_id: TenantId,
        environment_id: Option<Uuid>,
        application_id: Option<Uuid>,
    ) -> Result<Vec<MigrationCandidate>> {
        sqlx::query_as::<_, MigrationCandidate>(
            r#"
            SELECT j.id,
                   j.key,
                   j.name,
                   j.description,
                   j.priority,
                   j.application_id,
                   a.slug AS application_slug,
                   s.expression,
                   s.timezone,
                   s.misfire_policy::text AS misfire_policy,
                   s.enabled
              FROM jobs j
              LEFT JOIN applications a ON a.id = j.application_id
              LEFT JOIN schedules s
                     ON s.target_id = j.id AND s.target_type = 'JOB'
             WHERE j.tenant_id = $1
               AND ($2::uuid IS NULL OR j.environment_id = $2)
               AND ($3::uuid IS NULL OR j.application_id = $3)
             ORDER BY j.name
            "#,
        )
        .bind(tenant_id.into_uuid())
        .bind(environment_id)
        .bind(application_id)
        .fetch_all(self.pool)
        .await
        .map_err(crate::error::StorageError::from_sqlx)
    }
}

/// Which portable fields a plan carries.
///
/// The default is identity, schedule and parameters — never bindings. That
/// default is the safety property: a migration that silently included bindings
/// would point production at a queue that does not exist there.
pub const DEFAULT_MIGRATABLE: &[MigratableField] = &[
    MigratableField::Identity,
    MigratableField::Schedule,
    MigratableField::Parameters,
];

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::MigratableField;

    #[test]
    fn the_default_migration_excludes_bindings() {
        // This is the single most consequential default in the migration path:
        // bindings name destination objects that must be resolved by a human.
        assert!(
            !DEFAULT_MIGRATABLE.contains(&MigratableField::Bindings),
            "bindings must be opt-in"
        );
    }

    #[test]
    fn the_default_migration_carries_the_portable_fields() {
        for field in [
            MigratableField::Identity,
            MigratableField::Schedule,
            MigratableField::Parameters,
        ] {
            assert!(
                DEFAULT_MIGRATABLE.contains(&field),
                "{field:?} should be migrated by default"
            );
        }
    }

    #[test]
    fn only_bindings_require_resolution() {
        // If any other field needed resolution, the "safe by default" claim
        // above would be false.
        for field in [
            MigratableField::Identity,
            MigratableField::Schedule,
            MigratableField::Parameters,
        ] {
            // `needs_resolution` consumes the value, so the label used in the
            // failure message is taken first.
            let label = format!("{field:?}");
            assert!(!field.needs_resolution(), "{label} needs no mapping");
        }
        assert!(MigratableField::Bindings.needs_resolution());
    }
}
