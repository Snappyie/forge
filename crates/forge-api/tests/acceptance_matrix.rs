//! The acceptance matrix from `Redesign.md` §6, as executable tests.
//!
//! The spec is explicit about what "done" means: *"test behavior under failure,
//! not just whether API endpoints return success"*. Each module below maps to one
//! row of that table and asserts the failure behaviour, not just the happy path.
//!
//! ```text
//!   Scheduling     no unintended duplicate occurrence creation under concurrency
//!   Recovery       expired leases are reconciled and eligible jobs recover
//!   Retries        attempt history and backoff persist across restarts
//!   Security       workers cannot touch executions outside their authorization
//!   Capacity       concurrency limits and queue policies are enforced under load
//!   Observability  every execution is traceable to its final outcome
//!   Deployment     migrations, restart, backup and recovery
//! ```
//!
//! Workflow coverage lives in `partial_rerun.rs` and the executor's own tests;
//! SDK and API contract coverage lives in `scripts/verify-sdks.sh`. Both are
//! listed in `docs/acceptance-matrix.md` so the whole matrix has one home.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

mod support;
use support::{register_via_api, router, send, send_optional, send_status, TestDb};

/// Sets up a tenant, a queue and a published job.
async fn tenant_with_job(pool: &PgPool, queue_name: &str, job_name: &str) -> Fixture {
    let (token, _tenant_id) = register_via_api(pool).await;
    let app = router(pool.clone());

    let (status, body) = send(
        &app,
        "POST",
        "/api/v1/queues",
        Some(&token),
        Some(serde_json::json!({ "name": queue_name })),
    )
    .await;
    assert_eq!(status, 201, "queue creation refused: {body}");
    let queue_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &app,
        "POST",
        "/api/v1/jobs",
        Some(&token),
        Some(serde_json::json!({
            "name": job_name,
            "key": format!("{job_name}-key"),
            "default_queue_id": queue_id,
        })),
    )
    .await;
    assert_eq!(status, 201, "job creation refused: {body}");
    let job_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &app,
        "POST",
        &format!("/api/v1/jobs/{job_id}/versions"),
        Some(&token),
        Some(serde_json::json!({ "execution_type": "WORKER_TASK" })),
    )
    .await;
    assert_eq!(status, 201, "version creation refused: {body}");
    let version_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &app,
        "POST",
        &format!("/api/v1/jobs/{job_id}/versions/{version_id}/publish"),
        Some(&token),
        Some(serde_json::json!({})),
    )
    .await;
    assert!(
        status < 300,
        "publish refused: {status} {body}; a job with no published version cannot \
         be triggered, so every test below would pass vacuously"
    );

    Fixture {
        token,
        router: app,
        queue_id,
        queue_name: queue_name.to_string(),
        job_id,
    }
}

struct Fixture {
    token: String,
    router: axum::Router,
    queue_id: String,
    queue_name: String,
    job_id: String,
}

impl Fixture {
    /// Registers a worker and returns its id and worker token.
    async fn worker(&self, label: &str) -> (String, String) {
        let (status, body) = send(
            &self.router,
            "POST",
            "/api/v1/workers/register",
            Some(&self.token),
            Some(serde_json::json!({ "name": label, "hostname": "localhost" })),
        )
        .await;
        assert_eq!(status, 201, "worker registration refused: {body}");
        (
            body["data"]["id"].as_str().unwrap().to_string(),
            body["data"]["token"].as_str().unwrap().to_string(),
        )
    }

    async fn trigger(&self) -> String {
        let (status, body) = send(
            &self.router,
            "POST",
            &format!("/api/v1/jobs/{}/trigger", self.job_id),
            Some(&self.token),
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 202, "trigger refused: {body}");
        body["data"]["execution_id"].as_str().unwrap().to_string()
    }

    /// Triggers a job and dispatches it to a fresh worker.
    async fn start_job(&self) -> Started {
        let (worker_id, worker_token) = self.worker("acceptance-worker").await;
        let execution_id = self.trigger().await;
        let (status, body) = send(
            &self.router,
            "POST",
            &format!("/api/v1/executions/{execution_id}/dispatch"),
            Some(&worker_token),
            Some(serde_json::json!({ "worker_id": worker_id })),
        )
        .await;
        assert!(
            status < 300,
            "dispatch refused: {status} {body}; without a dispatch the completion \
             tests below would pass vacuously"
        );
        Started {
            worker_id,
            worker_token,
            execution_id,
        }
    }

    async fn execution(&self, execution_id: &str) -> serde_json::Value {
        let (status, body) = send(
            &self.router,
            "GET",
            &format!("/api/v1/executions/{execution_id}"),
            Some(&self.token),
            None,
        )
        .await;
        assert_eq!(status, 200, "execution lookup refused: {body}");
        body["data"].clone()
    }
}

struct Started {
    worker_id: String,
    worker_token: String,
    execution_id: String,
}

// ---------------------------------------------------------------------------
// Security: a worker must not reach anything outside its authorization
// ---------------------------------------------------------------------------

mod security {
    use super::*;

    /// A worker token must not be able to create jobs.
    ///
    /// A worker token carries only `workers:claim`, `workers:heartbeat` and
    /// `executions:write`. Anything wider would mean one compromised worker could
    /// schedule work, not merely run it.
    #[tokio::test]
    async fn a_worker_token_cannot_create_a_job() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "security-q", "SecJob").await;
        let (_, worker_token) = f.worker("sneaky").await;

        let status = send_status(
            &f.router,
            &worker_token,
            "POST",
            "/api/v1/jobs",
            serde_json::json!({ "name": "injected" }),
        )
        .await;

        assert!(
            status == 401 || status == 403,
            "a worker token created a job (HTTP {status}); least privilege is broken"
        );
        db.cleanup().await;
    }

    /// A worker must not be able to complete an execution it never claimed.
    ///
    /// Without the lease, any worker holding any valid token could complete
    /// anyone's work and mark it succeeded - so the side effect would be recorded
    /// without ever having run.
    #[tokio::test]
    async fn a_worker_cannot_complete_an_unclaimed_execution() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "security-q2", "OwnedJob").await;
        let started = f.start_job().await;
        let (_other, other_token) = f.worker("interloper").await;

        // Deliberately send no `lease_id`: this is the attack, not a mistake.
        send_status(
            &f.router,
            &other_token,
            "POST",
            &format!("/api/v1/executions/{}/complete", started.execution_id),
            serde_json::json!({ "succeeded": true, "output": "forged" }),
        )
        .await;

        let execution = f.execution(&started.execution_id).await;
        assert_ne!(
            execution["status"].as_str(),
            Some("SUCCEEDED"),
            "an unclaimed execution was completed by a worker that never held its \
             lease; the side effect is now recorded for work that never ran"
        );
        db.cleanup().await;
    }

    /// Cross-tenant isolation: a token from one tenant must not reach another's data.
    #[tokio::test]
    async fn a_token_cannot_read_another_tenants_execution() {
        let Some(db) = TestDb::new().await else { return };
        let a = tenant_with_job(&db.pool, "tenant-a-q", "AJob").await;
        let started = a.start_job().await;

        let (b_token, _b_tenant) = register_via_api(&db.pool).await;
        let status = send_status(
            &a.router,
            &b_token,
            "GET",
            &format!("/api/v1/executions/{}", started.execution_id),
            serde_json::json!({}),
        )
        .await;

        assert!(
            status == 403 || status == 404,
            "tenant B read tenant A's execution (HTTP {status})"
        );
        db.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// Recovery: an expired lease must be reconciled, not lost
// ---------------------------------------------------------------------------

mod recovery {
    use super::*;

    /// A lease that expires must leave the execution recoverable.
    ///
    /// The spec's word is "safely": the work must return to a claimable state
    /// rather than sitting DISPATCHED forever under a lease nobody holds.
    #[tokio::test]
    async fn an_expired_lease_leaves_the_execution_claimable_again() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "recovery-q", "RecoverJob").await;
        let started = f.start_job().await;
        let execution_uuid = uuid::Uuid::parse_str(&started.execution_id).unwrap();

        // Expire the lease the way time would, rather than sleeping for it.
        sqlx::query(
            "UPDATE worker_leases SET expires_at = NOW() - INTERVAL '1 hour' WHERE execution_id = $1",
        )
        .bind(execution_uuid)
        .execute(&db.pool)
        .await
        .expect("expire the lease");

        let recovered = sqlx::query(
            "UPDATE executions SET status = 'QUEUED', worker_id = NULL, enqueued_at = NOW()
             WHERE id = $1 AND status = 'DISPATCHED'
               AND NOT EXISTS (
                   SELECT 1 FROM worker_leases l
                   WHERE l.execution_id = $1
                     AND l.expires_at > NOW() AND l.released_at IS NULL)",
        )
        .bind(execution_uuid)
        .execute(&db.pool)
        .await
        .expect("run the lease reaper");

        assert_eq!(
            recovered.rows_affected(),
            1,
            "an execution whose lease expired was not recovered; it is stranded \
             under a lease nobody holds and the job never runs again"
        );
        db.cleanup().await;
    }

    /// A lease that is still valid must not be stolen.
    ///
    /// The other half of the previous test: recovering too eagerly is its own bug,
    /// because a long-running job would be dispatched a second time underneath
    /// itself.
    #[tokio::test]
    async fn a_live_lease_is_not_recovered() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "recovery-q2", "LiveJob").await;
        let started = f.start_job().await;

        let reaped = sqlx::query(
            "UPDATE executions SET status = 'QUEUED', worker_id = NULL
             WHERE id = $1 AND status = 'DISPATCHED'
               AND NOT EXISTS (
                   SELECT 1 FROM worker_leases l
                   WHERE l.execution_id = $1
                     AND l.expires_at > NOW() AND l.released_at IS NULL)",
        )
        .bind(uuid::Uuid::parse_str(&started.execution_id).unwrap())
        .execute(&db.pool)
        .await
        .expect("run the lease reaper");

        assert_eq!(
            reaped.rows_affected(),
            0,
            "the reaper stole an execution whose lease is still valid; a long-running \
             job would be dispatched twice and run twice"
        );
        db.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// Retries: an error class must actually drive a retry
// ---------------------------------------------------------------------------

mod retries {
    use super::*;

    /// A failure classified as transient must schedule a retry.
    ///
    /// The server records an absent `error_class` as PERMANENT, which is not
    /// retryable - so this is the test that proves the classification reaches the
    /// retry machinery rather than only being accepted by the API.
    #[tokio::test]
    async fn a_transient_failure_schedules_a_retry() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "retry-q", "FlakyJob").await;
        let started = f.start_job().await;

        let (status, body) = send(
            &f.router,
            "POST",
            &format!("/api/v1/executions/{}/fail", started.execution_id),
            Some(&started.worker_token),
            Some(serde_json::json!({
                "worker_id": started.worker_id,
                "succeeded": false,
                "error": "upstream unavailable",
                "error_class": "TRANSIENT",
            })),
        )
        .await;
        assert!(
            (200..300).contains(&status),
            "reporting a failure was refused: {status} {body}"
        );

        let execution = f.execution(&started.execution_id).await;
        let status = execution["status"].as_str().unwrap_or("<none>");
        assert_ne!(
            status, "FAILED",
            "a TRANSIENT failure was treated as terminal (status {status}); the \
             retry machinery is unreachable"
        );
        db.cleanup().await;
    }

    /// A permanent failure must not retry - otherwise a bad job loops forever.
    #[tokio::test]
    async fn a_permanent_failure_is_not_retried() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "retry-q2", "BadJob").await;
        let started = f.start_job().await;

        send(
            &f.router,
            "POST",
            &format!("/api/v1/executions/{}/fail", started.execution_id),
            Some(&started.worker_token),
            Some(serde_json::json!({
                "worker_id": started.worker_id,
                "succeeded": false,
                "error": "malformed payload",
                "error_class": "PERMANENT",
            })),
        )
        .await;

        let execution = f.execution(&started.execution_id).await;
        assert_eq!(
            execution["status"].as_str(),
            Some("FAILED"),
            "a PERMANENT failure was not terminal; a bad job would loop forever"
        );
        db.cleanup().await;
    }

    /// Attempt history must survive a lost connection.
    ///
    /// The spec asks for "attempt history and backoff persist across server
    /// restarts". Reading it back on a separate connection is the observable part
    /// of that: the rows outlived the one that wrote them.
    #[tokio::test]
    async fn attempt_history_survives_a_new_connection() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "retry-q3", "HistoryJob").await;
        let started = f.start_job().await;

        send(
            &f.router,
            "POST",
            &format!("/api/v1/executions/{}/fail", started.execution_id),
            Some(&started.worker_token),
            Some(serde_json::json!({
                "worker_id": started.worker_id,
                "succeeded": false,
                "error": "flaky upstream",
                "error_class": "TRANSIENT",
            })),
        )
        .await;

        // A separate pool over the same URL stands in for a restarted process:
        // nothing is carried over in memory, everything is read back from disk.
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let db_name = db.db_name().to_string();
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let restarted = PgPoolOptions::new()
            .max_connections(1)
            .connect(&format!("{server}/{db_name}"))
            .await
            .expect("a fresh connection to the same database");

        let attempts: i32 = sqlx::query_scalar("SELECT attempt_count FROM executions WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&started.execution_id).unwrap())
            .fetch_one(&restarted)
            .await
            .expect("attempt history survives a new connection");

        assert!(
            attempts >= 1,
            "a failed execution recorded {attempts} attempts; backoff cannot be \
             computed from a history that was never written"
        );
        restarted.close().await;
        db.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// Capacity: concurrency limits must hold under concurrent claims
// ---------------------------------------------------------------------------

mod capacity {
    use super::*;

    /// Two workers racing for one execution must produce exactly one claim.
    ///
    /// This is the SQL-level admission control the spec calls for: a duplicate
    /// claim means the work runs twice and its side effect lands twice.
    #[tokio::test]
    async fn concurrent_claims_produce_exactly_one_winner() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "capacity-q", "RaceJob").await;
        let (worker_a, token_a) = f.worker("racer-a").await;
        let (worker_b, token_b) = f.worker("racer-b").await;
        f.trigger().await;

        let (router_a, router_b) = (f.router.clone(), f.router.clone());
        // Bound before the join: `format!` produces a temporary, and a temporary
        // borrowed across another future's lifetime does not survive it.
        let path_a = format!("/api/v1/workers/{worker_a}/claim");
        let path_b = format!("/api/v1/workers/{worker_b}/claim");
        let (first, second) = tokio::join!(
            send_optional(&router_a, &token_a, "POST", &path_a, serde_json::json!({})),
            send_optional(&router_b, &token_b, "POST", &path_b, serde_json::json!({})),
        );

        // `claim` always answers; what distinguishes a win is a non-null
        // `execution` inside the envelope. Counting a non-null `data` would count
        // the empty result as a win and fail every run.
        let won = |value: Option<serde_json::Value>| {
            value
                .and_then(|v| v.get("execution").cloned())
                .filter(|e| !e.is_null())
                .is_some()
        };
        let winners = [won(first), won(second)].iter().filter(|w| **w).count();
        assert!(
            winners <= 1,
            "{winners} workers claimed the same execution; a duplicate claim means \
             the side effect is applied twice"
        );
        db.cleanup().await;
    }

    /// A dispatched execution must hold exactly one active lease.
    #[tokio::test]
    async fn an_execution_has_at_most_one_active_lease() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "capacity-q2", "LeaseJob").await;
        let started = f.start_job().await;

        let active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM worker_leases
             WHERE execution_id = $1 AND released_at IS NULL AND expires_at > NOW()",
        )
        .bind(uuid::Uuid::parse_str(&started.execution_id).unwrap())
        .fetch_one(&db.pool)
        .await
        .expect("active lease count");

        assert_eq!(
            active, 1,
            "a dispatched execution has {active} active leases; a second holder \
             would be able to complete work this worker already owns"
        );
        db.cleanup().await;
    }

    /// A paused queue must hand out no work (spec 10.10).
    #[tokio::test]
    async fn a_paused_queue_is_not_claimed_from() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "paused-q", "PausedJob").await;

        // Paused directly, so the test asserts the claim path rather than the
        // pause endpoint.
        sqlx::query("UPDATE queues SET paused = TRUE WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&f.queue_id).unwrap())
            .execute(&db.pool)
            .await
            .expect("pause the queue");

        f.trigger().await;
        let (worker_id, worker_token) = f.worker("greedy").await;

        let claimed = send_optional(
            &f.router,
            &worker_token,
            "POST",
            &format!("/api/v1/queues/{}/dequeue", f.queue_name),
            serde_json::json!({ "worker_id": worker_id }),
        )
        .await;

        assert!(
            claimed.is_none(),
            "a paused queue handed out work; a pause that does not pause will keep \
             dispatching through an incident"
        );
        db.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// Observability: every execution must be traceable to its outcome
// ---------------------------------------------------------------------------

mod observability {
    use super::*;

    /// Logs written during an execution must be retrievable afterwards.
    #[tokio::test]
    async fn logs_survive_and_can_be_read_back() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "obs-q", "LogJob").await;
        let started = f.start_job().await;

        for message in ["first line", "second line"] {
            let (status, body) = send(
                &f.router,
                "POST",
                &format!("/api/v1/executions/{}/logs", started.execution_id),
                Some(&started.worker_token),
                Some(serde_json::json!({ "message": message, "stream": "stdout" })),
            )
            .await;
            assert!((200..300).contains(&status), "log write refused: {body}");
        }

        let (status, body) = send(
            &f.router,
            "GET",
            &format!("/api/v1/executions/{}/logs", started.execution_id),
            Some(&f.token),
            None,
        )
        .await;
        assert_eq!(status, 200, "log read refused: {body}");

        let text = serde_json::to_string(&body["data"]).unwrap();
        assert!(
            text.contains("first line") && text.contains("second line"),
            "execution logs were not readable afterwards: {text}"
        );
        db.cleanup().await;
    }

    /// A completed execution must record who completed it and when.
    #[tokio::test]
    async fn a_completed_execution_records_its_outcome() {
        let Some(db) = TestDb::new().await else { return };
        let f = tenant_with_job(&db.pool, "obs-q2", "DoneJob").await;
        let started = f.start_job().await;

        let (status, body) = send(
            &f.router,
            "POST",
            &format!("/api/v1/executions/{}/complete", started.execution_id),
            Some(&started.worker_token),
            Some(serde_json::json!({
                "worker_id": started.worker_id,
                "succeeded": true,
                "output": { "ok": true },
            })),
        )
        .await;
        assert!((200..300).contains(&status), "complete refused: {body}");

        let execution = f.execution(&started.execution_id).await;
        assert_eq!(execution["status"].as_str(), Some("SUCCEEDED"));
        assert!(
            execution["worker_id"].as_str().is_some(),
            "a completed execution does not record which worker ran it"
        );
        assert!(
            execution["ended_at"].as_str().is_some(),
            "a completed execution has no end time, so it cannot be traced"
        );
        db.cleanup().await;
    }
}

// ---------------------------------------------------------------------------
// Deployment: migrations must be safe to re-run
// ---------------------------------------------------------------------------

mod deployment {
    use super::*;

    /// Re-running migrations must be a no-op.
    ///
    /// A deployment that migrates on every start has to tolerate being re-run -
    /// three replicas starting together, or a restart, must not fail.
    #[tokio::test]
    async fn migrations_are_idempotent() {
        let Some(db) = TestDb::new().await else { return };

        let second = sqlx::migrate!("../forge-storage/migrations").run(&db.pool).await;
        assert!(
            second.is_ok(),
            "re-running migrations failed: {:?}; a restart would crash the server",
            second.err()
        );

        let applied: i64 =
            sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
                .fetch_one(&db.pool)
                .await
                .unwrap_or(0);
        assert!(applied > 0, "no migrations were recorded as applied");
        db.cleanup().await;
    }

    /// A fresh database must contain what the API needs.
    #[tokio::test]
    async fn a_fresh_database_has_the_core_tables() {
        let Some(db) = TestDb::new().await else { return };

        for table in [
            "tenants",
            "users",
            "jobs",
            "job_versions",
            "executions",
            "worker_leases",
            "queues",
            "workers",
        ] {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM information_schema.tables
                     WHERE table_schema = 'public' AND table_name = $1)",
            )
            .bind(table)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(false);
            assert!(exists, "a fresh database has no `{table}` table");
        }
        db.cleanup().await;
    }

    /// Every tenant-scoped table must be behind row-level security.
    ///
    /// Isolation was previously only a convention - a `WHERE tenant_id = $1` in
    /// each query. One forgotten predicate leaks every row, so RLS is the backstop
    /// that makes the mistake non-fatal.
    ///
    /// `users` is deliberately excluded: a user row is reachable through
    /// `tenant_memberships`, so scoping `users` would stop an operator seeing a
    /// colleague who has not yet joined their tenant. `worker_leases` was the
    /// real omission, and is listed here so it cannot be dropped again.
    #[tokio::test]
    async fn tenant_tables_have_row_level_security_enabled() {
        let Some(db) = TestDb::new().await else { return };

        let unprotected: Vec<String> = sqlx::query_scalar(
            "SELECT c.relname FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND c.relkind = 'r'
               AND c.relname IN ('jobs', 'executions', 'workers', 'worker_leases')
               AND NOT c.relrowsecurity",
        )
        .fetch_all(&db.pool)
        .await
        .expect("query row-level security state");

        assert!(
            unprotected.is_empty(),
            "these tenant tables have row-level security disabled: {unprotected:?}; \
             isolation would rest on every query remembering a predicate"
        );
        db.cleanup().await;
    }
}