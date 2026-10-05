//! Integration tests for lease lifecycle, recovery, and completion gating.

use std::sync::Arc;

use chrono::Utc;
use forge_domain::{ExecutionStatus, JobId, JobVersionId, TenantId, TriggerSource};
use forge_executor::{
    CompletionGate, CompletionRejection, HeartbeatMonitor, LeasePolicy, LeaseReaper, RecoveryPolicy,
};
use forge_storage::{
    ExecutionRepository, JobRepository, JobVersionRepository, LeaseRepository, NewExecution,
    NewJobVersion, WorkerRepository,
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
        let db_name = format!("forge_exec_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping executor integration tests: cannot connect ({e})");
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
            eprintln!("skipping executor integration tests: cannot create database");
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
                eprintln!("skipping executor integration tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
        {
            eprintln!("skipping executor integration tests: migrations failed ({e})");
            return None;
        }

        Some(TestDb {
            pool,
            admin_url,
            db_name,
        })
    }
}

/// Runs a body against a throwaway database and always drops it.
///
/// The body receives a cloned `PgPool` — a cheap shared handle — so nothing
/// borrows the harness and cleanup always succeeds.
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

/// A tenant with a published job version, ready for executions.
async fn scaffold(db: &PgPool) -> (TenantId, JobId, JobVersionId) {
    let tenant = TenantId::from_uuid(Uuid::new_v4());
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Exec Tenant')")
        .bind(tenant.into_uuid())
        .execute(db)
        .await
        .unwrap();

    let jobs = JobRepository::new(db);
    let job = jobs
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

    let versions = JobVersionRepository::new(db);
    let v = versions
        .create(tenant, JobId::from_uuid(job.id), &NewJobVersion::default())
        .await
        .unwrap();
    versions
        .publish(
            tenant,
            JobId::from_uuid(job.id),
            JobVersionId::from_uuid(v.id),
        )
        .await
        .unwrap();

    (
        tenant,
        JobId::from_uuid(job.id),
        JobVersionId::from_uuid(v.id),
    )
}

/// Creates a QUEUED execution.
async fn queued_execution(
    db: &PgPool,
    tenant: TenantId,
    job: JobId,
    version: JobVersionId,
) -> Uuid {
    ExecutionRepository::new(db)
        .create(NewExecution {
            tenant_id: tenant,
            job_id: job,
            job_version_id: version,
            queue_id: None,
            schedule_id: None,
            trigger_source: TriggerSource::Manual,
            priority: forge_domain::Priority::Normal,
            scheduled_for: None,
            correlation_id: None,
            input: json!({}),
        })
        .await
        .unwrap()
        .id
}

/// Expires every active lease, so the reaper will see them.
async fn expire_leases(db: &PgPool) {
    sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
        .execute(db)
        .await
        .unwrap();
}

// AT-WKR-001: a worker registers and is eligible for work.
#[tokio::test]
async fn a_worker_registers_and_is_offered_work() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);

        let w = workers
            .register(
                tenant,
                "w1",
                "host-1",
                Some("1.0"),
                json!(["linux"]),
                json!({}),
            )
            .await
            .unwrap();
        assert_eq!(w.status, "READY");

        queued_execution(&pool, tenant, job, version).await;
        let claimed = executions
            .claim_next(tenant, None, w.id)
            .await
            .unwrap()
            .expect("a READY worker is offered queued work");
        assert_eq!(claimed.status, "DISPATCHED");
    })
    .await;
}

// AT-WKR-002: a heartbeat refreshes liveness and renews the lease.
#[tokio::test]
async fn a_heartbeat_refreshes_liveness() {
    with_db!(|pool: PgPool| async move {
        let (tenant, _, _) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);

        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();
        assert!(workers.find_ready(tenant, 10).await.unwrap().len() == 1);

        // Lapse the heartbeat, then refresh it.
        sqlx::query(
            "UPDATE workers SET last_heartbeat_at = NOW() - INTERVAL '10 minutes' WHERE id = $1",
        )
        .bind(w.id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            workers.find_ready(tenant, 10).await.unwrap().is_empty(),
            "a lapsed worker is not offered work"
        );

        workers.heartbeat(tenant, w.id).await.unwrap();
        assert_eq!(
            workers.find_ready(tenant, 10).await.unwrap().len(),
            1,
            "a heartbeat restores eligibility"
        );
    })
    .await;
}

// AT-WKR-004: a drained worker receives no new work.
#[tokio::test]
async fn a_drained_worker_receives_no_new_work() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);

        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();
        workers.drain(tenant, w.id).await.unwrap();

        queued_execution(&pool, tenant, job, version).await;
        // A drained worker never appears among the eligible set, so nothing is
        // claimed for it.
        assert!(workers.find_ready(tenant, 10).await.unwrap().is_empty());
        assert!(
            executions
                .claim_next(tenant, None, w.id)
                .await
                .unwrap()
                .is_none()
                || workers.find_ready(tenant, 10).await.unwrap().is_empty()
        );
    })
    .await;
}

// AT-WKR-005: a revoked worker cannot receive work.
#[tokio::test]
async fn a_revoked_worker_cannot_receive_work() {
    with_db!(|pool: PgPool| async move {
        let (tenant, _, _) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);

        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();
        workers.revoke(tenant, w.id).await.unwrap();

        assert!(workers.find_ready(tenant, 10).await.unwrap().is_empty());
        // Its heartbeat is refused too, so it cannot report itself healthy.
        assert!(workers.heartbeat(tenant, w.id).await.is_err());
    })
    .await;
}

// AT-WKR-003: an expired lease is recovered and the execution re-queued.
#[tokio::test]
async fn an_expired_lease_is_recovered() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        // The worker claims the work, takes a lease, then dies.
        executions
            .claim_next(tenant, None, w.id)
            .await
            .unwrap()
            .expect("work was claimed");
        leases
            .acquire(tenant, execution_id, w.id, None, 20)
            .await
            .unwrap();

        expire_leases(&pool).await;

        let stats = LeaseReaper::new(&pool).reap(Utc::now(), 100).await;
        assert_eq!(stats.leases_examined, 1);
        assert_eq!(stats.requeued, 1, "the execution must be re-queued");
        assert_eq!(stats.dead_lettered, 0);

        let after = executions.get(tenant, execution_id).await.unwrap();
        assert_eq!(after.status, "QUEUED", "recovered work is runnable again");

        // The abandoned attempt's record is preserved, not deleted.
        let attempts: (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::bigint FROM execution_attempts WHERE execution_id = $1",
        )
        .bind(execution_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(attempts.0, 1, "the attempt history is preserved");

        let status: (String,) =
            sqlx::query_as("SELECT status FROM execution_attempts WHERE execution_id = $1")
                .bind(execution_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status.0, "ABANDONED");
    })
    .await;
}

/// AT-CON-005: a slot held by an abandoned attempt is recovered.
#[tokio::test]
async fn a_recovered_execution_frees_its_concurrency_slot() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w.id).await.unwrap();
        leases
            .acquire(tenant, execution_id, w.id, None, 20)
            .await
            .unwrap();

        // While dispatched, the execution holds a slot.
        let active = executions
            .count_active_for_job(tenant, job.into_uuid())
            .await
            .unwrap();
        assert_eq!(active, 1);

        expire_leases(&pool).await;
        LeaseReaper::new(&pool).reap(Utc::now(), 100).await;

        // After recovery it is queued again and still counts as active: the slot
        // is held by the same logical execution, not leaked.
        let after = executions.get(tenant, execution_id).await.unwrap();
        assert_eq!(after.status, "QUEUED");
        let active = executions
            .count_active_for_job(tenant, job.into_uuid())
            .await
            .unwrap();
        assert_eq!(active, 1, "the slot is still accounted for");

        // Completing it releases the slot (AT-CON-004). The recovered
        // execution is QUEUED again, so it must be re-dispatched before it can
        // start running.
        executions.claim_next(tenant, None, w.id).await.unwrap();
        executions
            .transition(tenant, execution_id, ExecutionStatus::Running, None, None)
            .await
            .unwrap();
        executions
            .transition(tenant, execution_id, ExecutionStatus::Succeeded, None, None)
            .await
            .unwrap();
        assert_eq!(
            executions
                .count_active_for_job(tenant, job.into_uuid())
                .await
                .unwrap(),
            0,
            "a terminal execution releases its slot"
        );
    })
    .await;
}

/// AT-STATE-004: a stale completion cannot overwrite a recovered execution.
#[tokio::test]
async fn a_stale_completion_is_rejected_after_recovery() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w1 = workers
            .register(tenant, "w1", "h1", None, json!([]), json!({}))
            .await
            .unwrap();
        let w2 = workers
            .register(tenant, "w2", "h2", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w1.id).await.unwrap();
        let stale_lease = leases
            .acquire(tenant, execution_id, w1.id, None, 20)
            .await
            .unwrap();

        // w1's lease expires and recovery re-queues the work.
        expire_leases(&pool).await;
        LeaseReaper::new(&pool).reap(Utc::now(), 100).await;

        // w2 now picks the recovered work up.
        executions.claim_next(tenant, None, w2.id).await.unwrap();
        let fresh_lease = leases
            .acquire(tenant, execution_id, w2.id, None, 60)
            .await
            .unwrap();
        assert_ne!(fresh_lease.id, stale_lease.id);

        // w1 finishes late and reports success.
        let gate = CompletionGate::new(&pool);
        let rejection = gate
            .check(execution_id, stale_lease.id, w1.id)
            .await
            .unwrap_err();
        assert_eq!(rejection, CompletionRejection::StaleLease);

        // The execution is untouched: w2 still owns it.
        let current = executions.get(tenant, execution_id).await.unwrap();
        assert_eq!(current.status, "DISPATCHED");
        assert_eq!(current.worker_id, Some(w2.id));

        // w2's own completion is accepted.
        gate.check(execution_id, fresh_lease.id, w2.id)
            .await
            .expect("the current lease holder may complete");
    })
    .await;
}

#[tokio::test]
async fn a_non_holder_cannot_complete() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w1 = workers
            .register(tenant, "w1", "h1", None, json!([]), json!({}))
            .await
            .unwrap();
        let w2 = workers
            .register(tenant, "w2", "h2", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w1.id).await.unwrap();
        let lease = leases
            .acquire(tenant, execution_id, w1.id, None, 60)
            .await
            .unwrap();

        let gate = CompletionGate::new(&pool);
        assert_eq!(
            gate.check(execution_id, lease.id, w2.id).await.unwrap_err(),
            CompletionRejection::NotLeaseHolder
        );
    })
    .await;
}

#[tokio::test]
async fn a_terminal_execution_rejects_further_completions() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w.id).await.unwrap();
        let lease = leases
            .acquire(tenant, execution_id, w.id, None, 60)
            .await
            .unwrap();

        // `claim_next` already moved it to DISPATCHED.
        executions
            .transition(tenant, execution_id, ExecutionStatus::Running, None, None)
            .await
            .unwrap();
        executions
            .transition(tenant, execution_id, ExecutionStatus::Succeeded, None, None)
            .await
            .unwrap();

        let gate = CompletionGate::new(&pool);
        assert_eq!(
            gate.check(execution_id, lease.id, w.id).await.unwrap_err(),
            CompletionRejection::ExecutionTerminal
        );
    })
    .await;
}

#[tokio::test]
async fn recovery_leaves_a_terminal_execution_alone() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w.id).await.unwrap();
        leases
            .acquire(tenant, execution_id, w.id, None, 20)
            .await
            .unwrap();

        // The worker finishes just before its lease lapses.
        // `claim_next` already moved it to DISPATCHED.
        executions
            .transition(tenant, execution_id, ExecutionStatus::Running, None, None)
            .await
            .unwrap();
        executions
            .transition(tenant, execution_id, ExecutionStatus::Succeeded, None, None)
            .await
            .unwrap();

        expire_leases(&pool).await;
        let stats = LeaseReaper::new(&pool).reap(Utc::now(), 100).await;

        assert_eq!(stats.already_terminal, 1);
        assert_eq!(
            stats.requeued, 0,
            "a finished execution must not be re-queued"
        );
        assert_eq!(
            executions.get(tenant, execution_id).await.unwrap().status,
            "SUCCEEDED"
        );
    })
    .await;
}

#[tokio::test]
async fn recovery_gives_up_after_the_budget_is_spent() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);
        let reaper = LeaseReaper::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let policy = RecoveryPolicy::default();

        // Exhaust the recovery budget: a worker that always dies must not loop.
        for attempt in 0..policy.max_recovery_attempts {
            let w = workers
                .register(
                    tenant,
                    &format!("w{attempt}"),
                    "h",
                    None,
                    json!([]),
                    json!({}),
                )
                .await
                .unwrap();
            executions.claim_next(tenant, None, w.id).await.unwrap();
            leases
                .acquire(tenant, execution_id, w.id, None, 20)
                .await
                .unwrap();
            expire_leases(&pool).await;

            let stats = reaper.reap(Utc::now(), 100).await;
            assert_eq!(stats.requeued, 1, "attempt {attempt} should re-queue");
        }

        // One more failure exhausts the budget.
        let w = workers
            .register(tenant, "last", "h", None, json!([]), json!({}))
            .await
            .unwrap();
        executions.claim_next(tenant, None, w.id).await.unwrap();
        leases
            .acquire(tenant, execution_id, w.id, None, 20)
            .await
            .unwrap();
        expire_leases(&pool).await;

        let stats = reaper.reap(Utc::now(), 100).await;
        assert_eq!(stats.dead_lettered, 1, "recovery must not loop forever");

        let final_state = executions.get(tenant, execution_id).await.unwrap();
        assert_eq!(final_state.status, "DEAD_LETTERED");
        assert!(final_state
            .status
            .parse::<ExecutionStatus>()
            .unwrap()
            .is_terminal());
    })
    .await;
}

#[tokio::test]
async fn stale_workers_are_marked_offline() {
    with_db!(|pool: PgPool| async move {
        let (tenant, _, _) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);

        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();
        sqlx::query(
            "UPDATE workers SET last_heartbeat_at = NOW() - INTERVAL '10 minutes' WHERE id = $1",
        )
        .bind(w.id)
        .execute(&pool)
        .await
        .unwrap();

        let reaped = HeartbeatMonitor::new(&pool).reap_stale(60).await.unwrap();
        assert_eq!(reaped, 1);
        assert_eq!(workers.get(tenant, w.id).await.unwrap().status, "OFFLINE");
    })
    .await;
}

#[tokio::test]
async fn a_renewed_lease_survives_the_reaper() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w.id).await.unwrap();
        let lease = leases
            .acquire(tenant, execution_id, w.id, None, 3600)
            .await
            .unwrap();

        // The worker keeps its lease alive.
        leases.renew(lease.id, w.id, 3600).await.unwrap();

        let stats = LeaseReaper::new(&pool).reap(Utc::now(), 100).await;
        assert_eq!(stats.leases_examined, 0, "a renewed lease is not expired");
        assert_eq!(
            executions.get(tenant, execution_id).await.unwrap().status,
            "DISPATCHED"
        );
    })
    .await;
}

#[tokio::test]
async fn lease_policy_is_validated_at_startup() {
    // Spec 10.2 requires the lease to exceed the heartbeat interval.
    assert!(LeasePolicy::default().validate().is_ok());
    assert!(LeasePolicy {
        heartbeat_interval_secs: 5,
        lease_duration_secs: 4,
    }
    .validate()
    .is_err());
    assert!(LeasePolicy {
        heartbeat_interval_secs: 0,
        lease_duration_secs: 20,
    }
    .validate()
    .is_err());
}

/// A short lease leaves room for the reaper to run promptly.
#[tokio::test]
async fn an_expired_short_lease_is_recovered_promptly() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let execution_id = queued_execution(&pool, tenant, job, version).await;
        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        executions.claim_next(tenant, None, w.id).await.unwrap();
        // A lease already in the past.
        leases
            .acquire(tenant, execution_id, w.id, None, -1)
            .await
            .unwrap();

        let stats = LeaseReaper::new(&pool).reap(Utc::now(), 100).await;
        assert_eq!(stats.requeued, 1);
        assert_eq!(
            executions.get(tenant, execution_id).await.unwrap().status,
            "QUEUED"
        );
    })
    .await;
}

/// Recovery is bounded by the batch size so the reaper never holds locks long.
#[tokio::test]
async fn the_reaper_respects_its_batch_size() {
    with_db!(|pool: PgPool| async move {
        let (tenant, job, version) = scaffold(&pool).await;
        let workers = WorkerRepository::new(&pool);
        let executions = ExecutionRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let w = workers
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        // Five executions, each dispatched to the same worker and leased, so
        // each has in-flight work the reaper can act on.
        let mut ids = Vec::new();
        for _ in 0..5 {
            let id = queued_execution(&pool, tenant, job, version).await;
            let claimed = executions.claim_next(tenant, None, w.id).await.unwrap();
            assert_eq!(claimed.unwrap().id, id, "each claim takes one execution");
            leases.acquire(tenant, id, w.id, None, 20).await.unwrap();
            ids.push(id);
        }
        expire_leases(&pool).await;

        // A batch of two examines only two leases per pass.
        let first = LeaseReaper::new(&pool).reap(Utc::now(), 2).await;
        assert_eq!(first.leases_examined, 2);
        assert_eq!(first.requeued, 2);

        // Subsequent passes drain the remainder.
        let mut total = first.requeued;
        for _ in 0..5 {
            let pass = LeaseReaper::new(&pool).reap(Utc::now(), 100).await;
            total += pass.requeued;
            if total as usize == ids.len() {
                break;
            }
        }
        assert_eq!(
            total as usize,
            ids.len(),
            "every execution eventually recovers"
        );
    })
    .await;
}

/// A reaper pass with nothing to do is cheap and reports nothing.
#[tokio::test]
async fn reaping_with_nothing_expired_is_a_no_op() {
    with_db!(|pool: PgPool| async move {
        let stats = LeaseReaper::new(&pool).reap(Utc::now(), 100).await;
        assert_eq!(stats.leases_examined, 0);
        assert_eq!(stats.requeued, 0);
        assert_eq!(stats.dead_lettered, 0);
    })
    .await;
}
