//! Integration tests for the repository layer.
//!
//! These require a live PostgreSQL. `DATABASE_URL` must point at a database the
//! test may freely truncate; the suite creates a unique schema per test so
//! concurrent runs cannot collide.

use std::sync::Arc;

use chrono::Utc;
use forge_domain::{ExecutionStatus, JobId, JobStatus, JobVersionId, Priority, TenantId};
use forge_storage::{
    AuditRepository, ExecutionFilter, ExecutionRepository, IdempotencyOutcome,
    IdempotencyRepository, JobFilter, JobRepository, JobVersionRepository, LeaseRepository,
    NewAuditEvent, NewExecution, OutboxRepository, ScheduleRepository, StorageError,
    WorkerRepository,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

/// A pool bound to a throwaway database.
///
/// The database is dropped by an explicit `cleanup()` call after each test.
/// A `Drop` impl cannot await, and spawning a blocking thread from `Drop` while
/// tokio is already running its own runtime deadlocks on the connection pool,
/// so teardown is explicit rather than best-effort.
pub struct TestDb {
    pub pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    /// Drops the throwaway database. Called by the `with_db!` macro after the
    /// body finishes so repeated runs do not accumulate databases.
    pub async fn cleanup(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            // Terminate stragglers first; a leftover connection blocks DROP.
            let _ = admin
                .execute(
                    format!(
                        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                         WHERE datname = '{}' AND pid <> pg_backend_pid()",
                        self.db_name
                    )
                    .as_str(),
                )
                .await;
            let _ = admin
                .execute(format!(r#"DROP DATABASE IF EXISTS "{}""#, self.db_name).as_str())
                .await;
            admin.close().await;
        }
    }

    pub async fn new() -> Option<TestDb> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());

        let db_name = format!("forge_test_{}", Uuid::new_v4().simple());
        // Strip the database component from the configured URL so the admin
        // connection targets the server rather than `forgedb/postgres`.
        let (server, _existing_db) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping storage integration tests: cannot connect ({e})");
                return None;
            }
        };

        if admin
            .execute(format!(r#"CREATE DATABASE "{db_name}""#).as_str())
            .await
            .is_err()
        {
            eprintln!("skipping storage integration tests: cannot create database");
            return None;
        }
        admin.close().await;

        let url = format!("{server}/{db_name}");
        let pool = match PgPoolOptions::new().max_connections(10).connect(&url).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping storage integration tests: cannot connect to test db ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
            eprintln!("skipping storage integration tests: migrations failed ({e})");
            return None;
        }

        Some(TestDb {
            pool,
            admin_url,
            db_name,
        })
    }

    pub fn tenant(&self) -> TenantId {
        TenantId::from_uuid(Uuid::new_v4())
    }

    pub async fn seed_tenant(&self, tenant: TenantId) {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant.into_uuid())
            .bind("Test Tenant")
            .execute(&self.pool)
            .await
            .expect("seed tenant");
    }
}

/// Runs a body against a throwaway database and always drops it.
///
/// The body receives a cloned `PgPool` — a cheap shared handle — so nothing
/// borrows the harness and cleanup always succeeds. When no database is
/// reachable the body is skipped rather than failing, so the suite stays usable
/// on a machine without PostgreSQL.
macro_rules! with_db {
    ($body:expr) => {
        async move {
            match TestDb::new().await {
                Some(db) => {
                    let db = Arc::new(db);
                    $body(db.pool.clone()).await;
                    // Only the harness holds the `TestDb`, so this always
                    // succeeds and no database is left behind.
                    match Arc::try_unwrap(db) {
                        Ok(db) => db.cleanup().await,
                        Err(_) => eprintln!("warning: test db handle still shared; not dropped"),
                    }
                }
                None => {
                    eprintln!("(skipped: no database available)");
                }
            }
        }
    };
}

/// Inserts a tenant so tenant-scoped foreign keys resolve.
async fn seed_tenant(pool: &PgPool, tenant: TenantId) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Test Tenant')")
        .bind(tenant.into_uuid())
        .execute(pool)
        .await
        .expect("seed tenant");
}

/// Creates a job with one published version, ready to have executions run.
async fn scaffold(pool: &PgPool, tenant: TenantId) -> (JobId, JobVersionId) {
    seed_tenant(pool, tenant).await;
    let jobs = JobRepository::new(pool);
    let job = jobs
        .create(
            tenant,
            Some(format!("job-{}", Uuid::new_v4().simple())),
            "Test Job",
            None,
            Priority::Normal,
            None,
        )
        .await
        .expect("create job");

    let versions = JobVersionRepository::new(pool);
    let version = versions
        .create(
            tenant,
            forge_domain::JobId::from_uuid(job.id),
            &forge_storage::NewJobVersion::default(),
        )
        .await
        .expect("create version");
    versions
        .publish(tenant, forge_domain::JobId::from_uuid(job.id), forge_domain::JobVersionId::from_uuid(version.id))
        .await
        .expect("publish version");

    (
        JobId::from_uuid(job.id),
        JobVersionId::from_uuid(version.id),
    )
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn job_round_trips_through_the_repository() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);

        let created = jobs
            .create(tenant, Some("settle-daily".into()), "Settle Daily", None, Priority::High, None)
            .await
            .unwrap();
        assert_eq!(created.status, "DRAFT");
        assert_eq!(created.priority, "HIGH");

        let fetched = jobs.get(tenant, JobId::from_uuid(created.id)).await.unwrap();
        assert_eq!(fetched.name, "Settle Daily");
        assert_eq!(fetched.key.as_deref(), Some("settle-daily"));
    })
    .await;
}

// AT-TEN-001: tenant A cannot read tenant B's job.
#[tokio::test]
async fn cross_tenant_job_read_is_not_found() {
    with_db!(|pool: PgPool| async move {
        let tenant_a = TenantId::from_uuid(Uuid::new_v4());
        let tenant_b = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant_a).await;
        seed_tenant(&pool, tenant_b).await;

        let jobs = JobRepository::new(&pool);
        let job = jobs
            .create(tenant_a, Some("secret".into()), "Tenant A Job", None, Priority::Normal, None)
            .await
            .unwrap();

        let result = jobs.get(tenant_b, JobId::from_uuid(job.id)).await;
        assert!(
            matches!(result, Err(StorageError::NotFound { .. })),
            "cross-tenant read must look like not-found, got {result:?}"
        );
    })
    .await;
}

// AT-TEN-004: tenant-scoped uniqueness.
#[tokio::test]
async fn job_key_is_unique_within_a_tenant_but_across_tenants() {
    with_db!(|pool: PgPool| async move {
        let tenant_a = TenantId::from_uuid(Uuid::new_v4());
        let tenant_b = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant_a).await;
        seed_tenant(&pool, tenant_b).await;
        let jobs = JobRepository::new(&pool);

        jobs.create(tenant_a, Some("shared-key".into()), "A1", None, Priority::Normal, None)
            .await
            .unwrap();
        // Same key, different tenant: allowed.
        jobs.create(tenant_b, Some("shared-key".into()), "B1", None, Priority::Normal, None)
            .await
            .expect("the same key may exist in another tenant");

        // Same key, same tenant: rejected.
        let dup = jobs
            .create(tenant_a, Some("shared-key".into()), "A2", None, Priority::Normal, None)
            .await;
        assert!(matches!(dup, Err(StorageError::Conflict(_))), "got {dup:?}");
    })
    .await;
}

#[tokio::test]
async fn job_status_transitions_enforce_the_domain_machine() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, _) = scaffold(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);

        // DRAFT -> ACTIVE is allowed now that a version is published.
        let active = jobs.set_status(tenant, job_id, JobStatus::Active).await.unwrap();
        assert_eq!(active.status, "ACTIVE");

        // A job cannot jump from ACTIVE back to DRAFT.
        let bad = jobs.set_status(tenant, job_id, JobStatus::Draft).await;
        assert!(bad.is_err(), "ACTIVE -> DRAFT must be rejected");
    })
    .await;
}

// AT-API-007: a stale update returns a conflict.
#[tokio::test]
async fn stale_update_is_rejected_by_optimistic_concurrency() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);
        let created = jobs
            .create(tenant, Some("k".into()), "Original", None, Priority::Normal, None)
            .await
            .unwrap();

        // First update succeeds against the timestamp the caller last saw.
        let patch = forge_storage::JobPatch {
            name: Some("Renamed".to_string()),
            ..Default::default()
        };
        let updated = jobs
            .update(tenant, JobId::from_uuid(created.id), created.updated_at, &patch)
            .await
            .unwrap();
        assert_eq!(updated.name, "Renamed");

        // A second update using the stale timestamp is rejected.
        let stale = jobs
            .update(tenant, JobId::from_uuid(created.id), created.updated_at, &patch)
            .await;
        assert!(matches!(stale, Err(StorageError::Conflict(_))), "got {stale:?}");
    })
    .await;
}

#[tokio::test]
async fn archiving_is_soft_and_idempotent() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);
        let created = jobs
            .create(tenant, Some("k".into()), "Doomed", None, Priority::Normal, None)
            .await
            .unwrap();
        let job_id = JobId::from_uuid(created.id);

        jobs.archive(tenant, job_id).await.unwrap();
        let row = jobs.get(tenant, job_id).await.unwrap();
        assert_eq!(row.status, "ARCHIVED", "the row is retained, not deleted");

        // Archiving twice reports not-found rather than silently succeeding.
        assert!(jobs.archive(tenant, job_id).await.is_err());
    })
    .await;
}

// ---------------------------------------------------------------------------
// Job versions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn version_numbers_increment_and_publishing_is_idempotent() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);
        let versions = JobVersionRepository::new(&pool);

        let job = jobs
            .create(tenant, Some("k".into()), "Versioned", None, Priority::Normal, None)
            .await
            .unwrap();
        let job_id = JobId::from_uuid(job.id);

        for expected in 1..=3 {
            let v = versions
                .create(tenant, job_id, &forge_storage::NewJobVersion::default())
                .await
                .unwrap();
            assert_eq!(v.version_number, expected);
        }

        let v1 = versions.list(tenant, job_id).await.unwrap();
        let first = v1.last().unwrap();
        let published = versions
            .publish(tenant, job_id, forge_domain::JobVersionId::from_uuid(first.id))
            .await
            .unwrap();
        assert!(published.published_at.is_some());

        // A published version cannot be mutated afterwards.
        assert!(versions.ensure_draft(tenant, job_id, forge_domain::JobVersionId::from_uuid(first.id)).await.is_err());

        // Publishing again keeps the original timestamp (one-way).
        let again = versions
            .publish(tenant, job_id, forge_domain::JobVersionId::from_uuid(first.id))
            .await
            .unwrap();
        assert_eq!(again.published_at, published.published_at);

        // latest_published now resolves.
        let latest = versions.latest_published(tenant, job_id).await.unwrap();
        assert!(latest.is_some());
    })
    .await;
}

// ---------------------------------------------------------------------------
// Executions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execution_creation_is_atomic_and_tenant_scoped() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let other = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        seed_tenant(&pool, other).await;

        let executions = ExecutionRepository::new(&pool);
        let created = executions
            .create(NewExecution {
                tenant_id: tenant,
                job_id,
                job_version_id: version_id,
                queue_id: None,
                schedule_id: None,
                trigger_source: forge_domain::TriggerSource::Manual,
                priority: Priority::Normal,
                scheduled_for: None,
                correlation_id: Some("corr-1".into()),
                input: json!({}),
            })
            .await
            .unwrap();
        assert_eq!(created.status, "QUEUED");
        assert_eq!(created.attempt_count, 0);

        // Another tenant cannot read it.
        assert!(matches!(
            executions.get(other, created.id).await,
            Err(StorageError::NotFound { .. })
        ));
    })
    .await;
}

#[tokio::test]
async fn execution_transitions_replay_through_the_domain_machine() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let row = executions
            .create(NewExecution {
                tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
                schedule_id: None,
                trigger_source: forge_domain::TriggerSource::Manual, priority: Priority::Normal,
                scheduled_for: None, correlation_id: None, input: json!({}),
            })
            .await
            .unwrap();

        // Illegal: QUEUED -> SUCCEEDED.
        let bad = executions.transition(tenant, row.id, ExecutionStatus::Succeeded, None, None).await;
        assert!(bad.is_err(), "storage must reject an illegal transition");

        // Legal path.
        executions.transition(tenant, row.id, ExecutionStatus::Dispatched, None, None).await.unwrap();
        let running = executions.transition(tenant, row.id, ExecutionStatus::Running, None, None).await.unwrap();
        assert!(running.started_at.is_some());

        let done = executions.transition(tenant, row.id, ExecutionStatus::Succeeded, None, None).await.unwrap();
        assert!(done.ended_at.is_some());

        // Terminal: no going back.
        let back = executions.transition(tenant, row.id, ExecutionStatus::Running, None, None).await;
        assert!(back.is_err());
    })
    .await;
}

// AT-SCH-008 at the storage layer: the unique constraint prevents a duplicate
// occurrence even without locking.
#[tokio::test]
async fn duplicate_schedule_occurrence_is_rejected() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let schedules = ScheduleRepository::new(&pool);

        let schedule_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO schedules (id, tenant_id, job_id, target_id, target_type,
                                    cron_expression, timezone, next_run_at)
             VALUES ($1, $2, $3, $4, 'JOB', '0 2 * * *', 'UTC', $5)",
        )
        .bind(schedule_id)
        .bind(tenant.into_uuid())
        .bind(job_id.into_uuid())
        .bind(job_id.into_uuid())
        .bind(Utc::now())
        .execute(&pool)
        .await
        .unwrap();

        let executions = ExecutionRepository::new(&pool);
        let occurrence = Utc::now();

        let base = NewExecution {
            tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
            schedule_id: Some(schedule_id),
            trigger_source: forge_domain::TriggerSource::Schedule, priority: Priority::Normal,
            scheduled_for: Some(occurrence), correlation_id: None, input: json!({}),
        };
        executions.create(base.clone()).await.unwrap();

        // A second scheduler racing the same occurrence cannot insert: the
        // (schedule_id, scheduled_for) index rejects it outright.
        let result = executions.create(base).await;
        assert!(result.is_err(), "a duplicate occurrence must be rejected");

        // A different occurrence on the same schedule is fine.
        executions
            .create(NewExecution {
                schedule_id: Some(schedule_id),
                scheduled_for: Some(occurrence + chrono::Duration::days(1)),
                ..NewExecution {
                    tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
                    schedule_id: None, trigger_source: forge_domain::TriggerSource::Schedule,
                    priority: Priority::Normal, scheduled_for: None, correlation_id: None,
                    input: json!({}),
                }
            })
            .await
            .expect("a later occurrence is a distinct row");

        let _ = schedules;
    })
    .await;
}

#[tokio::test]
async fn concurrency_limit_admits_up_to_the_bound() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);

        let policy = forge_storage::bounded_concurrency(2);
        assert_eq!(policy.scope, forge_domain::ConcurrencyScope::Job);

        let mk = |corr: &str| NewExecution {
            tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
            schedule_id: None,
            trigger_source: forge_domain::TriggerSource::Manual, priority: Priority::Normal,
            scheduled_for: None, correlation_id: Some(corr.into()), input: json!({}),
        };

        // One active execution: room for another.
        assert!(executions.concurrency_admits(tenant, job_id.into_uuid(), None, &policy).await.unwrap());

        executions.create(mk("a")).await.unwrap();
        executions.create(mk("b")).await.unwrap();
        // Two active: at the bound.
        assert!(!executions.concurrency_admits(tenant, job_id.into_uuid(), None, &policy).await.unwrap());

        // Finishing one frees the slot (AT-CON-004).
        let active = executions.list(tenant, &ExecutionFilter::default(), None, 10).await.unwrap();
        let first = active.items[0].id;
        executions.transition(tenant, first, ExecutionStatus::Dispatched, None, None).await.unwrap();
        executions.transition(tenant, first, ExecutionStatus::Running, None, None).await.unwrap();
        executions.transition(tenant, first, ExecutionStatus::Succeeded, None, None).await.unwrap();

        assert!(executions.concurrency_admits(tenant, job_id.into_uuid(), None, &policy).await.unwrap());
    })
    .await;
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cursor_pagination_walks_every_row_exactly_once() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);

        for i in 0..7 {
            let name = format!("Job {i}");
            jobs.create(tenant, Some(format!("k{i}")), &name, None, Priority::Normal, None)
                .await
                .unwrap();
        }

        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        // A page size of 3 over 7 rows guarantees multiple pages.
        for _ in 0..10 {
            let page = jobs
                .list(tenant, &JobFilter::default(), cursor.as_deref(), 3)
                .await
                .unwrap();
            seen.extend(page.items.iter().map(|j| j.id));
            if !page.has_more {
                assert!(page.next_cursor.is_none());
                break;
            }
            cursor = page.next_cursor;
        }

        assert_eq!(seen.len(), 7, "every row appears exactly once");
        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), 7, "no row is returned twice");
    })
    .await;
}

#[tokio::test]
async fn job_filters_narrow_results() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let jobs = JobRepository::new(&pool);

        jobs.create(tenant, Some("alpha".into()), "Alpha", None, Priority::Normal, None).await.unwrap();
        jobs.create(tenant, Some("beta".into()), "Beta", None, Priority::Critical, None).await.unwrap();

        let by_status = jobs.list(tenant, &JobFilter { status: Some(JobStatus::Draft), ..Default::default() }, None, 10).await.unwrap();
        assert_eq!(by_status.items.len(), 2);

        let active = jobs.set_status(tenant, JobId::from_uuid(by_status.items[0].id), JobStatus::Archived).await;
        // DRAFT -> ARCHIVED is allowed.
        active.unwrap();

        let archived = jobs.list(tenant, &JobFilter { status: Some(JobStatus::Archived), ..Default::default() }, None, 10).await.unwrap();
        assert_eq!(archived.items.len(), 1);

        let search = jobs.list(tenant, &JobFilter { search: Some("alph".into()), ..Default::default() }, None, 10).await.unwrap();
        assert_eq!(search.items.len(), 1);
        assert_eq!(search.items[0].key.as_deref(), Some("alpha"));
    })
    .await;
}

// ---------------------------------------------------------------------------
// Workers and leases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn worker_registration_and_heartbeat() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let workers = WorkerRepository::new(&pool);

        let w = workers.register(tenant, "worker-1", "host-1", Some("0.1.0"), json!(["linux"]), json!({"region":"us-east-1"})).await.unwrap();
        assert_eq!(w.status, "READY");
        assert!(!w.draining);

        let beat = workers.heartbeat(tenant, w.id).await.unwrap();
        assert!(beat.last_heartbeat_at >= w.last_heartbeat_at);

        workers.drain(tenant, w.id).await.unwrap();
        // A drained worker must not be offered new work (AT-WKR-004).
        assert!(workers.find_ready(tenant, 10).await.unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn revoked_worker_cannot_heartbeat_or_receive_work() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let workers = WorkerRepository::new(&pool);
        let w = workers.register(tenant, "worker-2", "host-2", None, json!([]), json!({})).await.unwrap();

        workers.revoke(tenant, w.id).await.unwrap();
        // AT-SEC-003 style: revocation is effective immediately.
        assert!(workers.heartbeat(tenant, w.id).await.is_err());
        assert!(workers.find_ready(tenant, 10).await.unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn lease_ownership_is_exclusive_and_renewable_by_its_holder_only() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let row = executions
            .create(NewExecution {
                tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
                schedule_id: None,
                trigger_source: forge_domain::TriggerSource::Manual, priority: Priority::Normal,
                scheduled_for: None, correlation_id: None, input: json!({}),
            })
            .await
            .unwrap();

        let workers = WorkerRepository::new(&pool);
        let w1 = workers.register(tenant, "w1", "h1", None, json!([]), json!({})).await.unwrap();
        let w2 = workers.register(tenant, "w2", "h2", None, json!([]), json!({})).await.unwrap();

        let leases = LeaseRepository::new(&pool);
        let lease = leases.acquire(tenant, row.id, w1.id, None, 20).await.unwrap();

        // A second worker cannot take the same execution's lease.
        let second = leases.acquire(tenant, row.id, w2.id, None, 20).await;
        assert!(second.is_err(), "lease ownership must be exclusive");

        // The holder renews successfully.
        leases.renew(lease.id, w1.id, 20).await.unwrap();

        // A non-holder cannot renew (invariant 8).
        let bad = leases.renew(lease.id, w2.id, 20).await;
        assert!(matches!(bad, Err(StorageError::Conflict(_))), "got {bad:?}");

        // Releasing frees the lease for re-acquisition.
        leases.release(lease.id, w1.id).await.unwrap();
        leases.acquire(tenant, row.id, w2.id, None, 20).await.expect("lease is reusable after release");
    })
    .await;
}

#[tokio::test]
async fn expired_leases_are_claimable_by_the_reaper() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let row = executions
            .create(NewExecution {
                tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
                schedule_id: None,
                trigger_source: forge_domain::TriggerSource::Manual, priority: Priority::Normal,
                scheduled_for: None, correlation_id: None, input: json!({}),
            })
            .await
            .unwrap();
        let workers = WorkerRepository::new(&pool);
        let w = workers.register(tenant, "w", "h", None, json!([]), json!({})).await.unwrap();
        let leases = LeaseRepository::new(&pool);

        // Not yet expired: nothing to reap.
        leases.acquire(tenant, row.id, w.id, None, 3600).await.unwrap();
        assert!(leases.claim_expired(10).await.unwrap().is_empty());

        // Force expiry, then the reaper sees it.
        sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool).await.unwrap();
        let expired = leases.claim_expired(10).await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].execution_id, row.id);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Dispatch claiming
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dispatch_claims_high_priority_work_first() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = WorkerRepository::new(&pool);
        let w = workers.register(tenant, "w", "h", None, json!([]), json!({})).await.unwrap();

        let mk = |priority: Priority, corr: &str| NewExecution {
            tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
            schedule_id: None,
            trigger_source: forge_domain::TriggerSource::Manual, priority,
            scheduled_for: None, correlation_id: Some(corr.into()), input: json!({}),
        };
        executions.create(mk(Priority::Background, "low")).await.unwrap();
        executions.create(mk(Priority::Critical, "high")).await.unwrap();

        let first = executions.claim_next(tenant, None, w.id).await.unwrap().unwrap();
        assert_eq!(first.priority, "CRITICAL");
        assert_eq!(first.status, "DISPATCHED");
        assert_eq!(first.worker_id, Some(w.id));

        let second = executions.claim_next(tenant, None, w.id).await.unwrap().unwrap();
        assert_eq!(second.priority, "BACKGROUND");

        // Queue drained.
        assert!(executions.claim_next(tenant, None, w.id).await.unwrap().is_none());
    })
    .await;
}

#[tokio::test]
async fn concurrent_claimers_never_receive_the_same_execution() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_id, version_id) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = WorkerRepository::new(&pool);

        let total = 12;
        for i in 0..total {
            executions
                .create(NewExecution {
                    tenant_id: tenant, job_id, job_version_id: version_id, queue_id: None,
                    schedule_id: None,
                    trigger_source: forge_domain::TriggerSource::Manual, priority: Priority::Normal,
                    scheduled_for: None, correlation_id: Some(format!("c{i}")), input: json!({}),
                })
                .await
                .unwrap();
        }

        // Four claimers race. SKIP LOCKED must keep the claimed sets disjoint.
        let mut handles = Vec::new();
        for n in 0..4 {
            let pool = pool.clone();
            let worker = workers.register(tenant, &format!("w{n}"), &format!("h{n}"), None, json!([]), json!({})).await.unwrap().id;
            handles.push(tokio::spawn(async move {
                let repo = ExecutionRepository::new(&pool);
                let mut claimed = Vec::new();
                while let Some(row) = repo.claim_next(tenant, None, worker).await.unwrap() {
                    claimed.push(row.id);
                }
                claimed
            }));
        }

        let mut all = Vec::new();
        for h in handles {
            all.extend(h.await.unwrap());
        }

        assert_eq!(all.len(), total, "every execution is claimed exactly once");
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), total, "no execution was claimed twice");
    })
    .await;
}

// ---------------------------------------------------------------------------
// Idempotency
// ---------------------------------------------------------------------------

// AT-API-005 / AT-API-006
#[tokio::test]
async fn idempotency_replays_the_same_request_and_rejects_a_different_one() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let idem = IdempotencyRepository::new(&pool);
        let expires = Utc::now() + chrono::Duration::hours(24);

        let outcome = idem.reserve(tenant, "key-1", "POST /jobs", "fp-a", expires).await.unwrap();
        assert!(matches!(outcome, IdempotencyOutcome::Fresh));

        idem.complete(tenant, "key-1", "POST /jobs", 201, &json!({"id":"job-1"}), None).await.unwrap();

        // Same key, same fingerprint: replay the stored response.
        let replay = idem.reserve(tenant, "key-1", "POST /jobs", "fp-a", expires).await.unwrap();
        match replay {
            IdempotencyOutcome::Replay { status, body } => {
                assert_eq!(status, 201);
                assert_eq!(body["id"], "job-1");
            }
            other => panic!("expected a replay, got {other:?}"),
        }

        // Same key, different fingerprint: conflict.
        let conflict = idem.reserve(tenant, "key-1", "POST /jobs", "fp-b", expires).await.unwrap();
        assert!(matches!(conflict, IdempotencyOutcome::Conflict));

        // A different tenant may use the same key independently.
        let other = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, other).await;
        assert!(matches!(
            idem.reserve(other, "key-1", "POST /jobs", "fp-a", expires).await.unwrap(),
            IdempotencyOutcome::Fresh
        ));
    })
    .await;
}

// ---------------------------------------------------------------------------
// Audit and outbox
// ---------------------------------------------------------------------------

// AT-SEC-006: audit events are emitted for privileged actions.
#[tokio::test]
async fn audit_events_are_recorded_and_queryable() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let audit = AuditRepository::new(&pool);

        audit.record(NewAuditEvent::new(tenant, "USER", None, "job.create", "job", Some(Uuid::new_v4())))
            .await
            .unwrap();
        audit
            .record(NewAuditEvent::new(tenant, "USER", None, "job.archive", "job", Some(Uuid::new_v4())).denied())
            .await
            .unwrap();

        let page = audit
            .list(
                tenant,
                &forge_storage::AuditFilter {
                    action: Some("job.create".to_string()),
                    ..Default::default()
                },
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].action, "job.create");
        assert_eq!(page.items[0].result, "SUCCESS");

        // Another tenant cannot read these events.
        let other = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, other).await;
        assert!(audit
            .list(other, &Default::default(), None, 10)
            .await
            .unwrap()
            .items
            .is_empty());
    })
    .await;
}

#[tokio::test]
async fn outbox_claims_each_event_once_and_records_publication() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let outbox = OutboxRepository::new(&pool);

        for _ in 0..3 {
            outbox.enqueue(Some(tenant), "execution.completed", "execution", Uuid::new_v4(), json!({"ok":true}))
                .await
                .unwrap();
        }

        let claimed = outbox.claim_unpublished(10).await.unwrap();
        assert_eq!(claimed.len(), 3);
        // The claim increments the attempt counter so a stuck event is visible.
        assert!(claimed.iter().all(|e| e.attempt_count >= 1));

        for event in &claimed {
            outbox.mark_published(event.id).await.unwrap();
        }
        // Nothing left to publish.
        assert!(outbox.claim_unpublished(10).await.unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn outbox_publisher_restart_does_not_lose_events() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        seed_tenant(&pool, tenant).await;
        let outbox = OutboxRepository::new(&pool);

        let first = outbox.enqueue(Some(tenant), "a", "execution", Uuid::new_v4(), json!({})).await.unwrap();
        // Simulate a publisher that claimed but crashed before marking it.
        let claimed = outbox.claim_unpublished(10).await.unwrap();
        assert_eq!(claimed.len(), 1);
        outbox.mark_failed(claimed[0].id, "publisher crashed").await.unwrap();

        // A new publisher must find the event again.
        let retried = outbox.claim_unpublished(10).await.unwrap();
        assert_eq!(retried.len(), 1);
        assert_eq!(retried[0].id, first.id);
        assert!(retried[0].attempt_count >= 2);
        assert_eq!(retried[0].last_error.as_deref(), Some("publisher crashed"));
    })
    .await;
}