//! End-to-end API tests against a real database.
//!
//! These drive the router through HTTP, so they exercise the envelope, the
//! auth extractor, RBAC, tenant isolation, idempotency, and pagination exactly
//! as a client would.

use std::sync::Arc;

use axum::body::Body;
use chrono::{DateTime, Timelike};
use axum::extract::ConnectInfo;
use forge_api::create_router;
use http::Request;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;
use uuid::Uuid;

struct TestDb {
    pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    /// Drops the throwaway database.
    ///
    /// Failures are reported rather than swallowed: a silently skipped drop
    /// accumulates databases on every run until the server refuses to create
    /// more.
    async fn cleanup(self) {
        self.pool.close().await;
        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            Ok(admin) => admin,
            Err(e) => {
                eprintln!("could not drop {}: {e}", self.db_name);
                return;
            }
        };

        if let Err(e) = admin
            .execute(
                format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                     WHERE datname = '{}' AND pid <> pg_backend_pid()",
                    self.db_name
                )
                .as_str(),
            )
            .await
        {
            eprintln!("could not release connections to {}: {e}", self.db_name);
        }

        if let Err(e) = admin
            .execute(format!(r#"DROP DATABASE IF EXISTS "{}""#, self.db_name).as_str())
            .await
        {
            eprintln!("could not drop {}: {e}", self.db_name);
        }
        admin.close().await;
    }

    async fn new() -> Option<TestDb> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let db_name = format!("forge_api_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new().max_connections(1).connect(&admin_url).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping API integration tests: cannot connect ({e})");
                return None;
            }
        };
        if admin
            .execute(format!(r#"CREATE DATABASE "{db_name}""#).as_str())
            .await
            .is_err()
        {
            eprintln!("skipping API integration tests: cannot create database");
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
                eprintln!("skipping API integration tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("../forge-storage/migrations")
            .run(&pool)
            .await
        {
            eprintln!("skipping API integration tests: migrations failed ({e})");
            return None;
        }

        Some(TestDb { pool, admin_url, db_name })
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
                        Err(_) => eprintln!("warning: test db handle still shared"),
                    }
                }
                None => eprintln!("(skipped: no database available)"),
            }
        }
    };
}

const SECRET: &str = "integration-test-secret-not-for-production";

fn router(pool: PgPool) -> axum::Router {
    create_router(
        pool,
        SECRET,
        "http://localhost:3000/api/v1",
    )
}

/// Issues a request, attaching `ConnectInfo` so the rate limiter works.
async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
    extra_headers: &[(&str, &str)],
) -> (u16, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }

    let payload = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let mut req = builder.body(payload).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:4000".parse::<std::net::SocketAddr>().unwrap(),
    ));

    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// Registers a user and returns its access token and tenant.
///
/// Each call gets a fresh tenant, so tests running in parallel never contend
/// for the same rows. The handler reuses an existing tenant when one is
/// present, which is right for a single-tenant deployment but would make
/// independent tests interfere.
async fn register_user(pool: &PgPool, email: &str, password: &str) -> (String, Uuid) {
    // Isolate this registration in its own tenant.
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Isolated')")
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .unwrap();
    let tenant: (Uuid,) = sqlx::query_as("SELECT id FROM tenants WHERE name = 'Isolated' LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();

    // Register the user directly against that tenant rather than going through
    // the public endpoint, which would pick whichever tenant it finds first.
    let user_id = Uuid::new_v4();
    let hash = forge_auth::hash_password(password).unwrap();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $2)",
    )
    .bind(user_id)
    .bind(email)
    .bind(&hash)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'OWNER')",
    )
    .bind(user_id)
    .bind(tenant.0)
    .execute(pool)
    .await
    .unwrap();

    let jwt = forge_auth::JwtService::new(SECRET.as_bytes());
    let token = jwt
        .issue(user_id, tenant.0, forge_auth::Role::Owner)
        .unwrap();
    (token, tenant.0)
}

/// Registers through the public endpoint, for tests that exercise it directly.
///
/// Returns `(access_token, refresh_token, tenant_id)`.
async fn register_via_api(
    pool: &PgPool,
    email: &str,
    password: &str,
) -> (String, String, Uuid) {
    let app = router(pool.clone());
    let (status, body) = send(
        &app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "email": email, "password": password })),
        &[],
    )
    .await;
    assert_eq!(status, 201, "register failed: {body}");

    let tenant: (Uuid,) = sqlx::query_as("SELECT id FROM tenants ORDER BY created_at ASC LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();

    (
        body["data"]["access_token"].as_str().unwrap().to_string(),
        body["data"]["refresh_token"].as_str().unwrap().to_string(),
        tenant.0,
    )
}

// AT-API-001/002/004: the full auth path works end to end.
#[tokio::test]
async fn register_login_and_use_a_token() {
    with_db!(|pool: PgPool| async move {
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({
                "email": "owner@example.com",
                "password": "correct horse battery",
            })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{body}");
        assert_eq!(body["data"]["role"], "OWNER");
        assert!(body["data"]["access_token"].is_string());
        assert!(body["data"]["refresh_token"].is_string());
        assert!(body["request_id"].is_string(), "the envelope carries a request id");

        let token = body["data"]["access_token"].as_str().unwrap();

        // The token authenticates a tenant-scoped read.
        let (status, jobs) = send(&app, "GET", "/api/v1/jobs", Some(token), None, &[]).await;
        assert_eq!(status, 200, "{jobs}");
        assert_eq!(jobs["data"].as_array().unwrap().len(), 0);

        // Logging in again issues a fresh session.
        let (status, login) = send(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({
                "email": "owner@example.com",
                "password": "correct horse battery",
            })),
            &[],
        )
        .await;
        assert_eq!(status, 200, "{login}");
        assert!(login["data"]["access_token"].is_string());
    })
    .await;
}

#[tokio::test]
async fn a_wrong_password_is_refused() {
    with_db!(|pool: PgPool| async move {
        let app = router(pool.clone());
        send(
            &app,
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({"email": "a@b.com", "password": "correct horse battery"})),
            &[],
        )
        .await;

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "a@b.com", "password": "wrong password entirely"})),
            &[],
        )
        .await;
        assert_eq!(status, 401, "{body}");
        assert_eq!(body["error"]["code"], "AUTHENTICATION_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn an_unknown_account_looks_the_same_as_a_wrong_password() {
    with_db!(|pool: PgPool| async move {
        let app = router(pool.clone());
        // Register one account so the two paths are comparable.
        send(
            &app,
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({"email": "real@b.com", "password": "correct horse battery"})),
            &[],
        )
        .await;

        let (wrong_password, _) = send(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "real@b.com", "password": "wrong password entirely"})),
            &[],
        )
        .await;
        let (no_such_user, _) = send(
            &app,
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"email": "ghost@b.com", "password": "wrong password entirely"})),
            &[],
        )
        .await;

        assert_eq!(wrong_password, 401);
        assert_eq!(
            no_such_user, 401,
            "an unknown account must not be distinguishable from a wrong password"
        );
    })
    .await;
}

/// A refresh token is single-use; replaying it revokes the chain (spec 11.3).
#[tokio::test]
async fn a_refresh_token_is_single_use() {
    with_db!(|pool: PgPool| async move {
        // Register through the public endpoint, so a refresh token exists.
        let (_token, refresh, _tenant) =
            register_via_api(&pool, "rt@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        // First use succeeds and rotates the token.
        let (status, first) = send(
            &app,
            "POST",
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": &refresh })),
            &[],
        )
        .await;
        assert_eq!(status, 200, "{first}");
        assert_ne!(
            first["data"]["refresh_token"].as_str().unwrap(),
            refresh,
            "rotation must issue a new refresh token"
        );

        // Replaying the consumed token is reuse, and is refused.
        let (status, second) = send(
            &app,
            "POST",
            "/api/v1/auth/refresh",
            None,
            Some(json!({ "refresh_token": &refresh })),
            &[],
        )
        .await;
        assert_eq!(status, 401, "a replayed refresh token must be refused: {second}");
    })
    .await;
}

#[tokio::test]
async fn one_tenant_cannot_read_another_tents_job() {
    with_db!(|pool: PgPool| async move {
        // Two independent tenants.
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        for id in [tenant_a, tenant_b] {
            sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let jwt = forge_auth::JwtService::new(SECRET.as_bytes());
        let token_a = jwt.issue(Uuid::new_v4(), tenant_a, forge_auth::Role::Admin).unwrap();
        let token_b = jwt.issue(Uuid::new_v4(), tenant_b, forge_auth::Role::Admin).unwrap();
        let app = router(pool.clone());

        // A creates a job.
        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token_a),
            Some(json!({ "name": "A's job" })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{created}");
        let job_id = created["data"]["id"].as_str().unwrap().to_string();

        // B cannot see it.
        let (status, denied) = send(
            &app,
            "GET",
            &format!("/api/v1/jobs/{job_id}"),
            Some(&token_b),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 404, "a cross-tenant read must be not-found: {denied}");
        assert_eq!(
            denied["error"]["code"], "NOT_FOUND",
            "the error must not reveal the resource exists elsewhere"
        );

        // Nor can B list it.
        let (status, list) = send(&app, "GET", "/api/v1/jobs", Some(&token_b), None, &[]).await;
        assert_eq!(status, 200);
        assert!(list["data"].as_array().unwrap().is_empty());
    })
    .await;
}

// AT-TEN-002: tenant A cannot trigger tenant B's job.
#[tokio::test]
async fn one_tenant_cannot_trigger_another_tents_job() {
    with_db!(|pool: PgPool| async move {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        for id in [tenant_a, tenant_b] {
            sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let jwt = forge_auth::JwtService::new(SECRET.as_bytes());
        let token_a = jwt.issue(Uuid::new_v4(), tenant_a, forge_auth::Role::Admin).unwrap();
        let token_b = jwt.issue(Uuid::new_v4(), tenant_b, forge_auth::Role::Admin).unwrap();
        let app = router(pool.clone());

        let (_, created) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token_a),
            Some(json!({ "name": "A's job" })),
            &[],
        )
        .await;
        let job_id = created["data"]["id"].as_str().unwrap().to_string();

        let (status, denied) = send(
            &app,
            "POST",
            &format!("/api/v1/jobs/{job_id}/trigger"),
            Some(&token_b),
            Some(json!({})),
            &[],
        )
        .await;
        assert_eq!(status, 404, "{denied}");
    })
    .await;
}

/// AT-API-002: a viewer cannot author.
#[tokio::test]
async fn a_viewer_cannot_write() {
    with_db!(|pool: PgPool| async move {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T')")
            .bind(Uuid::new_v4())
            .execute(&pool)
            .await
            .unwrap();
        let tenant: (Uuid,) = sqlx::query_as("SELECT id FROM tenants LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let jwt = forge_auth::JwtService::new(SECRET.as_bytes());
        let viewer = jwt
            .issue(Uuid::new_v4(), tenant.0, forge_auth::Role::Viewer)
            .unwrap();
        let app = router(pool.clone());

        // Reading is allowed.
        let (status, _) = send(&app, "GET", "/api/v1/jobs", Some(&viewer), None, &[]).await;
        assert_eq!(status, 200);

        // Writing is not.
        let (status, denied) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&viewer),
            Some(json!({ "name": "nope" })),
            &[],
        )
        .await;
        assert_eq!(status, 403, "{denied}");
        assert_eq!(denied["error"]["code"], "AUTHORIZATION_DENIED");
    })
    .await;
}

/// AT-API-004: invalid input returns the standard error envelope.
#[tokio::test]
async fn invalid_input_returns_a_standard_error() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "   " })),
            &[],
        )
        .await;

        assert_eq!(status, 400, "{body}");
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(body["error"]["details"][0]["field"], "name");
        assert!(body["error"]["request_id"].is_string());
    })
    .await;
}

#[tokio::test]
async fn an_unknown_priority_is_rejected() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "job", "priority": "URGENT" })),
            &[],
        )
        .await;
        assert_eq!(status, 400);
        assert_eq!(body["error"]["details"][0]["field"], "priority");
    })
    .await;
}

/// The full definition lifecycle: create, version, publish, trigger.
#[tokio::test]
async fn a_job_can_be_defined_published_and_triggered() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Nightly settlement", "key": "settle" })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{created}");
        assert_eq!(created["data"]["status"], "DRAFT");
        let job_id = created["data"]["id"].as_str().unwrap().to_string();

        // A draft cannot be triggered.
        let (status, refused) = send(
            &app,
            "POST",
            &format!("/api/v1/jobs/{job_id}/trigger"),
            Some(&token),
            Some(json!({})),
            &[],
        )
        .await;
        assert_eq!(status, 409, "{refused}");

        // Create and publish a version.
        let (status, version) = send(
            &app,
            "POST",
            &format!("/api/v1/jobs/{job_id}/versions"),
            Some(&token),
            Some(json!({ "execution_type": "WORKER_TASK" })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{version}");
        assert_eq!(version["data"]["version_number"], 1);
        let version_id = version["data"]["id"].as_str().unwrap().to_string();

        let (status, published) = send(
            &app,
            "POST",
            &format!("/api/v1/jobs/{job_id}/versions/{version_id}/publish"),
            Some(&token),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 200, "{published}");
        assert!(published["data"]["published_at"].is_string());

        // Now it can run.
        let (status, execution) = send(
            &app,
            "POST",
            &format!("/api/v1/jobs/{job_id}/trigger"),
            Some(&token),
            Some(json!({ "input": { "amount": 100 } })),
            &[],
        )
        .await;
        assert_eq!(status, 202, "{execution}");
        assert_eq!(execution["data"]["status"], "QUEUED");
        assert_eq!(execution["data"]["trigger_source"], "MANUAL");

        // And it appears in the execution list.
        let (status, list) = send(&app, "GET", "/api/v1/executions", Some(&token), None, &[]).await;
        assert_eq!(status, 200);
        assert_eq!(list["data"].as_array().unwrap().len(), 1);
    })
    .await;
}

// AT-SCH-010: a manual trigger must not alter the calendar.
#[tokio::test]
async fn a_manual_trigger_does_not_disturb_the_schedule() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (_, job) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Scheduled" })),
            &[],
        )
        .await;
        let job_id = job["data"]["id"].as_str().unwrap().to_string();

        // Create a schedule pointing at the job.
        let (status, schedule) = send(
            &app,
            "POST",
            "/api/v1/schedules",
            Some(&token),
            Some(json!({
                "target_id": job_id,
                "timezone": "UTC",
                "expression": "0 2 * * *",
            })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{schedule}");
        let next_run_before = schedule["data"]["next_run_at"].as_str().unwrap().to_string();

        // Trigger the job manually several times.
        for _ in 0..3 {
            send(
                &app,
                "POST",
                &format!("/api/v1/jobs/{job_id}/trigger"),
                Some(&token),
                Some(json!({})),
                &[],
            )
            .await;
        }

        // The schedule's next run is untouched.
        let (status, after) = send(
            &app,
            "GET",
            &format!("/api/v1/schedules/{}", schedule["data"]["id"].as_str().unwrap()),
            Some(&token),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 200, "{after}");
        // Compare as instants: the response renders an offset (`+00:00`) while
        // the stored value may round-trip as `Z`, and both denote the same time.
        let now_after = chrono::DateTime::parse_from_rfc3339(
            after["data"]["next_run_at"].as_str().unwrap(),
        )
        .unwrap();
        let before = chrono::DateTime::parse_from_rfc3339(&next_run_before).unwrap();
        assert_eq!(
            now_after, before,
            "a manual trigger must not move the calendar"
        );
    })
    .await;
}

/// Spec 09.13: the preview and the scheduler share one engine.
#[tokio::test]
async fn the_schedule_preview_matches_the_scheduler() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (_, job) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Scheduled" })),
            &[],
        )
        .await;
        let job_id = job["data"]["id"].as_str().unwrap().to_string();

        let (_, schedule) = send(
            &app,
            "POST",
            "/api/v1/schedules",
            Some(&token),
            Some(json!({
                "target_id": job_id,
                "timezone": "Asia/Kolkata",
                "expression": "0 2 * * *",
            })),
            &[],
        )
        .await;
        let schedule_id = schedule["data"]["id"].as_str().unwrap().to_string();

        let (status, preview) = send(
            &app,
            "POST",
            &format!("/api/v1/schedules/{schedule_id}/preview"),
            Some(&token),
            Some(json!({ "count": 5 })),
            &[],
        )
        .await;
        assert_eq!(status, 200, "{preview}");

        let occurrences = preview["data"]["occurrences"].as_array().unwrap();
        assert_eq!(occurrences.len(), 5, "preview returns the requested count");

        // Each previewed occurrence is 02:00 in the schedule's own timezone.
        let kolkata: chrono_tz::Tz = "Asia/Kolkata".parse().unwrap();
        for occurrence in occurrences {
            let at = DateTime::parse_from_rfc3339(occurrence.as_str().unwrap())
                .unwrap()
                .with_timezone(&kolkata);
            assert_eq!(
                (at.hour(), at.minute()),
                (2, 0),
                "previewed runs must be at the intended local time: {at}"
            );
        }
    })
    .await;
}

// AT-API-003: pagination walks every job exactly once.
#[tokio::test]
async fn pagination_walks_every_job() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        for i in 0..7 {
            let (status, body) = send(
                &app,
                "POST",
                "/api/v1/jobs",
                Some(&token),
                Some(json!({ "name": format!("job-{i}") })),
                &[],
            )
            .await;
            assert_eq!(status, 201, "{body}");
        }

        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let uri = match &cursor {
                Some(c) => format!("/api/v1/jobs?limit=3&cursor={c}"),
                None => "/api/v1/jobs?limit=3".to_string(),
            };
            let (status, page) = send(&app, "GET", &uri, Some(&token), None, &[]).await;
            assert_eq!(status, 200, "{page}");

            for item in page["data"].as_array().unwrap() {
                seen.push(item["id"].as_str().unwrap().to_string());
            }

            if page["page"]["has_more"] == false {
                assert!(page["page"]["next_cursor"].is_null());
                break;
            }
            cursor = page["page"]["next_cursor"].as_str().map(str::to_string);
        }

        assert_eq!(seen.len(), 7, "every job is returned exactly once");
        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), 7, "no job is returned twice");
    })
    .await;
}

// AT-API-005: an idempotent mutation replays the original response.
#[tokio::test]
async fn an_idempotent_mutation_replays() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let body = json!({ "name": "Idempotent job" });
        let headers = [("idempotency-key", "key-abc-123")];

        let (first_status, first) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(body.clone()),
            &headers,
        )
        .await;
        assert_eq!(first_status, 201, "{first}");

        // The replay returns the identical response.
        let (second_status, second) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(body),
            &headers,
        )
        .await;
        assert_eq!(second_status, 201);
        assert_eq!(first["data"]["id"], second["data"]["id"], "the replay created no new job");

        let (_, list) = send(&app, "GET", "/api/v1/jobs", Some(&token), None, &[]).await;
        assert_eq!(list["data"].as_array().unwrap().len(), 1);
    })
    .await;
}

// AT-API-006: the same key with a different body conflicts.
#[tokio::test]
async fn a_reused_key_with_a_different_body_conflicts() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());
        let headers = [("idempotency-key", "key-reused")];

        send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "First" })),
            &headers,
        )
        .await;

        let (status, conflict) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Second" })),
            &headers,
        )
        .await;
        assert_eq!(status, 409, "{conflict}");
        assert_eq!(conflict["error"]["code"], "IDEMPOTENCY_KEY_CONFLICT");
    })
    .await;
}

/// Two tenants may reuse the same idempotency key independently.
#[tokio::test]
async fn idempotency_keys_are_tenant_scoped() {
    with_db!(|pool: PgPool| async move {
        let tenants: Vec<Uuid> = vec![Uuid::new_v4(), Uuid::new_v4()];
        for id in &tenants {
            sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let jwt = forge_auth::JwtService::new(SECRET.as_bytes());
        let token_a = jwt.issue(Uuid::new_v4(), tenants[0], forge_auth::Role::Admin).unwrap();
        let token_b = jwt.issue(Uuid::new_v4(), tenants[1], forge_auth::Role::Admin).unwrap();
        let app = router(pool.clone());

        let headers = [("idempotency-key", "shared-key")];
        let (status_a, a) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token_a),
            Some(json!({ "name": "A" })),
            &headers,
        )
        .await;
        let (status_b, b) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token_b),
            Some(json!({ "name": "B" })),
            &headers,
        )
        .await;

        assert_eq!(status_a, 201, "{a}");
        assert_eq!(status_b, 201, "{b}");
        assert_ne!(a["data"]["id"], b["data"]["id"], "each tenant made its own job");
    })
    .await;
}

// AT-API-007: a stale update conflicts.
#[tokio::test]
async fn a_stale_update_conflicts() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (_, created) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Original" })),
            &[],
        )
        .await;
        let job_id = created["data"]["id"].as_str().unwrap().to_string();
        let stale = created["data"]["updated_at"].as_str().unwrap().to_string();

        // First update succeeds against the timestamp we read.
        let (status, updated) = send(
            &app,
            "PATCH",
            &format!("/api/v1/jobs/{job_id}"),
            Some(&token),
            Some(json!({ "name": "Renamed", "expectedUpdatedAt": stale })),
            &[],
        )
        .await;
        assert_eq!(status, 200, "{updated}");
        assert_eq!(updated["data"]["name"], "Renamed");

        // The same timestamp is now stale.
        let (status, conflict) = send(
            &app,
            "PATCH",
            &format!("/api/v1/jobs/{job_id}"),
            Some(&token),
            Some(json!({ "name": "Again", "expectedUpdatedAt": stale })),
            &[],
        )
        .await;
        assert_eq!(status, 409, "{conflict}");
        assert_eq!(conflict["error"]["code"], "CONFLICT");
    })
    .await;
}

/// AT-SEC-001: a created secret is never echoed again.
#[tokio::test]
async fn an_api_key_is_shown_once() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/api-keys",
            Some(&token),
            Some(json!({ "name": "ci" })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{created}");
        let raw = created["data"]["key"].as_str().unwrap().to_string();
        assert!(raw.starts_with("forge_"));

        // The listing shows only metadata, never the value.
        let (_, list) = send(&app, "GET", "/api/v1/api-keys", Some(&token), None, &[]).await;
        let text = list.to_string();
        assert!(!text.contains(&raw), "the key value must not be listed");
        assert!(text.contains("prefix"));
    })
    .await;
}

/// Spec 09.11: pausing a schedule prevents new executions.
#[tokio::test]
async fn a_paused_schedule_stops_firing() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (_, job) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Job" })),
            &[],
        )
        .await;
        let job_id = job["data"]["id"].as_str().unwrap().to_string();

        let (_, schedule) = send(
            &app,
            "POST",
            "/api/v1/schedules",
            Some(&token),
            Some(json!({
                "target_id": job_id,
                "timezone": "UTC",
                "expression": "0 * * * *",
            })),
            &[],
        )
        .await;
        let schedule_id = schedule["data"]["id"].as_str().unwrap().to_string();

        // The computed next run is the next whole hour, which is not due yet.
        // Pull it into the past so the scheduler has something to claim.
        sqlx::query("UPDATE schedules SET next_run_at = NOW() - INTERVAL '1 minute' WHERE id = $1")
            .bind(Uuid::parse_str(&schedule_id).unwrap())
            .execute(&pool)
            .await
            .unwrap();

        let (status, paused) = send(
            &app,
            "POST",
            &format!("/api/v1/schedules/{schedule_id}/pause"),
            Some(&token),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 200, "{paused}");

        // The scheduler claims nothing while paused.
        let engine = forge_scheduler::SchedulerEngine::new(pool.clone(), 100);
        let report = engine.tick().await;
        assert_eq!(report.claimed, 0, "a paused schedule must not be claimed");

        // Resuming makes it claimable again.
        send(
            &app,
            "POST",
            &format!("/api/v1/schedules/{schedule_id}/resume"),
            Some(&token),
            None,
            &[],
        )
        .await;

        let report = engine.tick().await;
        assert_eq!(report.claimed, 1, "a resumed schedule fires again");
    })
    .await;
}

/// A schedule with an invalid expression is refused at creation.
#[tokio::test]
async fn an_invalid_cron_expression_is_rejected() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (_, job) = send(
            &app,
            "POST",
            "/api/v1/jobs",
            Some(&token),
            Some(json!({ "name": "Job" })),
            &[],
        )
        .await;
        let job_id = job["data"]["id"].as_str().unwrap().to_string();

        for bad in ["not a cron", "0 0 2 * * *", "* * *"] {
            let (status, body) = send(
                &app,
                "POST",
                "/api/v1/schedules",
                Some(&token),
                Some(json!({
                    "target_id": job_id,
                    "timezone": "UTC",
                    "expression": bad,
                })),
                &[],
            )
            .await;
            assert_eq!(status, 400, "{bad:?} should be rejected: {body}");
        }

        // An unknown timezone is refused too (spec 09.5).
        let (status, body) = send(
            &app,
            "POST",
            "/api/v1/schedules",
            Some(&token),
            Some(json!({
                "target_id": job_id,
                "timezone": "Mars/Olympus",
                "expression": "0 2 * * *",
            })),
            &[],
        )
        .await;
        assert_eq!(status, 400, "{body}");
    })
    .await;
}

/// A worker's lifecycle endpoints work end to end.
#[tokio::test]
async fn a_worker_registers_drains_and_is_revoked() {
    with_db!(|pool: PgPool| async move {
        let (token, _) = register_user(&pool, "owner@example.com", "correct horse battery").await;
        let app = router(pool.clone());

        let (status, worker) = send(
            &app,
            "POST",
            "/api/v1/workers/register",
            Some(&token),
            Some(json!({ "hostname": "host-1", "capabilities": ["linux"] })),
            &[],
        )
        .await;
        assert_eq!(status, 201, "{worker}");
        assert_eq!(worker["data"]["status"], "READY");
        let worker_id = worker["data"]["id"].as_str().unwrap().to_string();

        let (status, beat) = send(
            &app,
            "POST",
            &format!("/api/v1/workers/{worker_id}/heartbeat"),
            Some(&token),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 200, "{beat}");

        let (status, drained) = send(
            &app,
            "POST",
            &format!("/api/v1/workers/{worker_id}/drain"),
            Some(&token),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(drained["data"]["status"], "DRAINING");

        let (status, revoked) = send(
            &app,
            "POST",
            &format!("/api/v1/workers/{worker_id}/revoke"),
            Some(&token),
            None,
            &[],
        )
        .await;
        assert_eq!(status, 200, "{revoked}");
        assert_eq!(revoked["data"]["status"], "REVOKED");
    })
    .await;
}

/// A read-only role cannot administer users.
#[tokio::test]
async fn a_developer_cannot_manage_users() {
    with_db!(|pool: PgPool| async move {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'T')")
            .bind(Uuid::new_v4())
            .execute(&pool)
            .await
            .unwrap();
        let tenant: (Uuid,) = sqlx::query_as("SELECT id FROM tenants LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

        let jwt = forge_auth::JwtService::new(SECRET.as_bytes());
        let developer = jwt
            .issue(Uuid::new_v4(), tenant.0, forge_auth::Role::Developer)
            .unwrap();
        let app = router(pool.clone());

        let (status, denied) = send(
            &app,
            "POST",
            "/api/v1/users",
            Some(&developer),
            Some(json!({
                "email": "new@example.com",
                "password": "correct horse battery",
                "role": "VIEWER",
            })),
            &[],
        )
        .await;
        assert_eq!(status, 403, "{denied}");
    })
    .await;
}

/// The OpenAPI document describes the surface a client will call.
#[tokio::test]
async fn the_openapi_document_describes_the_api() {
    with_db!(|pool: PgPool| async move {
        let app = router(pool.clone());
        let (status, doc) = send(&app, "GET", "/api/v1/openapi.json", None, None, &[]).await;
        assert_eq!(status, 200);

        assert_eq!(doc["openapi"], "3.1.0");
        let paths = doc["paths"].as_object().unwrap();
        for expected in [
            "/jobs",
            "/jobs/{job_id}",
            "/jobs/{job_id}/versions",
            "/jobs/{job_id}/trigger",
            "/schedules",
            "/schedules/{id}/preview",
            "/executions",
            "/executions/{id}/cancel",
            "/workers",
            "/queues",
            "/users",
            "/api-keys",
            "/audit-events",
            "/auth/login",
            "/health/live",
        ] {
            assert!(paths.contains_key(expected), "openapi is missing {expected}");
        }

        // Spec 09.4/09.6: the choices are published.
        assert_eq!(doc["x-forge-scheduling"]["cronDialect"]["fields"], 5);
    })
    .await;
}

