use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{Result, StorageError};
use forge_domain::{
    ConcurrencyLimit, ConcurrencyPolicy, ErrorClass, ExecutionStatus, JobId, JobStatus,
    JobVersionId, Priority, RetryPolicy, TenantId, TriggerSource,
};

/// Row shape for `jobs`. Kept separate from `forge_domain::Job` so the domain
/// type stays free of storage concerns (spec 04.4).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct JobRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub current_version_id: Option<Uuid>,
    pub default_queue_id: Option<Uuid>,
    pub priority: String,
    pub owner_id: Option<Uuid>,
    pub labels: serde_json::Value,
    /// Where this job runs. Required as of migration 026.
    pub environment_id: Uuid,
    /// Which application groups it, when it is grouped at all.
    pub application_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct JobVersionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub job_id: Uuid,
    pub version_number: i32,
    pub execution_type: String,
    pub execution_config: serde_json::Value,
    pub input_schema: Option<serde_json::Value>,
    pub timeout_seconds: i32,
    pub retry_policy: serde_json::Value,
    pub concurrency_policy: serde_json::Value,
    pub resource_requirements: serde_json::Value,
    pub environment: serde_json::Value,
    pub secret_references: serde_json::Value,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

/// Filters for `list_jobs` (spec 05 endpoint 2).
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub status: Option<JobStatus>,
    pub queue_id: Option<Uuid>,
    pub owner_id: Option<Uuid>,
    pub label: Option<(String, String)>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub search: Option<String>,
}

impl JobFilter {
    /// Appends this filter's predicates to a positional query builder.
    ///
    /// `QueryBuilder` is used instead of string concatenation because the
    /// filters are optional and heterogeneous; each predicate binds its own
    /// placeholder so no value is ever interpolated into SQL text.
    fn apply(&self, qb: &mut QueryBuilder<Postgres>) {
        if let Some(status) = self.status {
            qb.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(queue) = self.queue_id {
            qb.push(" AND default_queue_id = ").push_bind(queue);
        }
        if let Some(owner) = self.owner_id {
            qb.push(" AND owner_id = ").push_bind(owner);
        }
        if let Some((key, value)) = &self.label {
            // Labels are JSONB; a bound JSON literal makes this a containment
            // test rather than a string comparison. The literal is built into
            // a local because `push_bind` borrows it for the query's lifetime.
            let literal = serde_json::json!({ key.clone(): value.clone() }).to_string();
            qb.push(" AND labels @> ").push_bind(literal);
        }
        if let Some(after) = self.created_after {
            qb.push(" AND created_at >= ").push_bind(after);
        }
        if let Some(before) = self.created_before {
            qb.push(" AND created_at < ").push_bind(before);
        }
        if let Some(term) = &self.search {
            // One placeholder reused for both name and key.
            let pattern = format!("%{term}%");
            qb.push(" AND (name ILIKE ").push_bind(pattern.clone());
            qb.push(" OR key ILIKE ").push_bind(pattern);
            qb.push(")");
        }
    }
}

const JOB_COLUMNS: &str = "id, tenant_id, key, name, description, status, current_version_id, \
     default_queue_id, priority, owner_id, labels, environment_id, application_id, \
     created_at, updated_at";

pub struct JobRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> JobRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Creates a job in `DRAFT`. Invariant: `tenant_id` is immutable and is
    /// supplied by the caller from the authenticated identity, never from the
    /// request body.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tenant_id: TenantId,
        key: Option<String>,
        name: &str,
        description: Option<String>,
        priority: Priority,
        owner_id: Option<Uuid>,
        environment_id: Option<Uuid>,
        application_id: Option<Uuid>,
    ) -> Result<JobRow> {
        let id = Uuid::new_v4();
        /*
         * `environment_id` is required by the schema, so a caller that supplies
         * none gets the tenant's default rather than a constraint failure. The
         * lookup is scoped to the tenant: falling back to another tenant's
         * environment would place a job somewhere it cannot be administered.
         */
        let environment_id = match environment_id {
            Some(id) => id,
            None => sqlx::query_scalar(
                "SELECT id FROM environments WHERE tenant_id = $1
                 ORDER BY (slug = 'default') DESC, created_at LIMIT 1",
            )
            .bind(tenant_id.into_uuid())
            .fetch_optional(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?
            .ok_or_else(|| {
                StorageError::Validation(
                    "this tenant has no environment to create the job in".to_string(),
                )
            })?,
        };

        sqlx::query_as::<_, JobRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO jobs
                 (id, tenant_id, key, name, description, status, priority, owner_id,
                  environment_id, application_id)
             VALUES ($1, $2, $3, $4, $5, 'DRAFT', $6, $7, $8, $9)
             RETURNING {JOB_COLUMNS}"
        )))
        .bind(id)
        .bind(tenant_id.into_uuid())
        .bind(&key)
        .bind(name)
        .bind(&description)
        .bind(priority.as_str())
        .bind(owner_id)
        .bind(environment_id)
        .bind(application_id)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Reads one job. Spec 08.3: the tenant scope is part of the WHERE clause,
    /// so a cross-tenant read finds nothing rather than returning another
    /// tenant's row (AT-TEN-001).
    pub async fn get(&self, tenant_id: TenantId, job_id: JobId) -> Result<JobRow> {
        sqlx::query_as::<_, JobRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_COLUMNS} FROM jobs WHERE id = $1 AND tenant_id = $2"
        )))
        .bind(job_id.into_uuid())
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("job"))
    }

    /// Cursor-paginated list (spec 05 §5.14).
    ///
    /// Ordered by `created_at DESC, id DESC`; the cursor carries both so
    /// paging is stable even when timestamps collide.
    pub async fn list(
        &self,
        tenant_id: TenantId,
        filter: &JobFilter,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<crate::error::Page<JobRow>> {
        let decoded = match cursor {
            Some(token) => Some(crate::error::Cursor::decode(token)?),
            None => None,
        };

        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {JOB_COLUMNS} FROM jobs WHERE tenant_id = "
        ));
        qb.push_bind(tenant_id.into_uuid());
        filter.apply(&mut qb);

        if let Some(c) = decoded.as_ref() {
            // Row-comparison keeps the paging predicate correct when several
            // rows share a created_at value.
            qb.push(" AND (created_at, id) < (")
                .push_bind(&c.sort_value)
                .push("::timestamptz, ")
                .push_bind(c.last_id)
                .push(")");
        }

        // One extra row signals whether another page exists.
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind((limit + 1) as i64);

        let rows = qb
            .build_query_as::<JobRow>()
            .fetch_all(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;

        Ok(crate::error::Page::from_overfetch(rows, limit, |row| {
            crate::error::Cursor::new(row.created_at.to_rfc3339(), row.id)
        }))
    }
}

/// Fields that a caller may change on an existing job.
///
/// `description` is a double `Option` so "leave unchanged" and "set to null"
/// are distinguishable, which a single `Option` cannot express.
#[derive(Debug, Clone, Default)]
pub struct JobPatch {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub key: Option<String>,
    pub priority: Option<Priority>,
}

impl<'a> JobRepository<'a> {
    /// Optimistic concurrency (spec 05 §5.15): the update only applies when the
    /// row's `updated_at` still matches what the caller last saw. A stale write
    /// yields `Conflict` (AT-API-007).
    pub async fn update(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        expected_updated_at: DateTime<Utc>,
        patch: &JobPatch,
    ) -> Result<JobRow> {
        sqlx::query_as::<_, JobRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE jobs SET
                 name = COALESCE($3, name),
                 description = CASE WHEN $4::boolean THEN $5 ELSE description END,
                 key = COALESCE($6, key),
                 priority = COALESCE($7, priority),
                 updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 AND updated_at = $8
             RETURNING {JOB_COLUMNS}"
        )))
        .bind(job_id.into_uuid())
        .bind(tenant_id.into_uuid())
        .bind(patch.name.as_deref())
        .bind(patch.description.is_some())
        .bind(patch.description.as_ref().and_then(|d| d.as_deref()))
        .bind(patch.key.as_deref())
        .bind(patch.priority.map(|p| p.as_str().to_string()))
        .bind(expected_updated_at)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or(StorageError::Conflict(
            "job was modified concurrently".to_string(),
        ))
    }

    /// Soft-delete: archives the job rather than removing it, so execution
    /// history stays queryable (spec 09.12).
    pub async fn archive(&self, tenant_id: TenantId, job_id: JobId) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE jobs SET status = 'ARCHIVED', updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 AND status <> 'ARCHIVED'",
        )
        .bind(job_id.into_uuid())
        .bind(tenant_id.into_uuid())
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();

        if affected == 0 {
            return Err(StorageError::not_found("job"));
        }
        Ok(())
    }

    /// Transitions job status through the domain state machine, so an illegal
    /// transition fails in the same way it does in the domain layer.
    pub async fn set_status(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        new_status: JobStatus,
    ) -> Result<JobRow> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            // Lock the row so two concurrent status changes serialise.
            let current = sqlx::query_as::<_, JobRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {JOB_COLUMNS} FROM jobs WHERE id = $1 AND tenant_id = $2 FOR UPDATE"
            )))
            .bind(job_id.into_uuid())
            .bind(tenant_id.into_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?
            .ok_or_else(|| StorageError::not_found("job"))?;

            let parsed: JobStatus = current
                .status
                .parse::<JobStatus>()
                .map_err(|e: forge_domain::DomainError| StorageError::Validation(e.to_string()))?;
            let mut job = forge_domain::Job {
                id: forge_domain::JobId::from_uuid(current.id),
                tenant_id: forge_domain::TenantId::from_uuid(current.tenant_id),
                key: current.key,
                name: current.name,
                description: current.description,
                status: parsed,
                current_version_id: current
                    .current_version_id
                    .map(forge_domain::JobVersionId::from_uuid),
                default_queue_id: current
                    .default_queue_id
                    .map(forge_domain::QueueId::from_uuid),
                priority: parse_priority(&current.priority),
                created_at: current.created_at,
                updated_at: current.updated_at,
            };
            job.transition_to(new_status)
                .map_err(|e| StorageError::Validation(e.to_string()))?;

            sqlx::query_as::<_, JobRow>(sqlx::AssertSqlSafe(format!(
                "UPDATE jobs SET status = $3, updated_at = NOW()
                 WHERE id = $1 AND tenant_id = $2 RETURNING {JOB_COLUMNS}"
            )))
            .bind(job_id.into_uuid())
            .bind(tenant_id.into_uuid())
            .bind(new_status.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)
        }
        .await;

        match result {
            Ok(row) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(row)
            }
            Err(e) => {
                // Roll back so a rejected transition never persists.
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }
}

fn parse_priority(raw: &str) -> Priority {
    match raw {
        "CRITICAL" => Priority::Critical,
        "HIGH" => Priority::High,
        "LOW" => Priority::Low,
        "BACKGROUND" => Priority::Background,
        _ => Priority::Normal,
    }
}

const JOB_VERSION_COLUMNS: &str = "id, tenant_id, job_id, version_number, execution_type, \
     execution_config, input_schema, timeout_seconds, retry_policy, concurrency_policy, \
     resource_requirements, environment, secret_references, created_by, created_at, published_at";

pub struct JobVersionRepository<'a> {
    pool: &'a PgPool,
}

/// A new job version, grouped so the repository call stays readable.
#[derive(Debug, Clone)]
pub struct NewJobVersion {
    pub execution_type: String,
    pub execution_config: serde_json::Value,
    pub input_schema: Option<serde_json::Value>,
    pub timeout_seconds: i32,
    pub retry_policy: RetryPolicy,
    pub concurrency_policy: ConcurrencyPolicy,
    pub resource_requirements: serde_json::Value,
    pub environment: serde_json::Value,
    pub secret_references: serde_json::Value,
    pub created_by: Option<Uuid>,
}

impl Default for NewJobVersion {
    fn default() -> Self {
        Self {
            execution_type: "WORKER_TASK".to_string(),
            execution_config: serde_json::json!({}),
            input_schema: None,
            timeout_seconds: 3600,
            retry_policy: RetryPolicy::default(),
            concurrency_policy: ConcurrencyPolicy::default(),
            resource_requirements: serde_json::json!({}),
            environment: serde_json::json!({}),
            secret_references: serde_json::json!([]),
            created_by: None,
        }
    }
}

impl<'a> JobVersionRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Creates the next version number for a job.
    pub async fn create(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        new: &NewJobVersion,
    ) -> Result<JobVersionRow> {
        // The next version number is derived inside the statement so two
        // concurrent creators cannot pick the same one.
        sqlx::query_as::<_, JobVersionRow>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO job_versions
                 (id, tenant_id, job_id, version_number, execution_type, execution_config,
                  timeout_seconds, retry_policy, concurrency_policy, resource_requirements,
                  environment, secret_references, created_by)
             VALUES ($1, $2, $3,
                 (SELECT COALESCE(MAX(version_number), 0) + 1
                    FROM job_versions WHERE job_id = $3),
                 $4, $5, $6, $7, $8, $9, $10, $11, $12)
             RETURNING {JOB_VERSION_COLUMNS}"
        )))
        .bind(Uuid::new_v4())
        .bind(tenant_id.into_uuid())
        .bind(job_id.into_uuid())
        .bind(&new.execution_type)
        .bind(&new.execution_config)
        .bind(new.timeout_seconds)
        .bind(serde_json::to_value(&new.retry_policy).unwrap_or_default())
        .bind(serde_json::to_value(&new.concurrency_policy).unwrap_or_default())
        .bind(&new.resource_requirements)
        .bind(&new.environment)
        .bind(&new.secret_references)
        .bind(new.created_by)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn get(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        version_id: JobVersionId,
    ) -> Result<JobVersionRow> {
        sqlx::query_as::<_, JobVersionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_VERSION_COLUMNS} FROM job_versions
             WHERE id = $1 AND job_id = $2 AND tenant_id = $3"
        )))
        .bind(version_id.into_uuid())
        .bind(job_id.into_uuid())
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("job version"))
    }

    pub async fn list(&self, tenant_id: TenantId, job_id: JobId) -> Result<Vec<JobVersionRow>> {
        sqlx::query_as::<_, JobVersionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_VERSION_COLUMNS} FROM job_versions
             WHERE job_id = $1 AND tenant_id = $2 ORDER BY version_number DESC"
        )))
        .bind(job_id.into_uuid())
        .bind(tenant_id.into_uuid())
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn latest_published(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
    ) -> Result<Option<JobVersionRow>> {
        sqlx::query_as::<_, JobVersionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {JOB_VERSION_COLUMNS} FROM job_versions
             WHERE job_id = $1 AND tenant_id = $2 AND published_at IS NOT NULL
             ORDER BY version_number DESC LIMIT 1"
        )))
        .bind(job_id.into_uuid())
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Publishes a version and, in the same transaction, points the job at it
    /// (spec 02.16). Publishing is one-way: `published_at` is never cleared, so
    /// a published version cannot be edited afterwards (invariant 4).
    pub async fn publish(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        version_id: JobVersionId,
    ) -> Result<JobVersionRow> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            let updated = sqlx::query_as::<_, JobVersionRow>(sqlx::AssertSqlSafe(format!(
                "UPDATE job_versions SET published_at = COALESCE(published_at, NOW())
                 WHERE id = $1 AND job_id = $2 AND tenant_id = $3
                 RETURNING {JOB_VERSION_COLUMNS}"
            )))
            .bind(version_id.into_uuid())
            .bind(job_id.into_uuid())
            .bind(tenant_id.into_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?
            .ok_or_else(|| StorageError::not_found("job version"))?;

            sqlx::query(
                "UPDATE jobs SET current_version_id = $3, updated_at = NOW()
                 WHERE id = $1 AND tenant_id = $2",
            )
            .bind(job_id.into_uuid())
            .bind(tenant_id.into_uuid())
            .bind(version_id.into_uuid())
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok(updated)
        }
        .await;

        match result {
            Ok(row) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(row)
            }
            Err(e) => {
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }

    /// Rejects mutation of a published version (spec 02.16, invariant 4).
    pub async fn ensure_draft(
        &self,
        tenant_id: TenantId,
        job_id: JobId,
        version_id: JobVersionId,
    ) -> Result<()> {
        let row = self.get(tenant_id, job_id, version_id).await?;
        if row.published_at.is_some() {
            return Err(StorageError::Conflict(
                "published job versions are immutable".to_string(),
            ));
        }
        Ok(())
    }
}

/// Row shape for `executions`, used by the execution repository and by tests.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExecutionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    /// `None` for a workflow run: a run is an execution that names a workflow
    /// rather than a job (migration 016).
    pub job_id: Option<Uuid>,
    pub job_version_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub status: String,
    pub queue_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub attempt_count: i32,
    pub priority: String,
    pub trigger_source: String,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub enqueued_at: Option<DateTime<Utc>>,
    pub correlation_id: Option<String>,
    pub error_class: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    /// The task input the worker needs in order to do the work (spec 01.6).
    pub input: serde_json::Value,
    /// The result a completed attempt reported, where one was given.
    pub output: Option<serde_json::Value>,
    /// When a `RETRY_SCHEDULED` execution becomes eligible again (migration 017).
    pub retry_at: Option<DateTime<Utc>>,
    /// When a running attempt must be declared `TIMED_OUT` (migration 017).
    pub deadline_at: Option<DateTime<Utc>>,
}

const EXECUTION_COLUMNS: &str = "id, tenant_id, job_id, job_version_id, workflow_id, status, \
     queue_id, worker_id, attempt_count, priority, trigger_source, scheduled_for, enqueued_at, \
     correlation_id, error_class, error_code, error_message, created_at, started_at, ended_at, \
     input, output, retry_at, deadline_at";

/// Filters for `list_executions` (spec 05 endpoint 17).
#[derive(Debug, Clone, Default)]
pub struct ExecutionFilter {
    pub job_id: Option<Uuid>,
    pub workflow_id: Option<Uuid>,
    pub status: Option<ExecutionStatus>,
    pub statuses: Vec<ExecutionStatus>,
    pub worker_id: Option<Uuid>,
    pub queue_id: Option<Uuid>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub correlation_id: Option<String>,
}

impl ExecutionFilter {
    /// Appends this filter's predicates to a positional query builder.
    fn apply(&self, qb: &mut QueryBuilder<Postgres>) {
        if let Some(job) = self.job_id {
            qb.push(" AND job_id = ").push_bind(job);
        }
        if let Some(workflow) = self.workflow_id {
            qb.push(" AND workflow_id = ").push_bind(workflow);
        }
        if !self.statuses.is_empty() {
            if self.statuses.len() == 1 {
                qb.push(" AND status = ")
                    .push_bind(self.statuses[0].as_str());
            } else {
                qb.push(" AND status IN (");
                let mut sep = qb.separated(", ");
                for s in &self.statuses {
                    sep.push_bind(s.as_str());
                }
                sep.push_unseparated(")");
            }
        } else if let Some(status) = self.status {
            qb.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(worker) = self.worker_id {
            qb.push(" AND worker_id = ").push_bind(worker);
        }
        if let Some(queue) = self.queue_id {
            qb.push(" AND queue_id = ").push_bind(queue);
        }
        if let Some(after) = self.created_after {
            qb.push(" AND created_at >= ").push_bind(after);
        }
        if let Some(before) = self.created_before {
            qb.push(" AND created_at < ").push_bind(before);
        }
        if let Some(corr) = &self.correlation_id {
            qb.push(" AND correlation_id = ").push_bind(corr);
        }
    }
}

pub struct ExecutionRepository<'a> {
    pool: &'a PgPool,
}

/// The execution plus the queue depth bookkeeping written alongside it.
///
/// Spec 08.4 requires execution creation and its queue record to commit
/// together, so this is one unit of work rather than two calls.
#[derive(Debug, Clone)]
pub struct NewExecution {
    pub tenant_id: TenantId,
    pub job_id: JobId,
    pub job_version_id: JobVersionId,
    pub queue_id: Option<Uuid>,
    /// The schedule that produced this execution, if any. Together with
    /// `scheduled_for` this engages the `(schedule_id, scheduled_for)`
    /// uniqueness constraint that makes AT-SCH-008 a database guarantee rather
    /// than a convention.
    pub schedule_id: Option<Uuid>,
    pub trigger_source: TriggerSource,
    pub priority: Priority,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub correlation_id: Option<String>,
    pub input: serde_json::Value,
}

impl<'a> ExecutionRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Creates an execution and its queue record atomically (spec 08.4).
    pub async fn create(&self, new: NewExecution) -> Result<ExecutionRow> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            // Every execution belongs to a queue. A job without an explicit
            // default queue still needs one, otherwise the execution lands with
            // `queue_id IS NULL` and a worker polling a named queue can never
            // claim it: the work is visible in the queue list and unreachable by
            // any worker. Only a caller that explicitly asks for "any queue" —
            // which no SDK does — would find it.
            let queue_id = match new.queue_id {
                Some(id) => Some(id),
                None => Some(default_queue_id(&mut tx, new.tenant_id).await?),
            };

            let row = sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
                "INSERT INTO executions
                     (id, tenant_id, job_id, job_version_id, queue_id, status, priority,
                      trigger_source, schedule_id, scheduled_for, enqueued_at, correlation_id, input)
                 VALUES ($1, $2, $3, $4, $5, 'QUEUED', $6, $7, $8, $9, NOW(), $10, $11)
                 RETURNING {EXECUTION_COLUMNS}"
            )))
            .bind(Uuid::new_v4())
            .bind(new.tenant_id.into_uuid())
            .bind(new.job_id.into_uuid())
            .bind(new.job_version_id.into_uuid())
            .bind(queue_id)
            .bind(new.priority.as_str())
            .bind(new.trigger_source.as_str())
            .bind(new.schedule_id)
            .bind(new.scheduled_for)
            .bind(&new.correlation_id)
            .bind(&new.input)
            .fetch_one(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok(row)
        }
        .await;

        match result {
            Ok(row) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(row)
            }
            Err(e) => {
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }

    pub async fn get(&self, tenant_id: TenantId, execution_id: Uuid) -> Result<ExecutionRow> {
        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {EXECUTION_COLUMNS} FROM executions WHERE id = $1 AND tenant_id = $2"
        )))
        .bind(execution_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("execution"))
    }

    /// Reads an execution without a tenant filter.
    ///
    /// Reserved for trusted infrastructure paths — the lease reaper and the
    /// completion gate — that hold the execution's own id but not its tenant.
    /// Every API-facing path uses `get`, which is tenant-scoped.
    pub async fn get_unchecked(&self, execution_id: Uuid) -> Result<Option<ExecutionRow>> {
        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {EXECUTION_COLUMNS} FROM executions WHERE id = $1"
        )))
        .bind(execution_id)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Counts a recovery as a new attempt.
    ///
    /// `ABANDONED -> QUEUED` does not route through `RETRY_SCHEDULED`, so it
    /// does not increment the counter on its own. Recovery calls this so a
    /// worker that keeps dying exhausts its budget instead of being retried
    /// forever.
    pub async fn increment_attempt(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
    ) -> Result<ExecutionRow> {
        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE executions SET attempt_count = attempt_count + 1, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 RETURNING {EXECUTION_COLUMNS}"
        )))
        .bind(execution_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("execution"))
    }

    pub async fn list(
        &self,
        tenant_id: TenantId,
        filter: &ExecutionFilter,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<crate::error::Page<ExecutionRow>> {
        let decoded = match cursor {
            Some(token) => Some(crate::error::Cursor::decode(token)?),
            None => None,
        };

        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {EXECUTION_COLUMNS} FROM executions WHERE tenant_id = "
        ));
        qb.push_bind(tenant_id.into_uuid());
        filter.apply(&mut qb);

        if let Some(c) = decoded.as_ref() {
            qb.push(" AND (created_at, id) < (")
                .push_bind(&c.sort_value)
                .push("::timestamptz, ")
                .push_bind(c.last_id)
                .push(")");
        }

        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind((limit + 1) as i64);

        let rows = qb
            .build_query_as::<ExecutionRow>()
            .fetch_all(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;

        Ok(crate::error::Page::from_overfetch(rows, limit, |row| {
            crate::error::Cursor::new(row.created_at.to_rfc3339(), row.id)
        }))
    }

    /// Applies a status change through the domain state machine.
    ///
    /// Spec 08.4: a terminal transition and the release of its concurrency slot
    /// commit together, so a slot can never leak.
    pub async fn transition(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        new_status: ExecutionStatus,
        error_class: Option<ErrorClass>,
        error_message: Option<&str>,
    ) -> Result<ExecutionRow> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            let current = sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {EXECUTION_COLUMNS} FROM executions
                 WHERE id = $1 AND tenant_id = $2 FOR UPDATE"
            )))
            .bind(execution_id)
            .bind(tenant_id.into_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?
            .ok_or_else(|| StorageError::not_found("execution"))?;

            // Replay the transition through the domain machine so storage can
            // never accept a transition the domain forbids.
            let mut exec = build_domain_execution(&current);
            exec.transition_to(new_status)
                .map_err(|e| StorageError::Validation(e.to_string()))?;

            let row = sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
                "UPDATE executions SET
                     status = $3,
                     attempt_count = $4,
                     started_at = $5,
                     ended_at = $6,
                     enqueued_at = $7,
                     error_class = COALESCE($8, error_class),
                     error_message = COALESCE($9, error_message),
                     -- Spec 02's side effects: becoming QUEUED clears the
                     -- pending retry, and a terminal state clears the deadline
                     -- so the timeout sweeper stops looking at it.
                     retry_at = CASE WHEN $3 = 'QUEUED' THEN NULL ELSE retry_at END,
                     deadline_at = CASE
                         WHEN $3 IN ('SUCCEEDED','FAILED','CANCELLED','TIMED_OUT',
                                     'DEAD_LETTERED','RETRY_SCHEDULED')
                         THEN NULL ELSE deadline_at END,
                     updated_at = NOW()
                 WHERE id = $1 AND tenant_id = $2
                 RETURNING {EXECUTION_COLUMNS}"
            )))
            .bind(execution_id)
            .bind(tenant_id.into_uuid())
            .bind(new_status.as_str())
            .bind(exec.attempt_count as i32)
            .bind(exec.started_at)
            .bind(exec.ended_at)
            .bind(exec.enqueued_at)
            .bind(error_class.map(|c| c.as_str().to_string()))
            .bind(error_message)
            .fetch_one(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok(row)
        }
        .await;

        match result {
            Ok(row) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(row)
            }
            Err(e) => {
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }

    /// Claims the next dispatchable execution for a queue.
    ///
    /// `FOR UPDATE SKIP LOCKED` (spec 04.7 / 08.5) lets several dispatchers
    /// run concurrently: each claims a disjoint set rather than blocking on the
    /// same row.
    ///
    /// Ordering is by priority then age. Priority uses the spec 02.13 weights
    /// plus an aging term, computed in SQL so a long-waiting low-priority
    /// execution eventually outranks a fresh high-priority one and cannot be
    /// starved.
    pub async fn claim_next(
        &self,
        tenant_id: TenantId,
        queue_id: Option<Uuid>,
        worker_id: Uuid,
    ) -> Result<Option<ExecutionRow>> {
        let priority_score = "\
            CASE priority \
                WHEN 'CRITICAL' THEN 1000 \
                WHEN 'HIGH' THEN 750 \
                WHEN 'NORMAL' THEN 500 \
                WHEN 'LOW' THEN 250 \
                WHEN 'BACKGROUND' THEN 100 \
                ELSE 0 \
            END \
            + FLOOR(EXTRACT(EPOCH FROM (NOW() - e.created_at)))::bigint";

        let sql = format!(
            "UPDATE executions SET status = 'DISPATCHED', worker_id = $3, enqueued_at = NOW(),
                    -- The ceiling is the one the version agreed to; a version
                    -- without a timeout falls back to one hour (spec 10.7).
                    deadline_at = NOW() + make_interval(secs => COALESCE(
                        (SELECT timeout_seconds FROM job_versions
                          WHERE id = executions.job_version_id), 3600)),
                    updated_at = NOW()
             WHERE id = (
                 SELECT e.id
                 FROM executions e
                 -- The concurrency policy lives on the version an execution
                 -- runs, not on the job itself.
                 LEFT JOIN job_versions jv ON jv.id = e.job_version_id
                 LEFT JOIN queues q ON q.id = e.queue_id
                 JOIN workers w ON w.id = $3
                 WHERE e.status = 'QUEUED'
                   AND e.tenant_id = $1
                   -- A workflow run is driven by the server, never claimed by a
                   -- worker, so it must not enter the dispatch queue.
                   AND e.job_id IS NOT NULL
                   AND ($2::uuid IS NULL OR e.queue_id = $2)
                   -- A paused queue must stop receiving work (spec 10.10).
                   AND (e.queue_id IS NULL OR q.paused = FALSE)
                   -- Queue max concurrency:
                   AND (
                       e.queue_id IS NULL
                       OR q.max_concurrency IS NULL
                       OR (
                           SELECT COUNT(*) FROM executions q_active
                           WHERE q_active.queue_id = e.queue_id
                             AND q_active.tenant_id = e.tenant_id
                             AND q_active.status IN ('DISPATCHED','RUNNING')
                       ) < q.max_concurrency
                   )
                   -- Worker capability matching:
                   AND (
                       w.capabilities @> '[\"*\"]'::jsonb
                       OR jv.resource_requirements IS NULL
                       OR jv.resource_requirements->'worker_capabilities' IS NULL
                       OR jsonb_typeof(jv.resource_requirements->'worker_capabilities') <> 'array'
                       OR jsonb_array_length(jv.resource_requirements->'worker_capabilities') = 0
                       OR (
                           jsonb_typeof(w.capabilities) = 'array'
                           AND w.capabilities @> (jv.resource_requirements->'worker_capabilities')
                       )
                   )
                   -- A revoked, offline, or draining worker takes no work.
                   AND w.tenant_id = $1
                   AND w.status IN ('READY','BUSY')
                   AND w.draining = FALSE
                   -- Concurrency is enforced here, not only in tests, so a
                   -- bounded policy cannot be exceeded. The limit lives at
                   -- `max_concurrent_executions`; `Unlimited` carries no `Bounded`
                   -- key, and the NULL test treats that as unbounded.
                   --
                   -- The candidate row is excluded from the count: a limit of 1
                   -- must admit the first dispatch, and counting the queued row
                   -- against itself would reject it and starve the job forever.
                   AND (
                       (jv.concurrency_policy->'max_concurrent_executions'->>'Bounded')::int
                         IS NULL
                       OR (
                           SELECT COUNT(*) FROM executions active
                           WHERE active.id <> e.id
                             AND active.tenant_id = e.tenant_id
                             AND active.status IN ('SCHEDULED','DISPATCHED',
                                                    'RUNNING','RETRY_SCHEDULED',
                                                    'CANCEL_REQUESTED','ABANDONED')
                             -- The scope decides which rows count against the
                             -- limit, so a JOB-scoped policy ignores the rest of
                             -- the tenant.
                             AND CASE COALESCE(jv.concurrency_policy->>'scope','JOB')
                                 WHEN 'JOB' THEN active.job_id = e.job_id
                                 WHEN 'QUEUE' THEN active.queue_id IS NOT DISTINCT FROM e.queue_id
                                 ELSE TRUE
                             END
                       ) < (jv.concurrency_policy->'max_concurrent_executions'->>'Bounded')::int
                   )
                 ORDER BY ({priority_score}) DESC, e.created_at ASC
                 FOR UPDATE OF e SKIP LOCKED
                 LIMIT 1
             )
             RETURNING {EXECUTION_COLUMNS}"
        );

        // `priority_score` is a string literal and `sql` interpolates only that
        // plus a `const *_COLUMNS` list, so no user input reaches this string.
        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(sql))
            .bind(tenant_id.into_uuid())
            .bind(queue_id)
            .bind(worker_id)
            .fetch_optional(self.pool)
            .await
            .map_err(StorageError::from_sqlx)
    }

    /// Dispatches one *specific* queued execution to a named worker (spec 10.10).
    ///
    /// `claim_next` is the pull path: a worker asks for whatever is next.
    /// `POST /executions/{id}/dispatch` is the push path and already knows which
    /// execution it means, so it must not re-derive a worker from the execution
    /// id. The transition is replayed through the domain machine, and the worker
    /// must still be able to accept work in this tenant.
    pub async fn dispatch_to(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        worker_id: Uuid,
    ) -> Result<Option<ExecutionRow>> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            let current = sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
                "SELECT {EXECUTION_COLUMNS} FROM executions
                 WHERE id = $1 AND tenant_id = $2 FOR UPDATE"
            )))
            .bind(execution_id)
            .bind(tenant_id.into_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            let Some(current) = current else {
                return Ok(None);
            };

            let mut exec = build_domain_execution(&current);
            exec.transition_to(ExecutionStatus::Dispatched)
                .map_err(|e| StorageError::Validation(e.to_string()))?;

            // A paused queue must not receive work, even by explicit dispatch.
            if let Some(queue_id) = current.queue_id {
                let queue_info: Option<(bool, Option<i32>)> = sqlx::query_as(
                    "SELECT paused, max_concurrency FROM queues WHERE id = $1 AND tenant_id = $2",
                )
                .bind(queue_id)
                .bind(tenant_id.into_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(StorageError::from_sqlx)?;
                if let Some((paused, max_concurrency)) = queue_info {
                    if paused {
                        return Err(StorageError::Validation(
                            "the execution's queue is paused".into(),
                        ));
                    }
                    if let Some(max_c) = max_concurrency {
                        let active_count: i64 = sqlx::query_scalar(
                            "SELECT COUNT(*) FROM executions
                             WHERE queue_id = $1 AND tenant_id = $2
                               AND status IN ('DISPATCHED','RUNNING')",
                        )
                        .bind(queue_id)
                        .bind(tenant_id.into_uuid())
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(StorageError::from_sqlx)?;

                        if active_count >= max_c as i64 {
                            return Err(StorageError::Validation(
                                "queue max concurrency limit reached".into(),
                            ));
                        }
                    }
                }
            }

            let dispatchable: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
                "SELECT id, capabilities FROM workers
                 WHERE id = $1 AND tenant_id = $2
                   AND status IN ('READY','BUSY') AND draining = FALSE",
            )
            .bind(worker_id)
            .bind(tenant_id.into_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            let Some((_, worker_caps_val)) = dispatchable else {
                return Err(StorageError::Validation(
                    "the worker is unknown, offline, revoked or draining".into(),
                ));
            };

            // Check worker capabilities if job specifies any
            if let Some(jv_id) = current.job_version_id {
                let reqs: Option<serde_json::Value> = sqlx::query_scalar(
                    "SELECT resource_requirements FROM job_versions WHERE id = $1",
                )
                .bind(jv_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(StorageError::from_sqlx)?;

                if let Some(req_val) = reqs {
                    if let Some(req_caps) = req_val
                        .get("worker_capabilities")
                        .and_then(|c| c.as_array())
                    {
                        if !req_caps.is_empty() {
                            let w_caps = worker_caps_val.as_array();
                            let has_wildcard = w_caps
                                .map(|caps| caps.iter().any(|c| c.as_str() == Some("*")))
                                .unwrap_or(false);
                            if !has_wildcard {
                                let has_all = w_caps
                                    .map(|caps| {
                                        req_caps.iter().all(|req_cap| caps.contains(req_cap))
                                    })
                                    .unwrap_or(false);
                                if !has_all {
                                    return Err(StorageError::Validation(
                                        "worker lacks required capabilities for this job".into(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            // A workflow run has no job version, so there is no timeout to read;
            // the dispatch below falls back to the default ceiling.
            let timeout_seconds: Option<i32> = sqlx::query_scalar::<_, i32>(
                "SELECT timeout_seconds FROM job_versions WHERE id = $1",
            )
            .bind(current.job_version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            let row = sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
                "UPDATE executions SET status = $3, worker_id = $4, enqueued_at = NOW(),
                        deadline_at = NOW() + make_interval(secs => COALESCE($5, 3600)),
                        updated_at = NOW()
                 WHERE id = $1 AND tenant_id = $2
                 RETURNING {EXECUTION_COLUMNS}"
            )))
            .bind(execution_id)
            .bind(tenant_id.into_uuid())
            .bind(ExecutionStatus::Dispatched.as_str())
            .bind(worker_id)
            .bind(timeout_seconds)
            .fetch_one(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok(Some(row))
        }
        .await;

        match result {
            Ok(row) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(row)
            }
            Err(error) => {
                let _ = tx.rollback().await;
                Err(error)
            }
        }
    }

    /// Stores the result a completed attempt produced (spec 10.4).
    ///
    /// Completion carries a result "where applicable"; without persisting it the
    /// worker's answer is accepted, acknowledged, and then silently dropped —
    /// downstream nodes and the console both see nothing.
    pub async fn record_output(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        output: &serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE executions SET output = $3, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(execution_id)
        .bind(tenant_id.into_uuid())
        .bind(output)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// Moves a failed execution to `RETRY_SCHEDULED` with the instant it may run
    /// again (spec 10.8, spec 10.9).
    ///
    /// The delay is computed by the caller from the version's retry policy, so
    /// storage records the decision rather than re-deriving it.
    pub async fn schedule_retry(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        retry_at: DateTime<Utc>,
    ) -> Result<ExecutionRow> {
        // The state machine requires `FAILED`/`TIMED_OUT` -> `RETRY_SCHEDULED`,
        // so the caller has already recorded the failure; this only records
        // *when* the retry may run.
        self.transition(
            tenant_id,
            execution_id,
            ExecutionStatus::RetryScheduled,
            None,
            None,
        )
        .await?;

        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE executions SET retry_at = $3, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 RETURNING {EXECUTION_COLUMNS}"
        )))
        .bind(execution_id)
        .bind(tenant_id.into_uuid())
        .bind(retry_at)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Retries whose delay has elapsed, oldest first.
    ///
    /// Ordered by `retry_at` rather than `updated_at` so a retry actually waits
    /// the backoff it was given instead of the sweep interval.
    pub async fn due_retries(&self, limit: i64) -> Result<Vec<ExecutionRow>> {
        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE executions SET status = 'QUEUED', retry_at = NULL, updated_at = NOW()
             WHERE id IN (
                 SELECT id FROM executions
                 WHERE status = 'RETRY_SCHEDULED'
                   AND retry_at IS NOT NULL
                   AND retry_at <= NOW()
                 ORDER BY retry_at ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             )
             RETURNING {EXECUTION_COLUMNS}"
        )))
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Executions that have outlived the timeout their version set (spec 10.7).
    ///
    /// Only `DISPATCHED` and `RUNNING` rows are eligible: a queued execution has
    /// not started, so its deadline has no meaning yet.
    pub async fn overdue(&self, limit: i64) -> Result<Vec<ExecutionRow>> {
        sqlx::query_as::<_, ExecutionRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {EXECUTION_COLUMNS} FROM executions
             WHERE status IN ('DISPATCHED', 'RUNNING')
               AND deadline_at IS NOT NULL
               AND deadline_at <= NOW()
             ORDER BY deadline_at ASC
             LIMIT $1"
        )))
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Counts non-terminal executions for a scope, used by concurrency checks.
    pub async fn count_active_for_job(&self, tenant_id: TenantId, job_id: Uuid) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM executions
             WHERE tenant_id = $1 AND job_id = $2
               AND status IN ('SCHEDULED','QUEUED','DISPATCHED','RUNNING',
                              'RETRY_SCHEDULED','CANCEL_REQUESTED','ABANDONED')",
        )
        .bind(tenant_id.into_uuid())
        .bind(job_id)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(row.0)
    }

    pub async fn count_active_for_tenant(&self, tenant_id: TenantId) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM executions
             WHERE tenant_id = $1
               AND status IN ('SCHEDULED','QUEUED','DISPATCHED','RUNNING',
                              'RETRY_SCHEDULED','CANCEL_REQUESTED','ABANDONED')",
        )
        .bind(tenant_id.into_uuid())
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(row.0)
    }

    pub async fn count_active_for_queue(&self, queue_id: Uuid) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM executions
             WHERE queue_id = $1
               AND status IN ('SCHEDULED','QUEUED','DISPATCHED','RUNNING',
                              'RETRY_SCHEDULED','CANCEL_REQUESTED','ABANDONED')",
        )
        .bind(queue_id)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(row.0)
    }

    /// Whether the effective concurrency policy admits another execution.
    pub async fn concurrency_admits(
        &self,
        tenant_id: TenantId,
        job_id: Uuid,
        queue_id: Option<Uuid>,
        policy: &ConcurrencyPolicy,
    ) -> Result<bool> {
        let active = match policy.scope {
            forge_domain::ConcurrencyScope::Job => {
                self.count_active_for_job(tenant_id, job_id).await?
            }
            forge_domain::ConcurrencyScope::Tenant => {
                self.count_active_for_tenant(tenant_id).await?
            }
            forge_domain::ConcurrencyScope::Queue => match queue_id {
                Some(q) => self.count_active_for_queue(q).await?,
                // A queue-scoped limit with no queue to count is unconstrained.
                None => return Ok(true),
            },
            // Workflow and global scopes are enforced by the workflow engine
            // and the process-wide counter respectively.
            forge_domain::ConcurrencyScope::Workflow | forge_domain::ConcurrencyScope::Global => {
                return Ok(true)
            }
        };
        Ok(policy.admits(active.clamp(0, u32::MAX as i64) as u32))
    }

    /// Requests cancellation (spec 10.6). Returns the updated row.
    pub async fn request_cancel(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
    ) -> Result<ExecutionRow> {
        self.transition(
            tenant_id,
            execution_id,
            ExecutionStatus::CancelRequested,
            None,
            None,
        )
        .await
    }
}

/// Rebuilds a domain `Execution` from a stored row so the state machine can be
/// applied without duplicating transition rules in SQL.
///
/// A workflow run has no job, but the transition machine never reads `job_id` —
/// it only walks statuses — so the nil id stands in for "not job work" rather
/// than being a meaningful reference.
fn build_domain_execution(row: &ExecutionRow) -> forge_domain::Execution {
    forge_domain::Execution {
        id: forge_domain::ExecutionId::from_uuid(row.id),
        tenant_id: forge_domain::TenantId::from_uuid(row.tenant_id),
        job_id: forge_domain::JobId::from_uuid(row.job_id.unwrap_or_default()),
        job_version_id: forge_domain::JobVersionId::from_uuid(
            row.job_version_id.unwrap_or_default(),
        ),
        status: row.status.parse().unwrap_or(ExecutionStatus::Queued),
        worker_id: row.worker_id.map(forge_domain::WorkerId::from_uuid),
        attempt_count: row.attempt_count.max(0) as u32,
        trigger_source: parse_trigger_source(&row.trigger_source),
        correlation_id: row.correlation_id.clone(),
        scheduled_for: row.scheduled_for,
        enqueued_at: row.enqueued_at,
        retry_at: None,
        error_class: row
            .error_class
            .as_deref()
            .and_then(|c| serde_json::from_value(serde_json::Value::String(c.to_string())).ok()),
        error_message: row.error_message.clone(),
        created_at: row.created_at,
        started_at: row.started_at,
        ended_at: row.ended_at,
        metrics: Default::default(),
    }
}

fn parse_trigger_source(raw: &str) -> TriggerSource {
    match raw {
        "SCHEDULE" => TriggerSource::Schedule,
        "API" => TriggerSource::Api,
        "WORKFLOW" => TriggerSource::Workflow,
        "RETRY" => TriggerSource::Retry,
        "RECOVERY" => TriggerSource::Recovery,
        _ => TriggerSource::Manual,
    }
}

/// Re-exported so callers can build a bounded policy without importing the
/// domain crate directly.
pub fn bounded_concurrency(limit: u32) -> ConcurrencyPolicy {
    ConcurrencyPolicy {
        max_concurrent_executions: ConcurrencyLimit::Bounded(limit),
        ..Default::default()
    }
}

/// The tenant's `default` queue, created on first use.
///
/// Work that names no queue still has to be addressable by a worker polling a
/// queue, and queues are how workers ask for work. Resolving the default inside
/// the same transaction as the insert keeps the invariant "every execution has a
/// queue" true without a separate reconciliation pass.
async fn default_queue_id(conn: &mut sqlx::PgConnection, tenant_id: TenantId) -> Result<Uuid> {
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM queues WHERE tenant_id = $1 AND name = 'default'")
            .bind(tenant_id.into_uuid())
            .fetch_optional(&mut *conn)
            .await
            .map_err(StorageError::from_sqlx)?;

    if let Some((id,)) = existing {
        return Ok(id);
    }

    // `DO NOTHING` rather than an upsert: a concurrent insert of the same queue
    // is not an error, and the follow-up select returns whichever row won.
    sqlx::query(
        "INSERT INTO queues (id, tenant_id, name) VALUES ($1, $2, 'default')
         ON CONFLICT (tenant_id, name) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.into_uuid())
    .execute(&mut *conn)
    .await
    .map_err(StorageError::from_sqlx)?;

    let (id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM queues WHERE tenant_id = $1 AND name = 'default'")
            .bind(tenant_id.into_uuid())
            .fetch_one(&mut *conn)
            .await
            .map_err(StorageError::from_sqlx)?;

    Ok(id)
}
