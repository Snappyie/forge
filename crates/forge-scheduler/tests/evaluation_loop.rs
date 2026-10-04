//! Integration tests for the schedule evaluation loop.
//!
//! Mirrors the storage suite's database handling: each test gets a throwaway
//! database, and the whole body is skipped when no PostgreSQL is reachable.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use forge_scheduler::{FixedClock, SchedulerEngine};
use forge_storage::{
    ExecutionFilter, ExecutionRepository, JobRepository, JobVersionRepository, NewExecution,
    NewJobVersion, ScheduleRepository, WorkerRepository,
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
        let db_name = format!("forge_sched_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping scheduler integration tests: cannot connect ({e})");
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
            eprintln!("skipping scheduler integration tests: cannot create database");
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
                eprintln!("skipping scheduler integration tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
        {
            eprintln!("skipping scheduler integration tests: migrations failed ({e})");
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
                    let outcome = tokio::spawn(std::panic::AssertUnwindSafe(
                        $body(db.pool.clone()),
                    ))
                    .await;
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

/// Creates a tenant plus an active job with a published version, and returns
/// the job id.
async fn scaffold_job(db: &PgPool, tenant: forge_domain::TenantId) -> uuid::Uuid {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Sched Tenant')")
        .bind(tenant.into_uuid())
        .execute(db)
        .await
        .unwrap();

    let jobs = JobRepository::new(db);
    let job = jobs
        .create(
            tenant,
            Some(format!("j{}", Uuid::new_v4().simple())),
            "Scheduled Job",
            None,
            forge_domain::Priority::Normal,
            None,
        )
        .await
        .unwrap();

    let versions = JobVersionRepository::new(db);
    let v = versions
        .create(
            tenant,
            forge_domain::JobId::from_uuid(job.id),
            &NewJobVersion::default(),
        )
        .await
        .unwrap();
    versions
        .publish(
            tenant,
            forge_domain::JobId::from_uuid(job.id),
            forge_domain::JobVersionId::from_uuid(v.id),
        )
        .await
        .unwrap();

    job.id
}

/// Everything a schedule row needs, so the helper stays readable.
struct ScheduleSpec {
    schedule_type: &'static str,
    expression: Option<&'static str>,
    timezone: &'static str,
    next_run_at: DateTime<Utc>,
    misfire: &'static str,
    catch_up_max: Option<u32>,
    interval_seconds: Option<i64>,
    one_time_at: Option<DateTime<Utc>>,
    version_policy: &'static str,
    pinned_version_id: Option<Uuid>,
}

impl ScheduleSpec {
    fn cron(expression: &'static str, next_run_at: DateTime<Utc>) -> Self {
        Self {
            schedule_type: "CRON",
            expression: Some(expression),
            timezone: "UTC",
            next_run_at,
            misfire: "FIRE_ONCE",
            catch_up_max: None,
            interval_seconds: None,
            one_time_at: None,
            version_policy: "LATEST_PUBLISHED",
            pinned_version_id: None,
        }
    }

    fn tz(expression: &'static str, timezone: &'static str, next_run_at: DateTime<Utc>) -> Self {
        Self {
            timezone,
            ..Self::cron(expression, next_run_at)
        }
    }

    /// A one-time schedule fires at exactly `at`, so its stored `next_run_at`
    /// is the same instant (the engine rejects a row where they disagree).
    fn one_time(at: DateTime<Utc>) -> Self {
        Self {
            schedule_type: "ONE_TIME",
            expression: None,
            timezone: "UTC",
            next_run_at: at,
            misfire: "FIRE_ONCE",
            catch_up_max: None,
            interval_seconds: None,
            one_time_at: Some(at),
            version_policy: "LATEST_PUBLISHED",
            pinned_version_id: None,
        }
    }

    fn interval(seconds: i64, next_run_at: DateTime<Utc>) -> Self {
        Self {
            schedule_type: "INTERVAL",
            expression: None,
            timezone: "UTC",
            next_run_at,
            misfire: "FIRE_ONCE",
            catch_up_max: None,
            interval_seconds: Some(seconds),
            one_time_at: None,
            version_policy: "LATEST_PUBLISHED",
            pinned_version_id: None,
        }
    }

    fn misfire(mut self, misfire: &'static str) -> Self {
        self.misfire = misfire;
        self
    }

    fn catch_up(mut self, max: u32) -> Self {
        self.catch_up_max = Some(max);
        self
    }

    fn pinned(mut self, version_id: Uuid) -> Self {
        self.version_policy = "PINNED";
        self.pinned_version_id = Some(version_id);
        self
    }
}

async fn insert_schedule(
    db: &PgPool,
    tenant: forge_domain::TenantId,
    job_id: uuid::Uuid,
    spec: ScheduleSpec,
) -> uuid::Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO schedules
            (id, tenant_id, job_id, target_id, target_type, target_version_policy,
             pinned_version_id, schedule_type, cron_expression, interval_seconds, one_time_at,
             timezone, next_run_at, enabled, misfire_policy, catch_up_policy)
         VALUES ($1, $2, $3, $4, 'JOB', $5, $6, $7, $8, $9, $10, $11, $12, TRUE, $13, $14)",
    )
    .bind(id)
    .bind(tenant.into_uuid())
    .bind(job_id)
    .bind(job_id)
    .bind(spec.version_policy)
    .bind(spec.pinned_version_id)
    .bind(spec.schedule_type)
    .bind(spec.expression)
    .bind(spec.interval_seconds)
    .bind(spec.one_time_at)
    .bind(spec.timezone)
    .bind(spec.next_run_at)
    .bind(spec.misfire)
    .bind(json!({ "max_occurrences": spec.catch_up_max.unwrap_or(100) }))
    .execute(db)
    .await
    .unwrap();
    id
}

/// The stored state of a schedule, for assertions about enabling/disabling.
struct ScheduleState {
    enabled: bool,
    is_paused: bool,
    next_run_at: Option<DateTime<Utc>>,
    disabled_reason: Option<String>,
}

async fn schedule_state(db: &PgPool, schedule_id: Uuid) -> ScheduleState {
    let (enabled, is_paused, next_run_at, disabled_reason): (
        bool,
        bool,
        Option<DateTime<Utc>>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT enabled, is_paused, next_run_at, disabled_reason FROM schedules WHERE id = $1",
    )
    .bind(schedule_id)
    .fetch_one(db)
    .await
    .unwrap();
    ScheduleState {
        enabled,
        is_paused,
        next_run_at,
        disabled_reason,
    }
}

// AT-SCH-001: a one-time schedule produces exactly one execution.
#[tokio::test]
async fn a_due_schedule_produces_one_execution() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));

        let report = engine.tick().await;
        assert_eq!(report.claimed, 1);
        assert_eq!(report.executions_created, 1);
        assert_eq!(report.errors.len(), 0, "{:?}", report.errors);

        let executions = ExecutionRepository::new(&pool);
        let all = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(all.items.len(), 1);
        assert_eq!(all.items[0].status, "QUEUED");
        assert_eq!(all.items[0].trigger_source, "SCHEDULE");
    })
    .await;
}

/// The loop must be idempotent: a second tick with nothing due creates nothing.
#[tokio::test]
async fn a_second_tick_creates_nothing_new() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));

        engine.tick().await;
        // The schedule's next_run_at has advanced past `now`, so nothing is due.
        let second = engine.tick().await;

        assert_eq!(second.claimed, 0);
        assert_eq!(second.executions_created, 0);

        let executions = ExecutionRepository::new(&pool);
        let all = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(
            all.items.len(),
            1,
            "the occurrence must not be created twice"
        );
    })
    .await;
}

// AT-SCH-003: a paused schedule creates no executions.
#[tokio::test]
async fn a_disabled_schedule_is_never_claimed() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let id = insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::hours(1)).misfire("CATCH_UP"),
        )
        .await;

        ScheduleRepository::new(&pool)
            .pause(tenant, id)
            .await
            .unwrap();

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(report.claimed, 0);
        assert_eq!(report.executions_created, 0);

        let executions = ExecutionRepository::new(&pool);
        assert!(executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap()
            .items
            .is_empty());
    })
    .await;
}

// AT-SCH-004: a resumed schedule continues correctly.
#[tokio::test]
async fn a_resumed_schedule_fires_again() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let id = insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let schedules = ScheduleRepository::new(&pool);
        schedules.pause(tenant, id).await.unwrap();

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));

        assert_eq!(engine.tick().await.executions_created, 0);

        schedules.resume(tenant, id).await.unwrap();
        assert_eq!(engine.tick().await.executions_created, 1);
    })
    .await;
}

// AT-SCH-008: two schedulers racing produce exactly one execution.
#[tokio::test]
async fn concurrent_schedulers_create_one_execution_per_occurrence() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        // Four schedulers tick at the same instant.
        let shared = FixedClock::at(&now.to_rfc3339()).shared();
        let mut handles = Vec::new();
        for _ in 0..4 {
            // Each spawned engine needs its own pool handle and its own claim
            // identity, which `SchedulerEngine::new` generates per instance.
            let pool = pool.clone();
            let clock = shared.clone();
            handles.push(tokio::spawn(async move {
                let engine = SchedulerEngine::new(pool, 100).with_clock(clock);
                engine.tick().await
            }));
        }

        let mut total_created = 0;
        let mut total_claimed = 0;
        for h in handles {
            let report = h.await.unwrap();
            total_created += report.executions_created;
            total_claimed += report.claimed;
        }

        // Only one scheduler may claim the row (SKIP LOCKED).
        assert_eq!(total_claimed, 1, "SKIP LOCKED must let exactly one claim");
        assert_eq!(total_created, 1);

        let executions = ExecutionRepository::new(&pool);
        let all = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(
            all.items.len(),
            1,
            "exactly one execution for the occurrence"
        );
    })
    .await;
}

/// Even if two schedulers somehow act on the same occurrence, the unique index
/// rejects the second insert.
#[tokio::test]
async fn the_database_rejects_a_duplicate_occurrence() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;
        let version = JobVersionRepository::new(&pool)
            .latest_published(tenant, forge_domain::JobId::from_uuid(job_id))
            .await
            .unwrap()
            .unwrap();

        let now = Utc::now();
        let schedule_id = insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let executions = ExecutionRepository::new(&pool);
        let base = NewExecution {
            tenant_id: tenant,
            job_id: forge_domain::JobId::from_uuid(job_id),
            job_version_id: forge_domain::JobVersionId::from_uuid(version.id),
            queue_id: None,
            schedule_id: Some(schedule_id),
            trigger_source: forge_domain::TriggerSource::Schedule,
            priority: forge_domain::Priority::Normal,
            scheduled_for: Some(now - Duration::minutes(1)),
            correlation_id: None,
            input: json!({}),
        };

        executions.create(base.clone()).await.unwrap();
        let duplicate = executions.create(base).await;
        assert!(
            duplicate.is_err(),
            "the (schedule_id, scheduled_for) index must reject the duplicate"
        );
    })
    .await;
}

// AT-SCH-009: catch-up obeys the configured maximum.
#[tokio::test]
async fn catch_up_creates_at_most_the_configured_number() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        // Hourly, due six hours ago, ceiling of 3.
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::hours(6))
                .misfire("CATCH_UP")
                .catch_up(3),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(
            report.executions_created, 3,
            "capped at the configured ceiling"
        );
        assert!(
            report
                .skipped
                .iter()
                .any(|(_, _, r)| { *r == forge_scheduler::SkipReason::CatchUpLimit }),
            "occurrences beyond the ceiling must be reported, not silently dropped"
        );

        let executions = ExecutionRepository::new(&pool);
        assert_eq!(
            executions
                .list(tenant, &ExecutionFilter::default(), None, 50)
                .await
                .unwrap()
                .items
                .len(),
            3
        );
    })
    .await;
}

/// FIRE_ONCE collapses a long outage into a single execution.
#[tokio::test]
async fn fire_once_collapses_a_long_outage() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::days(3)).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        assert_eq!(engine.tick().await.executions_created, 1);
    })
    .await;
}

/// A schedule with no published version is skipped rather than producing an
/// execution with a dangling version reference.
#[tokio::test]
async fn a_job_with_no_published_version_is_skipped() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        // Unpublish by pointing the job at nothing.
        sqlx::query("UPDATE jobs SET current_version_id = NULL WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE job_versions SET published_at = NULL WHERE job_id = $1")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(report.executions_created, 0, "no version, no execution");
        // The schedule still advances, so it is not retried forever.
        assert_eq!(report.schedules_advanced, 1);
    })
    .await;
}

/// An invalid cron expression disables the schedule rather than wedging the
/// batch.
#[tokio::test]
async fn an_invalid_cron_disables_rather_than_blocking() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let id = insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("not a cron", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(report.executions_created, 0);
        assert_eq!(report.errors.len(), 0, "a bad expression is not an error");

        // Disabled, with the reason recorded, so it will not be claimed again.
        let state = schedule_state(&pool, id).await;
        assert!(!state.enabled, "an unusable schedule is disabled");
        assert!(!state.is_paused, "the engine disables rather than pausing");
        assert_eq!(
            state.disabled_reason.as_deref(),
            Some("INVALID_CONFIGURATION")
        );
    })
    .await;
}

// AT-SCH-005: a timezone schedule fires at the intended local time.
#[tokio::test]
async fn a_timezone_schedule_fires_at_its_local_time() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        // 02:00 Asia/Kolkata, due a minute ago.
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::tz("0 2 * * *", "Asia/Kolkata", now - Duration::minutes(1))
                .misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        assert_eq!(engine.tick().await.executions_created, 1);

        let executions = ExecutionRepository::new(&pool);
        let all = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(all.items.len(), 1);
    })
    .await;
}

/// The scheduler must not disturb workers or the dispatch queue.
#[tokio::test]
async fn scheduling_does_not_claim_dispatchable_work() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        engine.tick().await;

        // A worker can immediately claim what the scheduler produced.
        let worker = WorkerRepository::new(&pool)
            .register(tenant, "w", "h", None, json!([]), json!({}))
            .await
            .unwrap();

        let executions = ExecutionRepository::new(&pool);
        let claimed = executions
            .claim_next(tenant, None, worker.id)
            .await
            .unwrap()
            .expect("the scheduled execution is dispatchable");
        assert_eq!(claimed.status, "DISPATCHED");
    })
    .await;
}

/// A batch limit bounds how much one tick does.
#[tokio::test]
async fn the_batch_size_bounds_a_tick() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        for _ in 0..5 {
            insert_schedule(
                &pool,
                tenant,
                job_id,
                ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).misfire("FIRE_ONCE"),
            )
            .await;
        }

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 2).with_clock(Arc::new(clock));
        assert_eq!(engine.tick().await.claimed, 2);

        let executions = ExecutionRepository::new(&pool);
        assert_eq!(
            executions
                .list(tenant, &ExecutionFilter::default(), None, 50)
                .await
                .unwrap()
                .items
                .len(),
            2
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// One-time and interval schedules (spec 01.5)
// ---------------------------------------------------------------------------

/// AT-SCH-001 for a one-shot: it fires exactly once, then finishes with no
/// next run. It must be disabled as *completed*, not as an error, so an
/// operator can tell a finished one-shot from a broken schedule.
#[tokio::test]
async fn a_one_time_schedule_fires_once_and_is_completed_not_paused() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let instant = now - Duration::minutes(1);
        let schedule_id =
            insert_schedule(&pool, tenant, job_id, ScheduleSpec::one_time(instant)).await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(report.claimed, 1);
        assert_eq!(report.executions_created, 1);
        assert_eq!(report.errors.len(), 0, "{:?}", report.errors);

        let state = schedule_state(&pool, schedule_id).await;
        assert!(!state.enabled, "a fired one-shot is disabled");
        assert!(
            !state.is_paused,
            "a completed one-shot is not an operator pause"
        );
        assert_eq!(
            state.next_run_at, None,
            "a completed one-shot has no next run"
        );
        assert_eq!(
            state.disabled_reason.as_deref(),
            Some("COMPLETED_ONE_TIME"),
            "the completion reason is recorded"
        );

        let executions = ExecutionRepository::new(&pool);
        let all = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(all.items.len(), 1);
        assert_eq!(all.items[0].scheduled_for, Some(instant));

        // A restart — a fresh engine over the same rows — must not fire it
        // again. This is the "including across restarts" half of the rule.
        let restarted = SchedulerEngine::new(pool.clone(), 100)
            .with_clock(Arc::new(FixedClock::at(&now.to_rfc3339())));
        let second = restarted.tick().await;
        assert_eq!(second.claimed, 0, "a completed one-shot is never reclaimed");
        assert_eq!(second.executions_created, 0);

        let after_restart = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(
            after_restart.items.len(),
            1,
            "a one-time occurrence must never fire twice"
        );
    })
    .await;
}

/// Regression: a non-cron schedule must not be paused or disabled for being
/// non-cron. Before migration 015 the engine parsed a NULL expression as cron,
/// failed, and switched the schedule off.
#[tokio::test]
async fn an_interval_schedule_is_not_disabled_and_advances_by_its_period() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let due = now - Duration::minutes(1);
        let schedule_id = insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::interval(3600, due).misfire("FIRE_ONCE"),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(report.claimed, 1);
        assert_eq!(report.executions_created, 1);
        assert_eq!(report.errors.len(), 0, "{:?}", report.errors);

        let state = schedule_state(&pool, schedule_id).await;
        assert!(
            state.enabled,
            "an interval schedule must keep running, not be disabled"
        );
        assert!(!state.is_paused, "and it must not be paused");
        assert_eq!(state.disabled_reason, None);
        assert_eq!(
            state.next_run_at,
            Some(due + Duration::seconds(3600)),
            "the next run is exactly one period after the intended occurrence"
        );

        // The next occurrence is an hour away, so a second tick claims nothing.
        let second = engine.tick().await;
        assert_eq!(second.claimed, 0);
        assert_eq!(second.executions_created, 0);
    })
    .await;
}

/// Spec 09.7/09.8: interval catch-up uses the interval period and honours the
/// configured ceiling, exactly as cron does.
#[tokio::test]
async fn an_interval_schedule_catches_up_within_its_ceiling() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let due = now - Duration::minutes(10);
        let schedule_id = insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::interval(60, due)
                .misfire("CATCH_UP")
                .catch_up(3),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(
            report.executions_created, 3,
            "capped at the configured ceiling"
        );
        assert!(report
            .skipped
            .iter()
            .any(|(_, _, reason)| *reason == forge_scheduler::SkipReason::CatchUpLimit));
        assert_eq!(report.errors.len(), 0, "{:?}", report.errors);

        let state = schedule_state(&pool, schedule_id).await;
        assert!(state.enabled, "catch-up must not disable the schedule");
        assert_eq!(state.next_run_at, Some(due + Duration::seconds(60)));
    })
    .await;
}

/// The claim lease and the occurrence index must still make a one-shot safe
/// under concurrent schedulers.
#[tokio::test]
async fn concurrent_schedulers_fire_a_one_time_schedule_once() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let instant = now - Duration::minutes(1);
        let schedule_id =
            insert_schedule(&pool, tenant, job_id, ScheduleSpec::one_time(instant)).await;

        let shared = FixedClock::at(&now.to_rfc3339()).shared();
        let mut handles = Vec::new();
        for _ in 0..4 {
            let pool = pool.clone();
            let clock = shared.clone();
            handles.push(tokio::spawn(async move {
                SchedulerEngine::new(pool, 100)
                    .with_clock(clock)
                    .tick()
                    .await
            }));
        }

        let mut total_created = 0;
        let mut total_claimed = 0;
        for handle in handles {
            let report = handle.await.unwrap();
            total_created += report.executions_created;
            total_claimed += report.claimed;
        }

        assert_eq!(total_claimed, 1, "SKIP LOCKED must let exactly one claim");
        assert_eq!(total_created, 1);

        let executions = ExecutionRepository::new(&pool);
        let all = executions
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(all.items.len(), 1, "the one-shot fires exactly once");

        let state = schedule_state(&pool, schedule_id).await;
        assert!(!state.enabled);
        assert_eq!(state.next_run_at, None);
    })
    .await;
}

/// Spec 02.4: PINNED runs the version recorded on the schedule, not the job's
/// current version.
#[tokio::test]
async fn a_pinned_schedule_runs_the_recorded_version() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        // `scaffold_job` publishes v1, which is the pinned version. Publish a
        // second version so the job's current version differs from the pin.
        let versions = JobVersionRepository::new(&pool);
        let v1 = versions
            .latest_published(tenant, forge_domain::JobId::from_uuid(job_id))
            .await
            .unwrap()
            .unwrap();
        let v2 = versions
            .create(
                tenant,
                forge_domain::JobId::from_uuid(job_id),
                &NewJobVersion::default(),
            )
            .await
            .unwrap();
        versions
            .publish(
                tenant,
                forge_domain::JobId::from_uuid(job_id),
                forge_domain::JobVersionId::from_uuid(v2.id),
            )
            .await
            .unwrap();

        let job = JobRepository::new(&pool)
            .get(tenant, forge_domain::JobId::from_uuid(job_id))
            .await
            .unwrap();
        assert_eq!(
            job.current_version_id,
            Some(v2.id),
            "the job's current version moved to v2"
        );

        let now = Utc::now();
        insert_schedule(
            &pool,
            tenant,
            job_id,
            ScheduleSpec::cron("0 * * * *", now - Duration::minutes(1)).pinned(v1.id),
        )
        .await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        assert_eq!(engine.tick().await.executions_created, 1);

        let all = ExecutionRepository::new(&pool)
            .list(tenant, &ExecutionFilter::default(), None, 50)
            .await
            .unwrap();
        assert_eq!(all.items.len(), 1);
        assert_eq!(
            all.items[0].job_version_id,
            Some(v1.id),
            "the execution must run the version the schedule pinned, not the current one"
        );
        assert_ne!(v1.id, v2.id);
    })
    .await;
}

/// A genuinely unusable stored configuration is disabled with a reason rather
/// than retried on every tick.
#[tokio::test]
async fn unusable_non_cron_configurations_are_disabled_with_a_reason() {
    with_db!(|pool: PgPool| async move {
        let tenant = forge_domain::TenantId::from_uuid(Uuid::new_v4());
        let job_id = scaffold_job(&pool, tenant).await;

        let now = Utc::now();
        let due = now - Duration::minutes(1);

        // An interval with a non-positive period.
        let zero_interval =
            insert_schedule(&pool, tenant, job_id, ScheduleSpec::interval(0, due)).await;

        // A one-time schedule with no instant.
        let mut no_instant = ScheduleSpec::one_time(due);
        no_instant.one_time_at = None;
        let missing_instant = insert_schedule(&pool, tenant, job_id, no_instant).await;

        // A one-time schedule whose stored anchor disagrees with its instant.
        let mut mismatched = ScheduleSpec::one_time(due);
        mismatched.one_time_at = Some(due + Duration::hours(1));
        let anchor_mismatch = insert_schedule(&pool, tenant, job_id, mismatched).await;

        let clock = FixedClock::at(&now.to_rfc3339());
        let engine = SchedulerEngine::new(pool.clone(), 100).with_clock(Arc::new(clock));
        let report = engine.tick().await;

        assert_eq!(report.executions_created, 0);
        assert_eq!(
            report.errors.len(),
            0,
            "an unusable configuration is handled, not surfaced as an error: {:?}",
            report.errors
        );

        for (schedule_id, label) in [
            (zero_interval, "non-positive interval"),
            (missing_instant, "one-time without an instant"),
            (anchor_mismatch, "one-time with a mismatched anchor"),
        ] {
            let state = schedule_state(&pool, schedule_id).await;
            assert!(!state.enabled, "{label} must be disabled");
            assert!(!state.is_paused, "{label} is disabled, not paused");
            assert_eq!(
                state.disabled_reason.as_deref(),
                Some("INVALID_CONFIGURATION"),
                "{label} records why it was disabled"
            );
        }
    })
    .await;
}
