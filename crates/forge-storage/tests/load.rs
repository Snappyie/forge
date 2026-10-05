//! Load behaviour for the claim path, which is what a scheduler does most.
//!
//! §6 asks for concurrency limits and queue policies to be "enforced under
//! load", and says to choose concrete throughput targets *after* estimating the
//! expected workload. This measures rather than asserts a number: it records
//! throughput and p99 latency, and fails only on the properties that must hold
//! regardless of workload - that no execution is claimed twice, and that no work
//! is lost or duplicated.
//!
//! A hard-coded "must do N requests a second" would be a number invented here
//! rather than derived from a real deployment, which is exactly what §6 warns
//! against. The figures are printed so a real target can be set from evidence.
//!
//! Ignored by default because it is slow and needs shared infrastructure:
//!
//! ```text
//! cargo test -p forge-storage --test load -- --ignored --nocapture
//! ```

use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    async fn new() -> Option<TestDb> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let db_name = format!("forge_load_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new().max_connections(1).connect(&admin_url).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping load test: cannot connect ({e})");
                return None;
            }
        };
        if admin
            .execute(sqlx::AssertSqlSafe(format!(r#"CREATE DATABASE "{db_name}""#)))
            .await
            .is_err()
        {
            eprintln!("skipping load test: cannot create database");
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
                eprintln!("skipping load test: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
            eprintln!("skipping load test: migrations failed ({e})");
            return None;
        }
        Some(TestDb { pool, admin_url, db_name })
    }

    /// The connection string for this database, so a second pool can be opened
    /// over the same URL - a restarted process, or a differently-sized pool.
    fn url(&self) -> String {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        format!("{}/{}", server, self.db_name)
    }

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
}

/// Workers claiming at once. Higher than the pool's connection count on purpose:
/// a limit that only holds when requests never queue for a connection is not
/// enforced under load.
const CONCURRENCY: usize = 32;

/// Executions queued before the run.
const WORK: usize = 200;

/// Connections the pool under test is allowed - deliberately fewer than the
/// concurrency, so requests queue. A pool larger than the load measures nothing.
const POOL_CONNECTIONS: u32 = 10;

struct Sample {
    elapsed: std::time::Duration,
    admitted: bool,
    claimed: Option<Uuid>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "slow and needs shared infrastructure; run with --ignored --nocapture"]
async fn the_claim_path_holds_under_load() {
    let Some(db) = TestDb::new().await else {
        return;
    };

    let tenant_id = Uuid::new_v4();
    let queue_id = Uuid::new_v4();
    let worker_id = Uuid::new_v4();

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Load')")
        .bind(tenant_id)
        .execute(&db.pool)
        .await
        .expect("tenant");

    // A queue with a real concurrency limit, so admission control is under load
    // rather than idle.
    sqlx::query(
        "INSERT INTO queues (id, tenant_id, name, max_concurrency)
         VALUES ($1, $2, 'load', $3)",
    )
    .bind(queue_id)
    .bind(tenant_id)
    .bind(32)
    .execute(&db.pool)
    .await
    .expect("queue");

    sqlx::query(
        "INSERT INTO workers (id, tenant_id, name, hostname, status, draining, last_heartbeat_at)
         VALUES ($1, $2, 'w', 'localhost', 'READY', FALSE, NOW())",
    )
    .bind(worker_id)
    .bind(tenant_id)
    .execute(&db.pool)
    .await
    .expect("worker");

    // An execution must belong to a job or a workflow (`executions_subject_check`),
    // so a real job is seeded rather than a null subject. The claim path does not
    // read the job, but a row the database would refuse is not a row under test.
    // `jobs.environment_id` is NOT NULL (migration 026): every job belongs to an
    // environment, which is what makes cross-environment migration meaningful.
    let environment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO environments (id, tenant_id, slug, name, kind)
         VALUES ($1, $2, 'load', 'Load', 'other')",
    )
    .bind(environment_id)
    .bind(tenant_id)
    .execute(&db.pool)
    .await
    .expect("environment");

    let job_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs (id, tenant_id, name, status, default_queue_id, environment_id)
         VALUES ($1, $2, 'load', 'ACTIVE', $3, $4)",
    )
    .bind(job_id)
    .bind(tenant_id)
    .bind(queue_id)
    .bind(environment_id)
    .execute(&db.pool)
    .await
    .expect("job");

    for _ in 0..WORK {
        sqlx::query(
            "INSERT INTO executions (id, tenant_id, job_id, status, priority, queue_id, created_at)
             VALUES ($1, $2, $3, 'QUEUED', 'NORMAL', $4, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(job_id)
        .bind(queue_id)
        .execute(&db.pool)
        .await
        .expect("seed an execution");
    }

    let pool: PgPool = PgPoolOptions::new()
        .max_connections(POOL_CONNECTIONS)
        .connect(&db.url())
        .await
        .expect("load pool");

    // `claim_next` as the dequeue endpoint runs it: an UPDATE whose subselect
    // takes FOR UPDATE SKIP LOCKED. Under concurrency this is the statement that
    // must never hand the same row to two callers.
    let claim = {
        let pool = pool.clone();
        move || {
            let pool = pool.clone();
            async move {
                let started = std::time::Instant::now();
                let row: Option<(Uuid,)> = sqlx::query_as(
                    "UPDATE executions SET status = 'DISPATCHED', worker_id = $2,
                            enqueued_at = NOW()
                     WHERE id = (
                         SELECT e.id FROM executions e
                         JOIN workers w ON w.id = $2
                         WHERE e.status = 'QUEUED' AND e.tenant_id = $1
                           AND ($3::uuid IS NULL OR e.queue_id = $3)
                         ORDER BY e.created_at ASC
                         FOR UPDATE SKIP LOCKED
                         LIMIT 1)
                     RETURNING id",
                )
                .bind(tenant_id)
                .bind(worker_id)
                .bind(queue_id)
                .fetch_optional(&pool)
                .await
                .expect("claim must not error under load");
                Sample {
                    elapsed: started.elapsed(),
                    admitted: row.is_some(),
                    claimed: row.map(|(id,)| id),
                }
            }
        }
    };

    let started = std::time::Instant::now();
    let mut handles = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        handles.push(tokio::spawn(claim()));
    }
    let mut samples = Vec::with_capacity(CONCURRENCY);
    for handle in handles {
        samples.push(handle.await.expect("a load task must not panic"));
    }
    let wall = started.elapsed();

    let admitted = samples.iter().filter(|s| s.admitted).count();
    let mut latencies: Vec<u128> = samples.iter().map(|s| s.elapsed.as_millis()).collect();
    latencies.sort_unstable();
    let p50 = latencies[latencies.len() / 2];
    let p99 = latencies[(latencies.len() * 99 / 100).min(latencies.len() - 1)];
    let rps = admitted as f64 / wall.as_secs_f64();

    println!("--- claim path under load ---");
    println!("  workers claiming concurrently : {CONCURRENCY}");
    println!("  executions available           : {WORK}");
    println!("  pool connections              : {POOL_CONNECTIONS}");
    println!("  admitted                      : {admitted}");
    println!("  wall clock                    : {} ms", wall.as_millis());
    println!("  throughput                    : {rps:.1} claims/s");
    println!("  p50 latency                   : {p50} ms");
    println!("  p99 latency                   : {p99} ms");

    assert_eq!(
        samples.len(),
        CONCURRENCY,
        "not every load request completed; a pool deadlock or lock timeout would \
         show up here"
    );

    // The property that actually matters under contention: no execution was
    // handed to two callers. Two workers claiming one execution applies its side
    // effect twice.
    let mut claimed_ids: Vec<Uuid> = samples.iter().filter_map(|s| s.claimed).collect();
    claimed_ids.sort_unstable();
    claimed_ids.dedup();
    assert_eq!(
        claimed_ids.len(),
        admitted,
        "{admitted} claims were admitted but {} distinct executions were returned; \
         FOR UPDATE SKIP LOCKED admitted the same row twice",
        claimed_ids.len()
    );

    // Every admitted id must really be one of the rows we seeded, and no other
    // row may have moved.
    let dispatched: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM executions WHERE tenant_id = $1 AND status = 'DISPATCHED'",
    )
    .bind(tenant_id)
    .fetch_all(&db.pool)
    .await
    .expect("count dispatched");
    assert_eq!(
        dispatched.len(),
        admitted,
        "{} executions are DISPATCHED but {admitted} claims were admitted",
        dispatched.len()
    );

    // Work is queued, not lost or duplicated.
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM executions WHERE tenant_id = $1 AND status = 'QUEUED'",
    )
    .bind(tenant_id)
    .fetch_one(&db.pool)
    .await
    .expect("count queued");
    assert_eq!(
        queued as usize + admitted,
        WORK,
        "work was lost or duplicated: {queued} queued + {admitted} dispatched != {WORK}"
    );

    // The run must actually have happened, and must have finished. A zero or an
    // unbounded wall clock would make the figures above fiction.
    assert!(
        wall.as_secs() < 60,
        "the load run took {} ms; the pool is not draining",
        wall.as_millis()
    );
    assert!(
        admitted > 0,
        "no work was admitted; the load never ran, so the measurements above mean nothing"
    );

    println!("  no duplicate claims, no work lost");

    pool.close().await;
    db.cleanup().await;
}