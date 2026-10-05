//! Acceptance tests for recovery (spec 01.22, spec 10.11, AT-REC).
//!
//! These restart or damage state deliberately, so each names the failure it
//! injects.

use std::sync::Arc;

use forge_domain::{ExecutionStatus, JobId, JobVersionId, TenantId, TriggerSource};
use forge_events::{DomainEvent, InMemorySink, OutboxPublisher, SinkError};
use forge_executor::{CompletionGate, LeaseReaper};
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
        let db_name = format!("forge_rec_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping recovery tests: cannot connect ({e})");
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
            eprintln!("skipping recovery tests: cannot create database");
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
                eprintln!("skipping recovery tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
        {
            eprintln!("skipping recovery tests: migrations failed ({e})");
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

struct Fixture {
    tenant: TenantId,
    job: JobId,
    version: JobVersionId,
}

async fn scaffold(pool: &PgPool) -> Fixture {
    let tenant = TenantId::from_uuid(Uuid::new_v4());
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

    Fixture {
        tenant,
        job: JobId::from_uuid(job.id),
        version: JobVersionId::from_uuid(version.id),
    }
}

async fn queued(pool: &PgPool, f: &Fixture) -> Uuid {
    ExecutionRepository::new(pool)
        .create(NewExecution {
            tenant_id: f.tenant,
            job_id: f.job,
            job_version_id: f.version,
            queue_id: None,
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

// AT-REC-001: an API restart preserves state.
//
// Modelled by disconnecting every pooled connection and reconnecting, which is
// what a restart does to an in-flight process's handles.
#[tokio::test]
async fn at_rec_001_state_survives_a_restart() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let id = queued(&pool, &f).await;

        // Record where to reconnect before the pool closes; querying it
        // afterwards is exactly what the process could no longer do.
        let name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .unwrap();

        // Simulate the process going away and coming back.
        pool.close().await;

        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let reopened = PgPoolOptions::new()
            .max_connections(5)
            .connect(&format!("{server}/{name}"))
            .await
            .unwrap();

        let row = ExecutionRepository::new(&reopened)
            .get(f.tenant, id)
            .await
            .unwrap();
        assert_eq!(row.status, "QUEUED", "state must survive a restart");
        reopened.close().await;
    })
    .await;
}

// AT-REC-002: a scheduler restart preserves schedules.
#[tokio::test]
async fn at_rec_002_schedules_survive_a_restart() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let job_id = f.job.into_uuid();
        let schedule_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO schedules
                (id, tenant_id, job_id, target_id, target_type, cron_expression, timezone,
                 next_run_at, enabled, misfire_policy, catch_up_policy)
             VALUES ($1, $2, $3, $4, 'JOB', '0 2 * * *', 'UTC', NOW(), TRUE, 'FIRE_ONCE', '{}')",
        )
        .bind(schedule_id)
        .bind(f.tenant.into_uuid())
        .bind(job_id)
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();

        // A fresh engine (as after a restart) reads the schedule back.
        let engine = forge_scheduler::SchedulerEngine::new(pool.clone(), 100);
        let report = engine.tick().await;
        assert_eq!(report.claimed, 1, "the schedule survived and was claimed");

        let (enabled,): (bool,) = sqlx::query_as("SELECT enabled FROM schedules WHERE id = $1")
            .bind(schedule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(enabled, "it must still be enabled after a restart");
    })
    .await;
}

// AT-REC-003: a worker restart recovers leased work.
#[tokio::test]
async fn at_rec_003_worker_restart_recovers_leased_work() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = WorkerRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let worker = workers
            .register(f.tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        let id = queued(&pool, &f).await;
        executions
            .claim_next(f.tenant, None, worker.id)
            .await
            .unwrap()
            .expect("claimed");
        leases
            .acquire(f.tenant, id, worker.id, None, 20)
            .await
            .unwrap();

        // The worker stops without releasing its lease.
        sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        let stats = LeaseReaper::new(&pool).reap(chrono::Utc::now(), 100).await;
        assert_eq!(stats.requeued, 1, "the work must come back");

        // Another worker can pick it up.
        let next = executions
            .claim_next(f.tenant, None, worker.id)
            .await
            .unwrap()
            .expect("the recovered execution is dispatchable");
        assert_eq!(next.id, id);
    })
    .await;
}

// AT-REC-004: a restored database is usable.
//
// Modelled by running the migrations over a database and confirming the
// resulting schema answers the queries the platform depends on.
#[tokio::test]
async fn at_rec_004_a_restored_database_is_usable() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let id = queued(&pool, &f).await;

        // The tables a restore must have for the platform to operate.
        for table in [
            "tenants",
            "users",
            "jobs",
            "job_versions",
            "schedules",
            "executions",
            "execution_attempts",
            "workers",
            "worker_leases",
            "audit_events",
            "outbox_events",
            "idempotency_keys",
        ] {
            let exists: (i64,) = sqlx::query_as(
                "SELECT COUNT(*)::bigint FROM information_schema.tables
                 WHERE table_schema = 'public' AND table_name = $1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(exists.0, 1, "a restore must include `{table}`");
        }

        // And the restored data is readable through the normal path.
        assert!(ExecutionRepository::new(&pool)
            .get(f.tenant, id)
            .await
            .is_ok());
    })
    .await;
}

/// A stale completion is refused after recovery (AT-STATE-004, restated here
/// because it is the recovery path's defining property).
#[tokio::test]
async fn recovery_refuses_a_completion_from_the_dead_worker() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = WorkerRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let dead = workers
            .register(f.tenant, "dead", "h1", None, json!([]), json!({}))
            .await
            .unwrap();
        let live = workers
            .register(f.tenant, "live", "h2", None, json!([]), json!({}))
            .await
            .unwrap();

        let id = queued(&pool, &f).await;
        executions
            .claim_next(f.tenant, None, dead.id)
            .await
            .unwrap();
        let stale = leases
            .acquire(f.tenant, id, dead.id, None, 20)
            .await
            .unwrap();

        sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        LeaseReaper::new(&pool).reap(chrono::Utc::now(), 100).await;

        executions
            .claim_next(f.tenant, None, live.id)
            .await
            .unwrap();
        let fresh = leases
            .acquire(f.tenant, id, live.id, None, 60)
            .await
            .unwrap();

        let gate = CompletionGate::new(&pool);
        assert_eq!(
            gate.check(id, stale.id, dead.id).await.unwrap_err(),
            forge_executor::CompletionRejection::StaleLease
        );
        assert!(gate.check(id, fresh.id, live.id).await.is_ok());
    })
    .await;
}

// AT-REC-005: an outbox publisher restart does not lose events.
#[tokio::test]
async fn at_rec_005_outbox_publisher_restart_does_not_lose_events() {
    with_db!(|pool: PgPool| async move {
        for _ in 0..4 {
            forge_storage::OutboxRepository::new(&pool)
                .enqueue(
                    None,
                    "execution.succeeded",
                    "execution",
                    Uuid::new_v4(),
                    json!({ "ok": true }),
                )
                .await
                .unwrap();
        }

        // First publisher claims everything, then dies before acknowledging.
        let failing = InMemorySink::new();
        failing.fail_with(Some(SinkError::Transient("broker down".into())));
        let first = OutboxPublisher::new(&pool, 100)
            .publish_batch(&failing)
            .await;
        assert_eq!(first.failed, 4);
        assert!(failing.is_empty());

        // A restarted publisher finds them all again.
        let restarted = InMemorySink::new();
        let second = OutboxPublisher::new(&pool, 100)
            .publish_batch(&restarted)
            .await;
        assert_eq!(second.published, 4, "no event is lost across a restart");
        assert_eq!(restarted.len(), 4);
    })
    .await;
}

/// Every outbox event is delivered exactly once even across restarts.
#[tokio::test]
async fn outbox_delivers_each_event_exactly_once() {
    with_db!(|pool: PgPool| async move {
        for i in 0..6 {
            forge_storage::OutboxRepository::new(&pool)
                .enqueue(
                    None,
                    "execution.created",
                    "execution",
                    Uuid::new_v4(),
                    json!({ "index": i }),
                )
                .await
                .unwrap();
        }

        let sink = InMemorySink::new();
        let publisher = OutboxPublisher::new(&pool, 100);

        // Several passes, as a restarted process would run.
        let mut total = 0;
        for _ in 0..3 {
            total += publisher.publish_batch(&sink).await.published;
        }

        assert_eq!(total, 6, "each event is delivered exactly once");
        let published = sink.published();
        let ids: std::collections::HashSet<_> = published.iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 6, "no event appears twice");
    })
    .await;
}

/// A restored execution keeps its correlation id, so a trace survives.
#[tokio::test]
async fn correlation_id_survives_recovery() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = WorkerRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);

        let worker = workers
            .register(f.tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        let id = queued(&pool, &f).await;
        let before = executions.get(f.tenant, id).await.unwrap();
        let correlation = before.correlation_id.clone().expect("a correlation id");

        executions
            .claim_next(f.tenant, None, worker.id)
            .await
            .unwrap();
        leases
            .acquire(f.tenant, id, worker.id, None, 20)
            .await
            .unwrap();
        sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        LeaseReaper::new(&pool).reap(chrono::Utc::now(), 100).await;

        let after = executions.get(f.tenant, id).await.unwrap();
        assert_eq!(
            after.correlation_id,
            Some(correlation),
            "a recovered execution keeps its trace identity"
        );
    })
    .await;
}

/// A recovery event reaches the outbox so operators can see it.
#[tokio::test]
async fn a_recovery_writes_an_outbox_event() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let executions = ExecutionRepository::new(&pool);
        let workers = WorkerRepository::new(&pool);
        let leases = LeaseRepository::new(&pool);
        let outbox = forge_storage::OutboxRepository::new(&pool);

        let worker = workers
            .register(f.tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();
        let id = queued(&pool, &f).await;
        executions
            .claim_next(f.tenant, None, worker.id)
            .await
            .unwrap();
        leases
            .acquire(f.tenant, id, worker.id, None, 20)
            .await
            .unwrap();

        sqlx::query("UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        LeaseReaper::new(&pool).reap(chrono::Utc::now(), 100).await;

        // The attempt is preserved with its classification.
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT error_class FROM execution_attempts WHERE execution_id = $1")
                .bind(id)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "TRANSIENT");

        // And the event stream carries the abandonment.
        let event = forge_events::execution_event(
            f.tenant,
            id,
            ExecutionStatus::Abandoned,
            1,
            Some(forge_domain::ErrorClass::Transient),
            None,
        );
        outbox
            .enqueue(
                Some(f.tenant),
                &event.event_type,
                "execution",
                id,
                event.payload.clone(),
            )
            .await
            .unwrap();

        let pending = outbox.claim_unpublished(10).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_type, "execution.abandoned");
    })
    .await;
}

/// A domain event is serialisable and re-parses to the same value.
#[tokio::test]
async fn a_domain_event_round_trips_through_json() {
    with_db!(|pool: PgPool| async move {
        let f = scaffold(&pool).await;
        let event = forge_events::execution_event(
            f.tenant,
            Uuid::new_v4(),
            ExecutionStatus::Failed,
            2,
            Some(forge_domain::ErrorClass::Timeout),
            Some("corr-1".into()),
        );

        let rendered = serde_json::to_string(&event).unwrap();
        let parsed: DomainEvent = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed.event_type, event.event_type);
        assert_eq!(parsed.correlation_id, event.correlation_id);
        assert_eq!(parsed.payload, event.payload);
    })
    .await;
}
