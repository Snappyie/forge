//! Partial rerun: resume a settled run from the nodes that did not succeed.
//!
//! The property under test is what is *preserved*. A full rerun re-executes
//! successful work, which for a pipeline that charges a card or sends an email
//! means doing it twice.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use forge_api::create_router;
use http::Request;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "rerun-test-secret";
const PEPPER: &[u8] = b"rerun-test-pepper";

struct TestDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    async fn new() -> Option<Self> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));
        let db_name = format!("forge_rerun_{}", Uuid::new_v4().simple());

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
            eprintln!("skipping: cannot create database");
            return None;
        }
        admin.close().await;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&format!("{server}/{db_name}"))
            .await
            .ok()?;
        if sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
            .is_err()
        {
            eprintln!("skipping: migrations failed");
            return None;
        }

        Some(Self {
            pool,
            admin_url,
            db_name,
        })
    }

    async fn cleanup(self) {
        if let Ok(admin) = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            let _ = admin
                .execute(sqlx::AssertSqlSafe(format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
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
        self.pool.close().await;
    }
}

macro_rules! with_db {
    ($body:expr) => {
        async move {
            match TestDb::new().await {
                Some(db) => {
                    let db = Arc::new(db);
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

fn router(pool: PgPool) -> axum::Router {
    create_router(
        pool,
        SECRET,
        "http://localhost:3000/api/v1",
        true,
        1024 * 1024,
        4 * 1024 * 1024,
        PEPPER,
    )
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"));
    let payload = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let mut request = builder.body(payload).unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:4000".parse::<std::net::SocketAddr>().unwrap(),
    ));

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

struct Fixture {
    token: String,
    run_id: Uuid,
    version_id: Uuid,
}

/// A settled run: `extract` succeeded, `transform` failed, `load` never ran.
async fn fixture(pool: &PgPool) -> Fixture {
    let email = format!("rerun-{}@example.com", Uuid::new_v4().simple());
    let hash = forge_auth::hash_password("rerun-password-123").unwrap();
    let user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id")
            .bind(Uuid::new_v4())
            .bind(&email)
            .bind(&hash)
            .fetch_one(pool)
            .await
            .unwrap();

    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (id, name) VALUES ($1, 'Rerun') RETURNING id")
            .bind(Uuid::new_v4())
            .fetch_one(pool)
            .await
            .unwrap();

    sqlx::query(
        "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'OWNER')",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .unwrap();

    let workflow_id: Uuid =
        sqlx::query_scalar("INSERT INTO workflows (id, tenant_id, key, name) VALUES ($1, $2, 'etl', 'ETL') RETURNING id")
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap();

    let version_id: Uuid = sqlx::query_scalar(
        "INSERT INTO workflow_versions (id, tenant_id, workflow_id, version_number, published_at)
         VALUES ($1, $2, $3, 1, NOW()) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(workflow_id)
    .fetch_one(pool)
    .await
    .unwrap();

    // `workflow_edges` references nodes by *id*, so the node ids are kept.
    let mut node_ids = std::collections::HashMap::new();
    for key in ["extract", "transform", "load"] {
        let node_id: Uuid = sqlx::query_scalar(
            "INSERT INTO workflow_nodes
                 (id, tenant_id, workflow_version_id, node_key, node_type, name)
             VALUES ($1, $2, $3, $4, 'JOB', $5) RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(version_id)
        .bind(key)
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap();
        node_ids.insert(key, node_id);
    }
    for (from, to) in [("extract", "transform"), ("transform", "load")] {
        sqlx::query(
            "INSERT INTO workflow_edges
                 (id, tenant_id, workflow_version_id, from_node_id, to_node_id)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(version_id)
        .bind(node_ids[from])
        .bind(node_ids[to])
        .execute(pool)
        .await
        .unwrap();
    }

    // The run itself: an execution with no job, as workflow runs are.
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO executions
             (id, tenant_id, job_id, job_version_id, workflow_id, workflow_version_id,
              status, trigger_source, workflow_context, created_at, started_at, ended_at)
         VALUES ($1, $2, NULL, NULL, $3, $4, 'FAILED', 'MANUAL', '{}'::jsonb,
                 NOW(), NOW() - INTERVAL '10 minutes', NOW() - INTERVAL '5 minutes')
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(workflow_id)
    .bind(version_id)
    .fetch_one(pool)
    .await
    .unwrap();

    // One succeeded with an output, one failed, one never ran.
    for (key, state, output) in [
        ("extract", "COMPLETED", Some(r#"{"rows": 1200}"#)),
        ("transform", "FAILED", None),
        ("load", "PENDING", None),
    ] {
        sqlx::query(
            "INSERT INTO workflow_node_states
                 (id, tenant_id, workflow_execution_id, workflow_version_id, node_key,
                  node_type, state, output, attempt_count)
             VALUES ($1, $2, $3, $4, $5, 'JOB', $6, $7::jsonb, 1)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(run_id)
        .bind(version_id)
        .bind(key)
        .bind(state)
        .bind(output)
        .execute(pool)
        .await
        .unwrap();
    }

    Fixture {
        token: forge_auth::JwtService::new(SECRET.as_bytes())
            .issue(user_id, tenant_id, forge_auth::Role::Owner)
            .unwrap(),
        run_id,
        version_id,
    }
}

async fn node_state(pool: &PgPool, run_id: Uuid, key: &str) -> (String, Option<serde_json::Value>) {
    sqlx::query_as(
        "SELECT state, output FROM workflow_node_states
          WHERE workflow_execution_id = $1 AND node_key = $2",
    )
    .bind(run_id)
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn rerunning_resets_only_the_nodes_that_did_not_succeed() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workflows/executions/{}/rerun", f.run_id),
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 200, "{body}");

        let reset: Vec<&str> = body["data"]["reset_nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let preserved: Vec<&str> = body["data"]["preserved_nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();

        let mut sorted_reset = reset.clone();
        sorted_reset.sort();
        assert_eq!(sorted_reset, vec!["load", "transform"], "{body}");
        assert_eq!(preserved, vec!["extract"], "{body}");

        // The successful node keeps its state *and* its output. Redoing it is
        // exactly what this feature exists to avoid.
        let (state, output) = node_state(&pool, f.run_id, "extract").await;
        assert_eq!(state, "COMPLETED");
        assert_eq!(output, Some(serde_json::json!({"rows": 1200})));

        // The failed node is back to pending with its failure cleared.
        let (state, _) = node_state(&pool, f.run_id, "transform").await;
        assert_eq!(state, "PENDING");

        // The never-run node is too: a settled run leaves PENDING downstream of a
        // failure, and that work has not been done.
        let (state, _) = node_state(&pool, f.run_id, "load").await;
        assert_eq!(state, "PENDING");

        // And the run is live again for the driver to advance.
        let status_now: String =
            sqlx::query_scalar("SELECT status FROM executions WHERE id = $1")
                .bind(f.run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status_now, "RUNNING");
    })
    .await;
}

#[tokio::test]
async fn rerunning_a_fully_successful_run_is_refused_by_default() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        // Mark the run successful with every node done.
        sqlx::query("UPDATE executions SET status = 'SUCCEEDED' WHERE id = $1")
            .bind(f.run_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE workflow_node_states SET state = 'COMPLETED' WHERE workflow_execution_id = $1")
            .bind(f.run_id)
            .execute(&pool)
            .await
            .unwrap();

        let app = router(pool.clone());
        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workflows/executions/{}/rerun", f.run_id),
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        // Redoing successful work is the destructive direction, so it has to be
        // asked for explicitly.
        assert_eq!(status, 400, "{body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("include_completed"),
            "the error must say how to proceed: {body}"
        );
    })
    .await;
}

#[tokio::test]
async fn include_completed_redoes_successful_work_when_asked() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workflows/executions/{}/rerun", f.run_id),
            &f.token,
            Some(serde_json::json!({ "include_completed": true })),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert!(
            body["data"]["preserved_nodes"].as_array().unwrap().is_empty(),
            "nothing is preserved when every node is redone: {body}"
        );

        // A reset node's output is dropped, so a downstream condition cannot
        // branch on the previous attempt's result.
        let (_, output) = node_state(&pool, f.run_id, "extract").await;
        assert_eq!(output, None, "a reset node must not keep a stale output");
    })
    .await;
}

#[tokio::test]
async fn a_named_node_that_the_run_does_not_have_is_refused() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workflows/executions/{}/rerun", f.run_id),
            &f.token,
            Some(serde_json::json!({ "nodes": ["does-not-exist"] })),
        )
        .await;
        assert_eq!(status, 400, "{body}");
    })
    .await;
}

#[tokio::test]
async fn a_run_that_is_still_running_cannot_be_rerun() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        sqlx::query("UPDATE executions SET status = 'RUNNING' WHERE id = $1")
            .bind(f.run_id)
            .execute(&pool)
            .await
            .unwrap();

        let app = router(pool.clone());
        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workflows/executions/{}/rerun", f.run_id),
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        // Rerunning a live run would race the driver already advancing it.
        assert_eq!(status, 409, "{body}");
    })
    .await;
}

#[tokio::test]
async fn the_preview_shows_exactly_what_a_rerun_would_reset() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "GET",
            &format!("/api/v1/workflows/executions/{}/rerun-preview", f.run_id),
            &f.token,
            None,
        )
        .await;
        assert_eq!(status, 200, "{body}");

        let nodes = body["data"]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 3, "{body}");
        let would_rerun: Vec<&str> = nodes
            .iter()
            .filter(|n| n["would_rerun"] == serde_json::Value::Bool(true))
            .map(|n| n["node_key"].as_str().unwrap())
            .collect();
        assert_eq!(would_rerun, vec!["load", "transform"], "{body}");
    })
    .await;
}

#[tokio::test]
async fn another_tenants_run_is_not_found() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;

        let other_hash = forge_auth::hash_password("other-password-12").unwrap();
        let other_user: Uuid = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(format!("other-{}@example.com", Uuid::new_v4().simple()))
        .bind(&other_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        let other_tenant: Uuid =
            sqlx::query_scalar("INSERT INTO tenants (id, name) VALUES ($1, 'Other') RETURNING id")
                .bind(Uuid::new_v4())
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'OWNER')",
        )
        .bind(other_user)
        .bind(other_tenant)
        .execute(&pool)
        .await
        .unwrap();
        let other_token = forge_auth::JwtService::new(SECRET.as_bytes())
            .issue(other_user, other_tenant, forge_auth::Role::Owner)
            .unwrap();

        let app = router(pool.clone());
        let (status, body) = send(
            &app,
            "POST",
            &format!("/api/v1/workflows/executions/{}/rerun", f.run_id),
            &other_token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 404, "{body}");
    })
    .await;
}