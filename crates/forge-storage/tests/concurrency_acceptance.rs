//! Acceptance tests for the concurrency limits (spec 01.12, spec 02, AT-CON).
//!
//! Each test names the acceptance ID it satisfies, so a search for the ID finds
//! both the specification and its implementation.

use std::sync::Arc;

use chrono::Utc;
use forge_domain::{
    ConcurrencyScope, ExecutionStatus, JobId, JobVersionId, TenantId, TriggerSource,
};
use forge_storage::{
    ExecutionRepository, JobRepository, JobVersionRepository, NewExecution, NewJobVersion,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    async fn cleanup(self) {
        self.pool.close().await;
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                         WHERE datname = '{}' AND pid <> pg_backend_pid()",
                    self.db_name
                )))
                .await;
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    r#"DROP DATABASE IF EXISTS "{}""#,
                    self.db_name
                )))
                .await;
            admin.close().await;
        }
    }

    async fn new() -> Option<TestDb> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let db_name = format!("forge_con_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping concurrency tests: cannot connect ({e})");
                return None;
            }
        };
        if admin
            .execute(sqlx::AssertSqlSafe(format!(
                r#"CREATE DATABASE "{db_name}""#
            )))
            .await
            .is_err()
        {
            eprintln!("skipping concurrency tests: cannot create database");
            return None;
        }
        admin.close().await;

        let pool = match PgPoolOptions::new()
            .max_connections(10)
            .connect(&format!("{server}/{db_name}"))
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping concurrency tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
            eprintln!("skipping concurrency tests: migrations failed ({e})");
            return None;
        }
        Some(TestDb {
            pool,
            admin_url,
            db_name,
        })
    }
}

macro_rules! with_db {
    ($body:expr) => {
        async move {
            match TestDb::new().await {
                Some(db) => {
                    let db = Arc::new(db);
                    // Spawned so the body's panic arrives as a value rather than an
                    // unwind: cleanup below has to run on the failure path too, or a
                    // broken test leaves its database behind forever. Awaiting a
                    // JoinHandle returns Err(JoinError) instead of propagating, so the
                    // panic is resumed *after* cleanup and a real failure still fails.
                    let outcome =
                        tokio::spawn(std::panic::AssertUnwindSafe($body(db.pool.clone()))).await;
                    match Arc::try_unwrap(db) {
                        Ok(db) => db.cleanup().await,
                        Err(_) => eprintln!("warning: test db handle still shared"),
                    }
                    if let Err(join_err) = outcome {
                        if join_err.is_panic() {
                            std::panic::resume_unwind(join_err.into_panic());
                        }
                        eprintln!("test body did not complete: {join_err}");
                    }
                }
                None => eprintln!("(skipped: no database available)"),
            }
        }
    };
}

/// A tenant with a published job version.
async fn scaffold(pool: &PgPool, tenant: TenantId) -> (JobId, JobVersionId) {
    // A tenant may already exist when a test builds several jobs in it.
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T') ON CONFLICT DO NOTHING")
        .bind(tenant.into_uuid())
        .execute(pool)
        .await
        .unwrap();

    let job = JobRepository::new(pool)
        .create(
            tenant,
            Some(format!("j{}", Uuid::new_v4().simple())),
            "Job",
            None,
            forge_domain::Priority::Normal,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let version = JobVersionRepository::new(pool)
        .create(tenant, JobId::from_uuid(job.id), &NewJobVersion::default())
        .await
        .unwrap();
    JobVersionRepository::new(pool)
        .publish(
            tenant,
            JobId::from_uuid(job.id),
            JobVersionId::from_uuid(version.id),
        )
        .await
        .unwrap();

    (
        JobId::from_uuid(job.id),
        JobVersionId::from_uuid(version.id),
    )
}

async fn queued(
    pool: &PgPool,
    tenant: TenantId,
    job: JobId,
    version: JobVersionId,
    queue: Option<Uuid>,
) -> Uuid {
    ExecutionRepository::new(pool)
        .create(NewExecution {
            tenant_id: tenant,
            job_id: job,
            job_version_id: version,
            queue_id: queue,
            schedule_id: None,
            trigger_source: TriggerSource::Manual,
            priority: forge_domain::Priority::Normal,
            scheduled_for: None,
            correlation_id: Some(Uuid::new_v4().to_string()),
            input: json!({}),
        })
        .await
        .unwrap()
        .id
}

fn policy(limit: u32, scope: ConcurrencyScope) -> forge_domain::ConcurrencyPolicy {
    forge_domain::ConcurrencyPolicy {
        max_concurrent_executions: forge_domain::ConcurrencyLimit::Bounded(limit),
        scope,
        queue_id: None,
    }
}

// AT-CON-001: a job's concurrency limit is enforced.
#[tokio::test]
async fn at_con_001_job_concurrency_limit_is_enforced() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job, version) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let limit = policy(2, ConcurrencyScope::Job);

        // Zero active: admitted.
        assert!(executions
            .concurrency_admits(tenant, job.into_uuid(), None, &limit)
            .await
            .unwrap());

        queued(&pool, tenant, job, version, None).await;
        assert!(executions
            .concurrency_admits(tenant, job.into_uuid(), None, &limit)
            .await
            .unwrap());

        queued(&pool, tenant, job, version, None).await;
        // At the bound: refused.
        assert!(
            !executions
                .concurrency_admits(tenant, job.into_uuid(), None, &limit)
                .await
                .unwrap(),
            "a third concurrent execution must be refused"
        );
    })
    .await;
}

// AT-CON-001: the limit applies per job, not globally.
#[tokio::test]
async fn at_con_001_limit_is_scoped_to_one_job() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_a, version_a) = scaffold(&pool, tenant).await;

        // A second, unrelated job.
        let job_b = JobRepository::new(&pool)
            .create(
                tenant,
                Some(format!("other-{}", Uuid::new_v4().simple())),
                "Other",
                None,
                forge_domain::Priority::Normal,
                None,
            None,
            None,
            )
            .await
            .unwrap();
        let version_b = JobVersionRepository::new(&pool)
            .create(
                tenant,
                JobId::from_uuid(job_b.id),
                &NewJobVersion::default(),
            )
            .await
            .unwrap();

        let executions = ExecutionRepository::new(&pool);
        let limit = policy(1, ConcurrencyScope::Job);

        // Saturate job A.
        queued(&pool, tenant, job_a, version_a, None).await;
        assert!(!executions
            .concurrency_admits(tenant, job_a.into_uuid(), None, &limit)
            .await
            .unwrap());

        // Job B is unaffected by job A's backlog.
        assert!(
            executions
                .concurrency_admits(tenant, job_b.id, None, &limit)
                .await
                .unwrap(),
            "one job's saturation must not block another"
        );
        let _ = version_b;
    })
    .await;
}

// AT-CON-002: a tenant's concurrency limit is enforced across its jobs.
#[tokio::test]
async fn at_con_002_tenant_concurrency_is_enforced() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job_a, version_a) = scaffold(&pool, tenant).await;
        let (job_b, version_b) = scaffold(&pool, tenant).await;

        let executions = ExecutionRepository::new(&pool);
        let limit = policy(2, ConcurrencyScope::Tenant);

        queued(&pool, tenant, job_a, version_a, None).await;
        queued(&pool, tenant, job_b, version_b, None).await;

        // Two active across two different jobs: the tenant limit still holds.
        assert!(
            !executions
                .concurrency_admits(tenant, job_a.into_uuid(), None, &limit)
                .await
                .unwrap(),
            "the tenant limit spans every job in the tenant"
        );
    })
    .await;
}

// AT-CON-002: another tenant's activity must not consume this one's budget.
#[tokio::test]
async fn at_con_002_tenant_limits_are_independent() {
    with_db!(|pool: PgPool| async move {
        let tenant_a = TenantId::from_uuid(Uuid::new_v4());
        let tenant_b = TenantId::from_uuid(Uuid::new_v4());
        let (job_a, version_a) = scaffold(&pool, tenant_a).await;
        let (job_b, version_b) = scaffold(&pool, tenant_b).await;

        let executions = ExecutionRepository::new(&pool);
        let limit = policy(1, ConcurrencyScope::Tenant);

        // Tenant A is saturated.
        queued(&pool, tenant_a, job_a, version_a, None).await;
        assert!(!executions
            .concurrency_admits(tenant_a, job_a.into_uuid(), None, &limit)
            .await
            .unwrap());

        // Tenant B is untouched.
        assert!(
            executions
                .concurrency_admits(tenant_b, job_b.into_uuid(), None, &limit)
                .await
                .unwrap(),
            "another tenant's usage must not consume this tenant's budget"
        );
        let _ = version_b;
    })
    .await;
}

// AT-CON-003: a queue's concurrency limit is enforced.
#[tokio::test]
async fn at_con_003_queue_concurrency_is_enforced() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job, version) = scaffold(&pool, tenant).await;

        let queue_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO queues (id, tenant_id, name, max_concurrency) VALUES ($1, $2, 'bulk', 2)",
        )
        .bind(queue_id)
        .bind(tenant.into_uuid())
        .execute(&pool)
        .await
        .unwrap();

        let executions = ExecutionRepository::new(&pool);
        let mut limit = policy(2, ConcurrencyScope::Queue);
        limit.queue_id = Some(queue_id.to_string());

        queued(&pool, tenant, job, version, Some(queue_id)).await;
        queued(&pool, tenant, job, version, Some(queue_id)).await;

        assert_eq!(
            executions.count_active_for_queue(queue_id).await.unwrap(),
            2
        );
        assert!(
            !executions
                .concurrency_admits(tenant, job.into_uuid(), Some(queue_id), &limit)
                .await
                .unwrap(),
            "a third execution must not join a saturated queue"
        );
    })
    .await;
}

// AT-CON-004: completing an execution releases its slot.
#[tokio::test]
async fn at_con_004_slot_is_released_after_completion() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job, version) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let limit = policy(1, ConcurrencyScope::Job);

        let id = queued(&pool, tenant, job, version, None).await;
        assert!(!executions
            .concurrency_admits(tenant, job.into_uuid(), None, &limit)
            .await
            .unwrap());

        // Run it to completion.
        executions
            .transition(tenant, id, ExecutionStatus::Dispatched, None, None)
            .await
            .unwrap();
        executions
            .transition(tenant, id, ExecutionStatus::Running, None, None)
            .await
            .unwrap();
        executions
            .transition(tenant, id, ExecutionStatus::Succeeded, None, None)
            .await
            .unwrap();

        assert_eq!(
            executions
                .count_active_for_job(tenant, job.into_uuid())
                .await
                .unwrap(),
            0,
            "a terminal execution must not hold a slot"
        );
        assert!(executions
            .concurrency_admits(tenant, job.into_uuid(), None, &limit)
            .await
            .unwrap());
    })
    .await;
}

/// Every terminal state must release the slot, not just success.
#[tokio::test]
async fn at_con_004_every_terminal_state_releases_the_slot() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job, version) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);

        // One execution per terminal outcome.
        for (terminal, via) in [
            (ExecutionStatus::Succeeded, vec![ExecutionStatus::Running]),
            (ExecutionStatus::Failed, vec![ExecutionStatus::Running]),
            (
                ExecutionStatus::Cancelled,
                vec![ExecutionStatus::Running, ExecutionStatus::CancelRequested],
            ),
            // Dead-lettering is reached from FAILED, not from RUNNING.
            (
                ExecutionStatus::DeadLettered,
                vec![ExecutionStatus::Running, ExecutionStatus::Failed],
            ),
            // A timeout also ends the execution.
            (ExecutionStatus::TimedOut, vec![ExecutionStatus::Running]),
        ] {
            let id = queued(&pool, tenant, job, version, None).await;
            executions
                .transition(tenant, id, ExecutionStatus::Dispatched, None, None)
                .await
                .unwrap();
            for step in via {
                executions
                    .transition(tenant, id, step, None, None)
                    .await
                    .unwrap();
            }
            executions
                .transition(tenant, id, terminal, None, None)
                .await
                .unwrap();
            let status = terminal;

            let after = executions.get(tenant, id).await.unwrap();
            assert_eq!(after.status, status.as_str());
            assert!(
                !after
                    .status
                    .parse::<ExecutionStatus>()
                    .unwrap()
                    .holds_concurrency_slot(),
                "{status} must not hold a concurrency slot"
            );
        }
    })
    .await;
}

// AT-CON-005: a slot held by an abandoned attempt is recovered.
#[tokio::test]
async fn at_con_005_slot_is_recovered_after_an_abandoned_attempt() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job, version) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = forge_storage::WorkerRepository::new(&pool);
        let leases = forge_storage::LeaseRepository::new(&pool);

        let worker = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        let id = queued(&pool, tenant, job, version, None).await;
        executions
            .claim_next(tenant, None, worker.id)
            .await
            .unwrap();
        leases
            .acquire(tenant, id, worker.id, None, 20)
            .await
            .unwrap();

        // The worker dies; recovery runs.
        sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        let stats = forge_executor::LeaseReaper::new(&pool)
            .reap(Utc::now(), 100)
            .await;
        assert_eq!(stats.requeued, 1);

        // The attempt is counted, so a crash-loop cannot retry forever.
        let after = executions.get(tenant, id).await.unwrap();
        assert_eq!(after.status, "QUEUED");
        assert_eq!(
            after.attempt_count, 1,
            "recovery must count as another attempt"
        );
        assert_eq!(
            executions
                .count_active_for_job(tenant, job.into_uuid())
                .await
                .unwrap(),
            1,
            "the recovered execution still holds exactly one slot"
        );
    })
    .await;
}

/// An unlimited policy admits work regardless of the current count.
#[tokio::test]
async fn an_unlimited_policy_always_admits() {
    with_db!(|pool: PgPool| async move {
        let tenant = TenantId::from_uuid(Uuid::new_v4());
        let (job, version) = scaffold(&pool, tenant).await;
        let executions = ExecutionRepository::new(&pool);

        for _ in 0..5 {
            queued(&pool, tenant, job, version, None).await;
        }

        let unlimited = forge_domain::ConcurrencyPolicy::default();
        assert!(executions
            .concurrency_admits(tenant, job.into_uuid(), None, &unlimited)
            .await
            .unwrap());
    })
    .await;
}
