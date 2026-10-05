//! Cross-environment migration planning (`redesign.md` §3).
//!
//! The plan is the safety property: nothing crosses environments until an
//! operator has read what would cross. These tests pin that the plan is
//! accurate, that it applies nothing, and that a job which cannot be matched on
//! the way back is reported rather than skipped.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use forge_api::create_router;
use http::Request;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "migration-test-secret";
const PEPPER: &[u8] = b"migration-test-pepper";

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
        let db_name = format!("forge_mig_{}", Uuid::new_v4().simple());

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
}

/// A tenant with dev and production environments and two scheduled jobs in dev.
async fn fixture(pool: &PgPool) -> Fixture {
    let email = format!("mig-{}@example.com", Uuid::new_v4().simple());
    let hash = forge_auth::hash_password("migration-password-1").unwrap();
    let user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id")
            .bind(Uuid::new_v4())
            .bind(&email)
            .bind(&hash)
            .fetch_one(pool)
            .await
            .unwrap();

    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (id, name) VALUES ($1, 'Mig') RETURNING id")
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

    // The trigger provisions a `default` environment; add dev and production.
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

    // Two jobs in dev, keyed so they can be matched on the way back. The second
    // has no key and must therefore be reported rather than silently skipped.
    for (name, key) in [("nightly-settlement", Some("nightly")), ("no-key-job", None)] {
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
            "INSERT INTO schedules (id, tenant_id, target_id, target_type, cron_expression,
                                     timezone, misfire_policy, catch_up_policy, enabled)
             VALUES ($1, $2, $3, 'JOB', '0 2 * * *', 'UTC', 'FIRE_ONCE', '[]'::jsonb, TRUE)",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(job_id)
        .execute(pool)
        .await
        .unwrap();
    }

    Fixture {
        token: forge_auth::JwtService::new(SECRET.as_bytes())
            .issue(user_id, tenant_id, forge_auth::Role::Owner)
            .unwrap(),
        tenant_id,
        dev_env,
        prod_env,
    }
}

#[tokio::test]
async fn planning_reports_what_would_cross_and_changes_nothing() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 200, "{body}");

        let entries = body["data"]["plan"]["entries"].as_array().unwrap();
        // Two dev jobs: one keyed, one not.
        assert_eq!(entries.len(), 2, "{body}");

        let keyed = entries
            .iter()
            .find(|e| e["name"] == "nightly-settlement")
            .expect("the keyed job is planned");
        assert_eq!(keyed["source_key"], "nightly");
        assert_eq!(keyed["action"], "create");
        assert_eq!(
            keyed["unresolved"].as_array().unwrap().len(),
            0,
            "identity and schedule cross without resolution"
        );

        // Bindings are excluded by default: a plan that included them would
        // point production at a dev queue.
        // `MigratableField` is an internally-tagged enum, so it serialises as
        // {"kind":"identity"} rather than a bare string.
        let include: Vec<String> = body["data"]["plan"]["include"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field["kind"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            include,
            vec!["identity", "schedule", "parameters"],
            "the portable fields cross by default and nothing else"
        );
        assert!(
            !include.iter().any(|field| field == "bindings"),
            "bindings must be opt-in: {include:?}"
        );

        // The key point of a plan: nothing moved.
        let dev_jobs: (i64,) =
            sqlx::query_as("SELECT count(*) FROM jobs WHERE environment_id = $1")
                .bind(f.dev_env)
                .fetch_one(&pool)
                .await
                .unwrap();
        let prod_jobs: (i64,) =
            sqlx::query_as("SELECT count(*) FROM jobs WHERE environment_id = $1")
                .bind(f.prod_env)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(dev_jobs.0, 2, "planning must not remove anything");
        assert_eq!(prod_jobs.0, 0, "planning must not create anything");
    })
    .await;
}

#[tokio::test]
async fn a_job_without_a_key_is_reported_rather_than_skipped() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 200, "{body}");

        let entries = body["data"]["plan"]["entries"].as_array().unwrap();
        let unkeyed = entries
            .iter()
            .find(|e| e["name"] == "no-key-job")
            .expect("a keyless job must still appear in the plan");
        // It cannot be matched on the way back, so it is not presented as a
        // clean create. An operator who did not see this would believe the job
        // was moving.
        assert!(
            !unkeyed["unresolved"].as_array().unwrap().is_empty(),
            "a keyless job must be reported as unresolvable: {unkeyed}"
        );
        assert_ne!(unkeyed["action"], "create", "{unkeyed}");
    })
    .await;
}

#[tokio::test]
async fn a_job_already_present_in_the_target_is_an_update() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;

        // The same key, in production, with a different expression.
        sqlx::query(
            "INSERT INTO jobs (id, tenant_id, name, status, environment_id, key)
             VALUES ($1, $2, 'Nightly Settlement', 'ACTIVE', $3, 'nightly')",
        )
        .bind(Uuid::new_v4())
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .execute(&pool)
        .await
        .unwrap();

        let app = router(pool.clone());
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 200, "{body}");

        let entries = body["data"]["plan"]["entries"].as_array().unwrap();
        let keyed = entries
            .iter()
            .find(|e| e["name"] == "nightly-settlement")
            .expect("planned");
        assert_eq!(
            keyed["action"], "update",
            "a matching key with a different definition is an update: {keyed}"
        );
        assert_eq!(body["data"]["summary"]["update"], 1, "{body}");
    })
    .await;
}

#[tokio::test]
async fn an_identical_job_in_the_target_is_reported_unchanged() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;

        // Copy the dev job's definition verbatim into production.
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO jobs (id, tenant_id, name, status, environment_id, key)
             VALUES ($1, $2, 'nightly-settlement', 'ACTIVE', $3, 'nightly') RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(f.tenant_id)
        .bind(f.prod_env)
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO schedules (id, tenant_id, target_id, target_type, cron_expression,
                                     timezone, misfire_policy, catch_up_policy, enabled)
             VALUES ($1, $2, $3, 'JOB', '0 2 * * *', 'UTC', 'FIRE_ONCE', '[]'::jsonb, TRUE)",
        )
        .bind(Uuid::new_v4())
        .bind(f.tenant_id)
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();

        let app = router(pool.clone());
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 200, "{body}");

        let keyed = body["data"]["plan"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == "nightly-settlement")
            .expect("planned")
            .clone();
        // An "update" that changes nothing is noise; an operator reviewing a
        // plan of forty jobs needs the unchanged ones to be distinguishable.
        assert_eq!(keyed["action"], "unchanged", "{keyed}");
    })
    .await;
}

#[tokio::test]
async fn planning_to_the_same_environment_is_refused() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=dev",
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 400, "{body}");
    })
    .await;
}

#[tokio::test]
async fn applying_without_binding_resolution_is_refused() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;
        let app = router(pool.clone());

        let (_, planned) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
            &f.token,
            Some(serde_json::json!({})),
        )
        .await;

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/apply",
            &f.token,
            Some(serde_json::json!({ "plan": planned["data"]["plan"] })),
        )
        .await;
        // Refused rather than creating jobs pointing at a dev queue. 400 is
        // this API's validation status; the point is that it is a refusal with
        // a reason, not a 501 "not implemented" and not a silent success.
        assert_eq!(status, 400, "{body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("binding"),
            "{body}"
        );

        // And nothing was created.
        let prod_jobs: (i64,) =
            sqlx::query_as("SELECT count(*) FROM jobs WHERE environment_id = $1")
                .bind(f.prod_env)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(prod_jobs.0, 0);
    })
    .await;
}

#[tokio::test]
async fn planning_is_scoped_to_the_callers_tenant() {
    with_db!(|pool: PgPool| async move {
        let f = fixture(&pool).await;

        // Another tenant with its own `prod` environment. The plan must not see
        // it, and resolving the target must not reach across.
        let other_hash = forge_auth::hash_password("other-password-123").unwrap();
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

        // The other tenant has no `dev`, so its plan cannot start.
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/migration/plan?from_environment=dev&to_environment=prod",
            &other_token,
            Some(serde_json::json!({})),
        )
        .await;
        assert_eq!(status, 404, "{body}");

        let _ = f;
    })
    .await;
}