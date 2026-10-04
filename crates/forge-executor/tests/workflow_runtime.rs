//! End-to-end tests for the workflow runtime.
//!
//! These are the tests that would have caught the original defect: the DAG
//! engine existed, was unit-tested, and was never reachable. Everything here
//! goes through the database — start a run, drive it, observe child executions —
//! so it fails if the wiring regresses even when the engine itself still passes
//! its own tests.
//!
//! A live PostgreSQL is required; without `DATABASE_URL` (or the documented
//! local default) the suite skips rather than failing, matching the other
//! integration suites.

use std::sync::Arc;

use forge_domain::{ErrorClass, TenantId};
use forge_executor::WorkflowDriver;
use forge_storage::{
    ExecutionRepository, NewWorkflowRun, NodeStateRow, WorkflowRunRepository as RunRepository,
};
use serde_json::{json, Value};
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
        let db_name = format!("forge_wf_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
            .ok()?;

        if admin
            .execute(sqlx::AssertSqlSafe(format!(
                r#"CREATE DATABASE "{db_name}""#
            )))
            .await
            .is_err()
        {
            eprintln!("skipping workflow runtime tests: cannot create database");
            return None;
        }
        admin.close().await;

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&format!("{server}/{db_name}"))
            .await
            .ok()?;

        if let Err(error) = sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
        {
            panic!("migrations must apply for the workflow runtime tests: {error}");
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
                    $body(db.pool.clone()).await;
                    match Arc::try_unwrap(db) {
                        Ok(db) => db.cleanup().await,
                        Err(_) => eprintln!("warning: test db handle still shared; not dropped"),
                    }
                }
                None => eprintln!("skipping: no database available"),
            }
        }
    };
}

async fn seed_tenant(pool: &PgPool) -> TenantId {
    let tenant = TenantId::new();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant.into_uuid())
        .bind("Workflow Test Tenant")
        .execute(pool)
        .await
        .expect("seed tenant");
    tenant
}

/// Creates a job with one published version, or with no published version when
/// `published` is false.
async fn seed_job(pool: &PgPool, tenant: TenantId, published: bool) -> Uuid {
    let job_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO jobs (id, tenant_id, key, name, status) VALUES ($1, $2, $3, $4, 'ACTIVE')",
    )
    .bind(job_id)
    .bind(tenant.into_uuid())
    .bind(format!("job-{job_id}"))
    .bind("Workflow node job")
    .execute(pool)
    .await
    .expect("seed job");

    sqlx::query(
        "INSERT INTO job_versions
             (id, tenant_id, job_id, version_number, execution_type, timeout_seconds, published_at)
         VALUES ($1, $2, $3, 1, 'WORKER_TASK', 3600, $4)",
    )
    .bind(version_id)
    .bind(tenant.into_uuid())
    .bind(job_id)
    .bind(if published {
        Some(chrono::Utc::now())
    } else {
        None
    })
    .execute(pool)
    .await
    .expect("seed job version");

    if published {
        sqlx::query("UPDATE jobs SET current_version_id = $2 WHERE id = $1")
            .bind(job_id)
            .bind(version_id)
            .execute(pool)
            .await
            .expect("point job at its published version");
    }

    job_id
}

struct WorkflowFixture {
    workflow_id: Uuid,
    version_id: Uuid,
}

/// Creates a published workflow whose nodes are `(key, type, config)`.
async fn seed_workflow(
    pool: &PgPool,
    tenant: TenantId,
    nodes: &[(&str, &str, Value)],
    edges: &[(&str, &str, Option<&str>)],
    timeout_seconds: i32,
) -> WorkflowFixture {
    let workflow_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO workflows (id, tenant_id, key, name, status)
         VALUES ($1, $2, $3, $4, 'ACTIVE')",
    )
    .bind(workflow_id)
    .bind(tenant.into_uuid())
    .bind(format!("wf-{workflow_id}"))
    .bind("Test workflow")
    .execute(pool)
    .await
    .expect("seed workflow");

    sqlx::query(
        "INSERT INTO workflow_versions
             (id, tenant_id, workflow_id, version_number, timeout_seconds, published_at)
         VALUES ($1, $2, $3, 1, $4, NOW())",
    )
    .bind(version_id)
    .bind(tenant.into_uuid())
    .bind(workflow_id)
    .bind(timeout_seconds)
    .execute(pool)
    .await
    .expect("seed workflow version");

    sqlx::query("UPDATE workflows SET current_version_id = $2 WHERE id = $1")
        .bind(workflow_id)
        .bind(version_id)
        .execute(pool)
        .await
        .expect("publish workflow");

    let mut node_ids = std::collections::HashMap::new();
    for (key, kind, config) in nodes {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workflow_nodes
                 (id, tenant_id, workflow_version_id, node_key, node_type, name, config)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(tenant.into_uuid())
        .bind(version_id)
        .bind(*key)
        .bind(*kind)
        .bind(*key)
        .bind(config)
        .execute(pool)
        .await
        .expect("seed workflow node");
        node_ids.insert(key.to_string(), id);
    }

    for (from, to, condition) in edges {
        sqlx::query(
            "INSERT INTO workflow_edges
                 (id, tenant_id, workflow_version_id, from_node_id, to_node_id, condition)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant.into_uuid())
        .bind(version_id)
        .bind(node_ids[*from])
        .bind(node_ids[*to])
        .bind(condition.unwrap_or("ALL_SUCCEEDED"))
        .execute(pool)
        .await
        .expect("seed workflow edge");
    }

    WorkflowFixture {
        workflow_id,
        version_id,
    }
}

async fn start_run(
    pool: &PgPool,
    tenant: TenantId,
    fixture: &WorkflowFixture,
    input: Value,
) -> Uuid {
    RunRepository::new(pool)
        .start_run(NewWorkflowRun {
            tenant_id: tenant,
            workflow_id: fixture.workflow_id,
            workflow_version_id: fixture.version_id,
            trigger_source: "MANUAL".to_string(),
            correlation_id: Some("test-correlation".into()),
            input,
            timeout_seconds: 3600,
        })
        .await
        .expect("start run")
}

async fn run_status(pool: &PgPool, run_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM executions WHERE id = $1")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .expect("read run status")
}

async fn node_state(pool: &PgPool, run_id: Uuid, node_key: &str) -> NodeStateRow {
    RunRepository::new(pool)
        .node_states(run_id)
        .await
        .expect("read node states")
        .into_iter()
        .find(|row| row.node_key == node_key)
        .unwrap_or_else(|| panic!("node {node_key} missing"))
}

async fn children_of(pool: &PgPool, run_id: Uuid) -> Vec<(Uuid, String)> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, status FROM executions WHERE parent_execution_id = $1 ORDER BY created_at",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .expect("read children")
}

/// Forces a child execution to a terminal state, standing in for a worker.
async fn settle_child(pool: &PgPool, child_id: Uuid, status: &str) {
    sqlx::query("UPDATE executions SET status = $2, ended_at = NOW() WHERE id = $1")
        .bind(child_id)
        .bind(status)
        .execute(pool)
        .await
        .expect("settle child");
}

fn driver(pool: &PgPool) -> WorkflowDriver<'_> {
    // `stale_secs = 0` so a run started in the test is immediately claimable.
    WorkflowDriver::new(pool)
}

async fn tick(pool: &PgPool, now: chrono::DateTime<chrono::Utc>) {
    let report = driver(pool).tick(now, 50, 0.0).await;
    assert!(
        report.errors.is_empty(),
        "the driver reported errors: {:?}",
        report.errors
    );
}

#[tokio::test]
async fn a_linear_workflow_dispatches_its_job_and_settles() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let job = seed_job(&pool, tenant, true).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("build", "JOB", json!({ "job_id": job.to_string() }))],
            &[],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        assert_eq!(run_status(&pool, run_id).await, "RUNNING");

        // First pass: the job node is dispatched as a child execution.
        tick(&pool, chrono::Utc::now()).await;
        let children = children_of(&pool, run_id).await;
        assert_eq!(children.len(), 1, "one job node dispatches one child");
        assert_eq!(children[0].1, "QUEUED");
        assert_eq!(node_state(&pool, run_id, "build").await.state, "RUNNING");

        // The run cannot finish while its child is outstanding.
        assert_eq!(run_status(&pool, run_id).await, "RUNNING");

        settle_child(&pool, children[0].0, "SUCCEEDED").await;
        tick(&pool, chrono::Utc::now()).await;

        assert_eq!(node_state(&pool, run_id, "build").await.state, "COMPLETED");
        assert_eq!(run_status(&pool, run_id).await, "SUCCEEDED");
    })
    .await;
}

#[tokio::test]
async fn a_failed_child_fails_the_run() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let job = seed_job(&pool, tenant, true).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("build", "JOB", json!({ "job_id": job.to_string() }))],
            &[],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, chrono::Utc::now()).await;

        let children = children_of(&pool, run_id).await;
        settle_child(&pool, children[0].0, "FAILED").await;
        tick(&pool, chrono::Utc::now()).await;

        let node = node_state(&pool, run_id, "build").await;
        assert_eq!(node.state, "FAILED");
        assert!(node.failure_reason.is_some(), "a failure keeps its reason");
        assert_eq!(run_status(&pool, run_id).await, "FAILED");
    })
    .await;
}

#[tokio::test]
async fn an_approval_node_waits_for_a_decision() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("sign_off", "APPROVAL", json!({ "required_role": "ADMIN" }))],
            &[],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, chrono::Utc::now()).await;

        let node = node_state(&pool, run_id, "sign_off").await;
        assert_eq!(node.state, "SUSPENDED");
        assert_eq!(node.suspension_reason.as_deref(), Some("AWAITING_APPROVAL"));
        assert_eq!(node.required_role.as_deref(), Some("ADMIN"));
        assert_eq!(
            run_status(&pool, run_id).await,
            "RUNNING",
            "an approval must not let the run finish"
        );

        // A pending request exists for the human to act on.
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM manual_approvals
             WHERE workflow_execution_id = $1 AND status = 'PENDING'",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1);

        // The decision is recorded out of band, exactly as the endpoint does.
        sqlx::query(
            "UPDATE manual_approvals SET status = 'APPROVED', decided_at = NOW()
             WHERE workflow_execution_id = $1",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();

        tick(&pool, chrono::Utc::now()).await;

        assert_eq!(
            node_state(&pool, run_id, "sign_off").await.state,
            "COMPLETED"
        );
        assert_eq!(run_status(&pool, run_id).await, "SUCCEEDED");
    })
    .await;
}

#[tokio::test]
async fn a_rejected_approval_fails_the_run() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("sign_off", "APPROVAL", json!({ "required_role": "ADMIN" }))],
            &[],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, chrono::Utc::now()).await;

        sqlx::query(
            "UPDATE manual_approvals SET status = 'REJECTED', decided_at = NOW()
             WHERE workflow_execution_id = $1",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();

        tick(&pool, chrono::Utc::now()).await;

        assert_eq!(node_state(&pool, run_id, "sign_off").await.state, "FAILED");
        assert_eq!(run_status(&pool, run_id).await, "FAILED");
    })
    .await;
}

#[tokio::test]
async fn a_delay_node_resumes_once_its_deadline_passes() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("pause", "DELAY", json!({ "seconds": 60 }))],
            &[],
            3600,
        )
        .await;

        let start = chrono::Utc::now();
        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, start).await;

        let node = node_state(&pool, run_id, "pause").await;
        assert_eq!(node.state, "SUSPENDED");
        assert_eq!(node.suspension_reason.as_deref(), Some("DELAY"));
        assert!(node.resume_at.is_some());

        // Not yet: the deadline has not arrived.
        tick(&pool, start).await;
        assert_eq!(node_state(&pool, run_id, "pause").await.state, "SUSPENDED");

        // Once it has, the node completes and the run settles.
        tick(&pool, start + chrono::Duration::seconds(120)).await;
        assert_eq!(node_state(&pool, run_id, "pause").await.state, "COMPLETED");
        assert_eq!(run_status(&pool, run_id).await, "SUCCEEDED");
    })
    .await;
}

#[tokio::test]
async fn a_conditional_node_runs_only_the_matching_branch() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let yes_job = seed_job(&pool, tenant, true).await;
        let no_job = seed_job(&pool, tenant, true).await;

        let fixture = seed_workflow(
            &pool,
            tenant,
            &[
                (
                    "decide",
                    "CONDITION",
                    json!({ "expression": "flag == true" }),
                ),
                ("yes", "JOB", json!({ "job_id": yes_job.to_string() })),
                ("no", "JOB", json!({ "job_id": no_job.to_string() })),
            ],
            &[
                ("decide", "yes", Some("true")),
                ("decide", "no", Some("false")),
            ],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({ "flag": true })).await;
        tick(&pool, chrono::Utc::now()).await;

        let children = children_of(&pool, run_id).await;
        assert_eq!(
            children.len(),
            1,
            "only the true branch may dispatch, got {children:?}"
        );

        // The false branch is unreachable and stays pending, so the run never
        // settles on its own; the branch that did run decides the outcome.
        assert_eq!(node_state(&pool, run_id, "decide").await.state, "COMPLETED");
        assert_eq!(node_state(&pool, run_id, "yes").await.state, "RUNNING");
    })
    .await;
}

#[tokio::test]
async fn a_map_node_fans_out_once_per_item_without_dispatching_its_target() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let job = seed_job(&pool, tenant, true).await;

        let fixture = seed_workflow(
            &pool,
            tenant,
            &[
                (
                    "spread",
                    "MAP",
                    json!({ "target_node_id": "each", "job_id": job.to_string() }),
                ),
                ("each", "JOB", json!({ "job_id": job.to_string() })),
            ],
            &[("spread", "each", None)],
            3600,
        )
        .await;

        let run_id = start_run(
            &pool,
            tenant,
            &fixture,
            json!({ "map_items": ["a", "b", "c"] }),
        )
        .await;
        tick(&pool, chrono::Utc::now()).await;

        let children = children_of(&pool, run_id).await;
        assert_eq!(children.len(), 3, "one child per mapped item");

        let spread = node_state(&pool, run_id, "spread").await;
        assert_eq!(
            spread.suspension_reason.as_deref(),
            Some("AWAITING_FAN_OUT")
        );

        // The target must not be dispatched a second time by the ordinary path.
        assert_ne!(
            node_state(&pool, run_id, "each").await.state,
            "PENDING",
            "the target is owned by the fan-out"
        );

        for (child_id, _) in &children {
            settle_child(&pool, *child_id, "SUCCEEDED").await;
        }
        tick(&pool, chrono::Utc::now()).await;

        assert_eq!(node_state(&pool, run_id, "spread").await.state, "COMPLETED");
        assert_eq!(node_state(&pool, run_id, "each").await.state, "COMPLETED");
        assert_eq!(
            children_of(&pool, run_id).await.len(),
            3,
            "settling the fan-out must not dispatch more work"
        );
        assert_eq!(run_status(&pool, run_id).await, "SUCCEEDED");
    })
    .await;
}

#[tokio::test]
async fn a_job_node_without_a_published_version_fails_with_a_reason() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let job = seed_job(&pool, tenant, false).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("build", "JOB", json!({ "job_id": job.to_string() }))],
            &[],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, chrono::Utc::now()).await;

        let node = node_state(&pool, run_id, "build").await;
        assert_eq!(node.state, "FAILED");
        assert!(
            node.failure_reason
                .as_deref()
                .unwrap_or_default()
                .contains("published version"),
            "the reason must say what is wrong: {:?}",
            node.failure_reason
        );
        assert_eq!(run_status(&pool, run_id).await, "FAILED");
    })
    .await;
}

#[tokio::test]
async fn cancelling_a_run_waits_for_its_children() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let job = seed_job(&pool, tenant, true).await;
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("build", "JOB", json!({ "job_id": job.to_string() }))],
            &[],
            3600,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, chrono::Utc::now()).await;

        let cancelled = RunRepository::new(&pool)
            .cancel_run(tenant, run_id)
            .await
            .expect("cancel");
        assert!(cancelled);
        assert_eq!(run_status(&pool, run_id).await, "CANCEL_REQUESTED");

        // The child was asked to stop.
        let children = children_of(&pool, run_id).await;
        assert_eq!(children[0].1, "CANCEL_REQUESTED");

        // The run only settles once the child has actually stopped.
        tick(&pool, chrono::Utc::now()).await;
        assert_eq!(run_status(&pool, run_id).await, "CANCEL_REQUESTED");

        settle_child(&pool, children[0].0, "CANCELLED").await;
        tick(&pool, chrono::Utc::now()).await;
        assert_eq!(run_status(&pool, run_id).await, "CANCELLED");
    })
    .await;
}

#[tokio::test]
async fn a_run_that_outlives_its_timeout_is_timed_out() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        // An approval node never completes on its own, so the only way out is
        // the workflow-level timeout.
        let fixture = seed_workflow(
            &pool,
            tenant,
            &[("sign_off", "APPROVAL", json!({ "required_role": "ADMIN" }))],
            &[],
            0,
        )
        .await;

        let run_id = start_run(&pool, tenant, &fixture, json!({})).await;
        tick(&pool, chrono::Utc::now()).await;

        assert_eq!(run_status(&pool, run_id).await, "TIMED_OUT");
        let node = node_state(&pool, run_id, "sign_off").await;
        assert_eq!(node.state, "FAILED");
        assert!(node
            .failure_reason
            .as_deref()
            .unwrap_or_default()
            .contains("timeout"));
    })
    .await;
}

/// The retry policy is now load-bearing: a failing execution with a retry
/// budget is rescheduled with its backoff instead of stopping at FAILED.
#[tokio::test]
async fn a_failure_with_retry_budget_is_rescheduled_with_its_backoff() {
    with_db!(|pool: PgPool| async move {
        let tenant = seed_tenant(&pool).await;
        let job = seed_job(&pool, tenant, true).await;

        // Move the execution to RUNNING so the failure is a legal transition.
        let execution_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO executions
                 (id, tenant_id, job_id, job_version_id, status, trigger_source, attempt_count,
                  started_at)
             VALUES ($1, $2, $3, (SELECT current_version_id FROM jobs WHERE id = $3),
                     'RUNNING', 'MANUAL', 0, NOW())",
        )
        .bind(execution_id)
        .bind(tenant.into_uuid())
        .bind(job)
        .execute(&pool)
        .await
        .expect("seed execution");

        let handler = forge_executor::FailureHandler::new(&pool);
        let outcome = handler
            .record_failure(
                tenant,
                execution_id,
                ErrorClass::Transient,
                "network blip",
                1,
            )
            .await
            .expect("record failure");

        match outcome {
            forge_executor::FailureOutcome::RetryScheduled { attempt, .. } => {
                assert_eq!(attempt, 1);
            }
            other => panic!("expected a scheduled retry, got {other:?}"),
        }

        let row = ExecutionRepository::new(&pool)
            .get(tenant, execution_id)
            .await
            .expect("read execution");
        assert_eq!(row.status, "RETRY_SCHEDULED");
        assert!(row.retry_at.is_some(), "the backoff instant is recorded");

        // The retry becomes claimable only once its delay elapses.
        assert!(
            ExecutionRepository::new(&pool)
                .due_retries(10)
                .await
                .expect("due retries")
                .is_empty(),
            "a retry that is not due yet must not be requeued"
        );

        // A permanent error exhausts nothing: it simply is not retried.
        let other = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO executions
                 (id, tenant_id, job_id, job_version_id, status, trigger_source, started_at)
             VALUES ($1, $2, $3, (SELECT current_version_id FROM jobs WHERE id = $3),
                     'RUNNING', 'MANUAL', NOW())",
        )
        .bind(other)
        .bind(tenant.into_uuid())
        .bind(job)
        .execute(&pool)
        .await
        .expect("seed second execution");

        let outcome = handler
            .record_failure(tenant, other, ErrorClass::Permanent, "bad input", 1)
            .await
            .expect("record permanent failure");
        assert!(matches!(
            outcome,
            forge_executor::FailureOutcome::Failed { .. }
        ));
        assert_eq!(
            ExecutionRepository::new(&pool)
                .get(tenant, other)
                .await
                .expect("read execution")
                .status,
            "FAILED"
        );
    })
    .await;
}
