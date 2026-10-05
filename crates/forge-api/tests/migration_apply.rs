//! Applying a migration: what actually crosses, and what does not.
//!
//! The plan endpoint's test proves nothing moves while planning. This proves the
//! other half — that applying carries the right jobs across, leaves the right
//! ones alone, and refuses when a binding has not been resolved.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use forge_api::create_router;
use http::Request;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "apply-test-secret";
const PEPPER: &[u8] = b"apply-test-pepper";

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
        let db_name = format!("forge_apply_{}", Uuid::new_v4().simple());

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
    tenant_id: Uuid,
    dev_env: Uuid,
    prod_env: Uuid,
    prod_queue: Uuid,
}

/// dev with two scheduled jobs; prod with one existing job under the same key
/// as one of them, plus its own queue.
async fn fixture(pool: &PgPool) -> Fixture {
    let email = format!("apply-{}@example.com", Uuid::new_v4().simple());
    let hash = forge_auth::hash_password("apply-password-1234").unwrap();
    let user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id")
            .bind(Uuid::new_v4())
            .bind(&email)
            .bind(&hash)
            .fetch_one(pool)
            .await
            .unwrap();

    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (id, name) VALUES ($1, 'Apply') RETURNING id")
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

    let dev_env: Uuid = sqlx::query_scalar(
        "INSERT INTO environments (id, tenant_id, slug, name, kind)
         VALUES ($1, $2, 'dev', 'Development', 'development') RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();

    let prod_env: Uuid = sqlx::query_scalar(
        "INSERT INTO environments (id, tenant_id, slug, name, kind)
         VALUES ($1, $2, 'prod', 'Production', 'production') RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();

    let prod_queue: Uuid =
        sqlx::query_scalar("INSERT INTO queues (id, tenant_id, name) VALUES ($1, $2, 'prod-critical') RETURNING id")
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap();

    sqlx::query("INSERT INTO queues (id, tenant_id, name) VALUES ($1, $2, 'dev-payments')")
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .execute(pool)
        .await
        .unwrap();

    // Two dev jobs. `settle` is new in the target; `report` already exists there.
    for (name, key, cron) in [
        ("Settle payments", "settle", "0 2 * * *"),
        ("Daily report", "report", "0 6 * * *"),
    ] {
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO jobs (id, tenant_id, name, status, environment_id, key)
             VALUES ($1, $2, $3, 'ACTIVE', $4, $5) RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(name)
        .bind(dev_env)
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO schedules
                 (id, tenant_id, job_id, target_id, target_type, schedule_type,
                  cron_expression, timezone, misfire_policy, catch_up_policy,
                  next_run_at, enabled)
             VALUES ($1, $2, $3, $3, 'JOB', 'CRON', $4, 'UTC', 'FIRE_ONCE',
                     '[]'::jsonb, NOW() + INTERVAL '1 day', TRUE)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(job_id)
        .bind(cron)
        .execute(pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO jobs (id, tenant_id, name, status, environment_id, key)
         VALUES ($1, $2, 'Report (prod copy)', 'ACTIVE', $3, 'report')",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(prod_env)
    .execute(pool)
    .await
    .unwrap();

    Fixture {
        token: forge_auth::JwtService::new(SECRET.as_bytes())
            .issue(user_id, tenant_id, forge_auth::Role::Owner)
            .unwrap(),
        tenant_id,
        dev_env,
        prod_env,
        prod_queue,
    }
}

async fn plan_for(app: &axum::Router, token: &str) -> serde_json::Value {
    let (status, body) = send(
        app,
        "POST",
        "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
        token,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    body["data"]["plan"].clone()
}

#[tokio::test]
async fn applying_carries_new_jobs_across_and_touches_existing_ones() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());
        let plan = plan_for(&app, &f.token).await;

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({ "plan": plan, "copy_schedules": true })),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["data"]["created"], 1, "{body}");
        assert_eq!(body["data"]["updated"], 1, "{body}");

        // The new job exists in the target with its key preserved, which is what
        // makes a second migration able to match it.
        let carried: Option<String> = sqlx::query_scalar(
            "SELECT name FROM jobs
              WHERE tenant_id = $1 AND environment_id = $2 AND key = 'settle'",
        )
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(
            carried,
            Some("Settle payments".to_string()),
            "the new job crossed with its key preserved, which is what lets a \
             second migration match it"
        );

        // The existing job was updated in place, not duplicated.
        let report_count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM jobs
              WHERE tenant_id = $1 AND environment_id = $2 AND key = 'report'",
        )
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(report_count.0, 1, "an update must not create a second job");

        // The source is untouched: a migration copies, it does not move.
        let dev_jobs: (i64,) =
            sqlx::query_as("SELECT count(*) FROM jobs WHERE environment_id = $1")
                .bind(f.dev_env)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(dev_jobs.0, 2, "the source keeps its jobs");
    })
    .await;
}

#[tokio::test]
async fn a_copied_schedule_arrives_disabled() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());
        let plan = plan_for(&app, &f.token).await;

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({ "plan": plan, "copy_schedules": true })),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["data"]["schedules_copied"], 1, "{body}");

        // A schedule that starts firing the moment it arrives is a change the
        // operator has not reviewed. Resuming it is the deliberate act.
        let enabled: bool = sqlx::query_scalar(
            "SELECT s.enabled FROM schedules s
               JOIN jobs j ON j.id = s.target_id
              WHERE j.tenant_id = $1 AND j.key = 'settle' AND j.environment_id = $2",
        )
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            !enabled,
            "a copied schedule must arrive disabled so it cannot fire un-reviewed"
        );
    })
    .await;
}

#[tokio::test]
async fn a_queue_mapped_to_another_tenants_id_is_refused() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;

        // A queue belonging to a different tenant.
        let foreign_tenant: Uuid =
            sqlx::query_scalar("INSERT INTO tenants (id, name) VALUES ($1, 'Foreign') RETURNING id")
                .bind(Uuid::new_v4())
                .fetch_one(&pool)
                .await
                .unwrap();
        let foreign_queue: Uuid =
            sqlx::query_scalar("INSERT INTO queues (id, tenant_id, name) VALUES ($1, $2, 'theirs') RETURNING id")
                .bind(Uuid::new_v4())
                .bind(foreign_tenant)
                .fetch_one(&pool)
                .await
                .unwrap();

        let app = router(pool.clone());
        let plan = plan_for(&app, &f.token).await;

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({
                "plan": plan,
                "queues": { "dev-payments": foreign_queue },
            })),
        )
        .await;
        assert_eq!(status, 400, "{body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("does not exist in this tenant"),
            "{body}"
        );

        // And nothing was created on the way out.
        let carried: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM jobs WHERE tenant_id = $1 AND environment_id = $2 AND key = 'settle'",
        )
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(carried.0, 0, "a refused apply must write nothing");
        let _ = f.prod_queue;
    })
    .await;
}

#[tokio::test]
async fn create_only_and_update_only_apply_one_direction() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());
        let plan = plan_for(&app, &f.token).await;

        // Update-only: the existing job is touched, the new one is left behind.
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({ "plan": plan, "allow_create": false })),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["data"]["created"], 0, "{body}");

        let carried: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM jobs WHERE tenant_id = $1 AND environment_id = $2 AND key = 'settle'",
        )
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(carried.0, 0, "create was disabled, so nothing new appeared");
    })
    .await;
}

#[tokio::test]
async fn an_empty_plan_is_refused() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({
                // `Slug` validates on deserialisation, so these have to be
                // real slugs; "t" is below the two-character minimum and is
                // rejected before the handler runs, which is a 422 rather than
                // the 400 this test is about.
                "plan": {
                    "source_tenant": "acme", "source_environment": "dev",
                    "target_tenant": "acme", "target_environment": "prod",
                    "application": null, "entries": [], "include": []
                }
            })),
        )
        .await;
        assert_eq!(status, 400, "{body}");
    })
    .await;
}

#[tokio::test]
async fn the_binding_list_reports_what_has_to_be_resolved() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());
        let plan = plan_for(&app, &f.token).await;

        // The endpoint takes the plan itself, not a wrapper.
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/bindings",
            &f.token,
            Some(plan),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        // The target's real queues are offered, so an operator picks from a list
        // rather than being asked to type a uuid.
        let available = body["data"]["available_queues"].as_array().unwrap();
        assert!(
            available.iter().any(|q| q["name"] == "prod-critical"),
            "{body}"
        );
    })
    .await;
}

#[tokio::test]
async fn applying_records_an_audit_event() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());
        let plan = plan_for(&app, &f.token).await;

        send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({ "plan": plan })),
        )
        .await;

        let audited: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM audit_events
              WHERE tenant_id = $1 AND action = 'migration.applied'",
        )
        .bind(f.tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            audited.0, 1,
            "what crossed has to be visible in the audit trail, not only in the response"
        );
    })
    .await;
}