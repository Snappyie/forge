use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::{Result, StorageError};
use forge_domain::{
    CatchUpPolicy, MisfirePolicy, Schedule, ScheduleType, TargetType, TargetVersionPolicy, TenantId,
};

/// A due schedule claimed by one scheduler instance.
///
/// Claiming uses `FOR UPDATE SKIP LOCKED` (spec 08.5) so several schedulers
/// can poll the same table concurrently without contending for the same row.
#[derive(Debug, Clone, FromRow)]
pub struct DueSchedule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub target_version_policy: String,
    pub schedule_type: String,
    pub cron_expression: Option<String>,
    pub timezone: String,
    pub misfire_policy: String,
    pub catch_up_policy: serde_json::Value,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
}

impl DueSchedule {
    /// Rebuilds the domain schedule used for occurrence calculation.
    pub fn to_domain(&self, tenant: TenantId) -> Schedule {
        let catch_up_max = self
            .catch_up_policy
            .get("max_occurrences")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;

        Schedule {
            id: self.id,
            tenant_id: tenant,
            target_type: parse_target_type(&self.target_type),
            target_id: self.target_id,
            target_version_policy: parse_version_policy(&self.target_version_policy),
            schedule_type: parse_schedule_type(&self.schedule_type),
            expression: self.cron_expression.clone(),
            timezone: self.timezone.clone(),
            interval: None,
            one_time_at: None,
            misfire_policy: parse_misfire_policy(&self.misfire_policy),
            catch_up_policy: CatchUpPolicy {
                max_occurrences: catch_up_max,
            },
            enabled: self.enabled,
            next_run_at: Some(self.next_run_at),
            last_run_at: self.last_run_at,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

fn parse_target_type(raw: &str) -> TargetType {
    match raw {
        "WORKFLOW" => TargetType::Workflow,
        _ => TargetType::Job,
    }
}

fn parse_version_policy(raw: &str) -> TargetVersionPolicy {
    match raw {
        "PINNED" => TargetVersionPolicy::Pinned,
        _ => TargetVersionPolicy::LatestPublished,
    }
}

fn parse_schedule_type(raw: &str) -> ScheduleType {
    match raw {
        "ONE_TIME" => ScheduleType::OneTime,
        "INTERVAL" => ScheduleType::Interval,
        _ => ScheduleType::Cron,
    }
}

fn parse_misfire_policy(raw: &str) -> MisfirePolicy {
    match raw {
        "SKIP" => MisfirePolicy::Skip,
        "CATCH_UP" => MisfirePolicy::CatchUp,
        _ => MisfirePolicy::FireOnce,
    }
}

pub struct ScheduleRepository<'a> {
    pool: &'a PgPool,
}

/// A due schedule leased to one scheduler instance.
///
/// Field order matches the `RETURNING` clause in `claim_due`, which `FromRow`
/// relies on positionally.
#[derive(Debug, Clone, FromRow)]
pub struct ClaimedSchedule {
    /// Identifier of this claim, used to release it.
    pub lease_id: Uuid,
    pub lease_expires_at: DateTime<Utc>,
    /// The schedule row itself, flattened in.
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub target_version_policy: String,
    pub schedule_type: String,
    pub cron_expression: Option<String>,
    pub timezone: String,
    pub misfire_policy: String,
    pub catch_up_policy: serde_json::Value,
    pub enabled: bool,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
}

impl<'a> ScheduleRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Atomically claims up to `batch_size` due schedules and leases them to
    /// `worker_id`.
    ///
    /// A bare `SELECT ... FOR UPDATE SKIP LOCKED` would not be a claim at all:
    /// the row lock is released the instant the implicit transaction ends, which
    /// is as soon as the statement returns. Two schedulers would then both
    /// receive the same schedule. Writing a lease marker inside the same
    /// transaction is what makes the claim real, and `lease_expires_at` bounds
    /// how long a crashed scheduler can hold a schedule.
    pub async fn claim_due(
        &self,
        now: DateTime<Utc>,
        batch_size: i64,
        worker_id: Uuid,
        lease_secs: i64,
    ) -> Result<Vec<ClaimedSchedule>> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_sqlx)?;

        let result = async {
            let rows = sqlx::query_as::<_, ClaimedSchedule>(
                "UPDATE schedules SET
                     last_claimed_at = $2,
                     claimed_by = $3,
                     lease_expires_at = $2 + make_interval(secs => $4),
                     updated_at = NOW()
                 WHERE id IN (
                     SELECT id FROM schedules
                     WHERE enabled = TRUE
                       AND next_run_at IS NOT NULL
                       AND next_run_at <= $1
                       AND (lease_expires_at IS NULL OR lease_expires_at <= $1)
                     ORDER BY next_run_at ASC
                     FOR UPDATE SKIP LOCKED
                     LIMIT $5
                 )
                 RETURNING id AS lease_id, lease_expires_at, id, tenant_id, target_type,
                     target_id, target_version_policy, schedule_type, cron_expression,
                     timezone, misfire_policy, catch_up_policy, enabled, next_run_at, last_run_at",
            )
            .bind(now)
            .bind(now)
            .bind(worker_id)
            .bind(lease_secs)
            .bind(batch_size)
            .fetch_all(&mut *tx)
            .await
            .map_err(StorageError::from_sqlx)?;

            Ok(rows)
        }
        .await;

        match result {
            Ok(claimed) => {
                tx.commit().await.map_err(StorageError::from_sqlx)?;
                Ok(claimed)
            }
            Err(e) => {
                let _ = tx.rollback().await;
                Err(e)
            }
        }
    }

    /// Releases a claim so another scheduler may pick the schedule up.
    pub async fn release_claim(&self, schedule_id: Uuid, worker_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE schedules SET lease_expires_at = NULL, updated_at = NOW()
             WHERE id = $1 AND claimed_by = $2",
        )
        .bind(schedule_id)
        .bind(worker_id)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// Persists the next occurrence after an occurrence has been handled.
    ///
    /// The UPDATE is conditional on the occurrence the scheduler acted on, so a
    /// scheduler that acted on a stale value cannot overwrite a newer one.
    pub async fn advance_next_run(
        &self,
        schedule_id: Uuid,
        tenant_id: TenantId,
        from_occurrence: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> Result<bool> {
        let affected = sqlx::query(
            "UPDATE schedules SET next_run_at = $4, last_run_at = $3, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 AND next_run_at = $3",
        )
        .bind(schedule_id)
        .bind(tenant_id.into_uuid())
        .bind(from_occurrence)
        .bind(next_run_at)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();

        Ok(affected > 0)
    }

    /// Disables a schedule without touching running executions (spec 09.11).
    pub async fn pause(&self, tenant_id: TenantId, schedule_id: Uuid) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE schedules SET enabled = FALSE, is_paused = TRUE, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(schedule_id)
        .bind(tenant_id.into_uuid())
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();

        if affected == 0 {
            return Err(StorageError::not_found("schedule"));
        }
        Ok(())
    }

    pub async fn resume(&self, tenant_id: TenantId, schedule_id: Uuid) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE schedules SET enabled = TRUE, is_paused = FALSE, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(schedule_id)
        .bind(tenant_id.into_uuid())
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();

        if affected == 0 {
            return Err(StorageError::not_found("schedule"));
        }
        Ok(())
    }

    /// Occurrences already materialised for a schedule, so a repeated
    /// scheduler tick stays idempotent (spec 09.2).
    pub async fn materialised_occurrences(
        &self,
        schedule_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>> {
        let rows: Vec<(DateTime<Utc>,)> = sqlx::query_as(
            "SELECT scheduled_for FROM executions
             WHERE schedule_id = $1 AND scheduled_for IS NOT NULL AND scheduled_for >= $2",
        )
        .bind(schedule_id)
        .bind(since)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;

        Ok(rows.into_iter().map(|(t,)| t).collect())
    }
}

/// Row shape for `workers` (spec 02.8).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkerRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: Option<String>,
    pub hostname: String,
    pub version: Option<String>,
    pub status: String,
    pub capabilities: serde_json::Value,
    pub labels: serde_json::Value,
    pub draining: bool,
    pub last_heartbeat_at: DateTime<Utc>,
    pub registered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const WORKER_COLUMNS: &str = "id, tenant_id, name, hostname, version, status, capabilities, \
     labels, draining, last_heartbeat_at, registered_at, created_at, updated_at";

pub struct WorkerRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> WorkerRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn register(
        &self,
        tenant_id: TenantId,
        name: &str,
        hostname: &str,
        version: Option<&str>,
        capabilities: serde_json::Value,
        labels: serde_json::Value,
    ) -> Result<WorkerRow> {
        sqlx::query_as::<_, WorkerRow>(&format!(
            "INSERT INTO workers
                 (id, tenant_id, name, hostname, version, status, capabilities, labels,
                  last_heartbeat_at, registered_at)
             VALUES ($1, $2, $3, $4, $5, 'READY', $6, $7, NOW(), NOW())
             RETURNING {WORKER_COLUMNS}"
        ))
        .bind(Uuid::new_v4())
        .bind(tenant_id.into_uuid())
        .bind(name)
        .bind(hostname)
        .bind(version)
        .bind(capabilities)
        .bind(labels)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    pub async fn get(&self, tenant_id: TenantId, worker_id: Uuid) -> Result<WorkerRow> {
        sqlx::query_as::<_, WorkerRow>(&format!(
            "SELECT {WORKER_COLUMNS} FROM workers WHERE id = $1 AND tenant_id = $2"
        ))
        .bind(worker_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("worker"))
    }

    /// Refreshes liveness. A heartbeat that names an unknown worker fails, so
    /// a revoked worker cannot silently keep reporting healthy.
    pub async fn heartbeat(&self, tenant_id: TenantId, worker_id: Uuid) -> Result<WorkerRow> {
        sqlx::query_as::<_, WorkerRow>(&format!(
            "UPDATE workers SET last_heartbeat_at = NOW(), updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 AND status NOT IN ('OFFLINE', 'REVOKED')
             RETURNING {WORKER_COLUMNS}"
        ))
        .bind(worker_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("worker"))
    }

    /// Draining workers stop receiving new work but finish what they hold
    /// (spec 10.1).
    pub async fn drain(&self, tenant_id: TenantId, worker_id: Uuid) -> Result<WorkerRow> {
        sqlx::query_as::<_, WorkerRow>(&format!(
            "UPDATE workers SET status = 'DRAINING', draining = TRUE, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2 RETURNING {WORKER_COLUMNS}"
        ))
        .bind(worker_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or_else(|| StorageError::not_found("worker"))
    }

    pub async fn revoke(&self, tenant_id: TenantId, worker_id: Uuid) -> Result<()> {
        let affected = sqlx::query(
            "UPDATE workers SET status = 'REVOKED', draining = TRUE, updated_at = NOW()
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(worker_id)
        .bind(tenant_id.into_uuid())
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .rows_affected();

        if affected == 0 {
            return Err(StorageError::not_found("worker"));
        }
        Ok(())
    }

    pub async fn list(
        &self,
        tenant_id: TenantId,
        status: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<crate::error::Page<WorkerRow>> {
        let decoded = match cursor {
            Some(token) => Some(crate::error::Cursor::decode(token)?),
            None => None,
        };

        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {WORKER_COLUMNS} FROM workers WHERE tenant_id = "
        ));
        qb.push_bind(tenant_id.into_uuid());
        if let Some(status) = status {
            qb.push(" AND status = ").push_bind(status);
        }
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
            .build_query_as::<WorkerRow>()
            .fetch_all(self.pool)
            .await
            .map_err(StorageError::from_sqlx)?;

        Ok(crate::error::Page::from_overfetch(rows, limit, |row| {
            crate::error::Cursor::new(row.created_at.to_rfc3339(), row.id)
        }))
    }

    /// Workers that are eligible to receive work (spec 10.10).
    pub async fn find_ready(&self, tenant_id: TenantId, limit: i64) -> Result<Vec<WorkerRow>> {
        sqlx::query_as::<_, WorkerRow>(&format!(
            "SELECT {WORKER_COLUMNS} FROM workers
             WHERE tenant_id = $1 AND status = 'READY' AND draining = FALSE
               AND last_heartbeat_at > NOW() - INTERVAL '60 seconds'
             ORDER BY last_heartbeat_at DESC LIMIT $2"
        ))
        .bind(tenant_id.into_uuid())
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Marks workers whose heartbeat lapsed as offline (spec 10.11).
    pub async fn reap_stale(&self, stale_after_secs: i64) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE workers SET status = 'OFFLINE', updated_at = NOW()
             WHERE status IN ('READY', 'BUSY')
               AND last_heartbeat_at < NOW() - make_interval(secs => $1)",
        )
        .bind(stale_after_secs)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(result.rows_affected())
    }
}

/// Row shape for `worker_leases` (spec 02.9).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct LeaseRow {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub worker_id: Uuid,
    pub attempt_id: Option<Uuid>,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub renewed_at: Option<DateTime<Utc>>,
    pub released_at: Option<DateTime<Utc>>,
}

pub struct LeaseRepository<'a> {
    pool: &'a PgPool,
}

impl<'a> LeaseRepository<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Acquires an exclusive lease on an execution.
    ///
    /// The partial unique index on `(execution_id) WHERE released_at IS NULL`
    /// makes ownership exclusive at the database level, so two workers cannot
    /// both believe they hold the same execution.
    pub async fn acquire(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        worker_id: Uuid,
        attempt_id: Option<Uuid>,
        ttl_secs: i64,
    ) -> Result<LeaseRow> {
        sqlx::query_as::<_, LeaseRow>(
            "INSERT INTO worker_leases
                 (id, tenant_id, execution_id, worker_id, attempt_id, expires_at, renewed_at)
             VALUES ($1, $2, $3, $4, $5, NOW() + make_interval(secs => $6), NOW())
             RETURNING id, execution_id, worker_id, attempt_id, acquired_at, expires_at,
                       renewed_at, released_at",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id.into_uuid())
        .bind(execution_id)
        .bind(worker_id)
        .bind(attempt_id)
        .bind(ttl_secs)
        .fetch_one(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// Renews a lease, verifying ownership (spec 10.3, invariant 8).
    ///
    /// Only the current holder can renew; a stale holder's renewal affects zero
    /// rows and is reported as a conflict rather than silently succeeding.
    pub async fn renew(&self, lease_id: Uuid, worker_id: Uuid, ttl_secs: i64) -> Result<LeaseRow> {
        sqlx::query_as::<_, LeaseRow>(
            "UPDATE worker_leases
             SET expires_at = NOW() + make_interval(secs => $3), renewed_at = NOW()
             WHERE id = $1 AND worker_id = $2 AND released_at IS NULL AND expires_at > NOW()
             RETURNING id, execution_id, worker_id, attempt_id, acquired_at, expires_at,
                       renewed_at, released_at",
        )
        .bind(lease_id)
        .bind(worker_id)
        .bind(ttl_secs)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?
        .ok_or(StorageError::Conflict(
            "lease is not held by this worker or has expired".to_string(),
        ))
    }

    pub async fn release(&self, lease_id: Uuid, worker_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE worker_leases SET released_at = NOW()
             WHERE id = $1 AND worker_id = $2 AND released_at IS NULL",
        )
        .bind(lease_id)
        .bind(worker_id)
        .execute(self.pool)
        .await
        .map_err(StorageError::from_sqlx)?;
        Ok(())
    }

    /// Active leases that have passed their expiry, for the reaper
    /// (spec 10.11).
    pub async fn claim_expired(&self, limit: i64) -> Result<Vec<LeaseRow>> {
        sqlx::query_as::<_, LeaseRow>(
            "SELECT id, execution_id, worker_id, attempt_id, acquired_at, expires_at,
                    renewed_at, released_at
             FROM worker_leases
             WHERE released_at IS NULL AND expires_at <= NOW()
             ORDER BY expires_at ASC
             FOR UPDATE SKIP LOCKED
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }

    /// The active lease on an execution, if any.
    pub async fn active_for_execution(&self, execution_id: Uuid) -> Result<Option<LeaseRow>> {
        sqlx::query_as::<_, LeaseRow>(
            "SELECT id, execution_id, worker_id, attempt_id, acquired_at, expires_at,
                    renewed_at, released_at
             FROM worker_leases
             WHERE execution_id = $1 AND released_at IS NULL",
        )
        .bind(execution_id)
        .fetch_optional(self.pool)
        .await
        .map_err(StorageError::from_sqlx)
    }
}
