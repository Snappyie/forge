//! Applications and environments (`redesign.md` §D, §2.G).
//!
//! These are the two containers PowerJob-style grouping and cross-environment
//! migration depend on, and they had no endpoint at all: nothing could create an
//! application, and there was nothing to migrate a job between.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use forge_api::create_router;
use http::Request;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "applications-test-secret";
const PEPPER: &[u8] = b"applications-test-pepper";

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
        let db_name = format!("forge_apps_{}", Uuid::new_v4().simple());

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
                    // Spawned so a failing assertion cannot skip cleanup.
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

/// The router over this test database.
///
/// `create_router` is synchronous, and every request needs `ConnectInfo`
/// because the rate limiter keys on the peer address.
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
    path: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let payload = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    let mut request = builder.body(payload).unwrap();
    // The rate limiter reads the peer address; without this every request is a
    // 500 rather than the response under test.
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:4000".parse::<std::net::SocketAddr>().unwrap(),
    ));

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}

async fn admin_token(pool: &PgPool) -> String {
    let email = format!("admin-{}@example.com", Uuid::new_v4().simple());
    let hash = forge_auth::hash_password("admin-password-123").unwrap();

    let user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id")
            .bind(Uuid::new_v4())
            .bind(&email)
            .bind(&hash)
            .fetch_one(pool)
            .await
            .unwrap();

    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (id, name) VALUES ($1, 'Apps Tenant') RETURNING id",
    )
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

    forge_auth::JwtService::new(SECRET.as_bytes())
        .issue(user_id, tenant_id, forge_auth::Role::Owner)
        .unwrap()
}

#[tokio::test]
async fn an_application_is_created_read_and_derived_from_its_name() {
    with_db!(|pool: PgPool| async move {
        let token = admin_token(&pool).await;
        let app = router(pool.clone());

        // A display name with punctuation becomes a usable slug, so an operator
        // is never asked to invent one.
        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/applications",
            Some(&token),
            Some(serde_json::json!({ "name": "Payments API (EU)" })),
        )
        .await;
        assert_eq!(status, 201, "{created}");
        assert_eq!(created["data"]["slug"], "payments-api-eu");
        assert_eq!(created["data"]["job_count"], 0);

        let id = created["data"]["id"].as_str().unwrap().to_string();

        // Readable by id and by slug, because a slug is what a deployment
        // manifest and a bookmarked URL carry.
        let (status, by_id) = send(
            &app,
            "GET",
            &format!("/api/v1/applications/{id}"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, 200, "{by_id}");

        let (status, by_slug) = send(
            &app,
            "GET",
            "/api/v1/applications/by-slug/payments-api-eu",
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, 200, "{by_slug}");
        assert_eq!(by_slug["data"]["id"], id);

        let (status, list) = send(&app, "GET", "/api/v1/applications", Some(&token), None).await;
        assert_eq!(status, 200);
        // Migration 019 provisions a `default` application for every tenant, so
        // the list holds that plus the one just created.
        let applications = list["data"]["applications"].as_array().unwrap();
        assert_eq!(applications.len(), 2, "{list}");
        assert!(
            applications.iter().any(|a| a["slug"] == "payments-api-eu"),
            "the created application must be listed: {list}"
        );
    })
    .await;
}

#[tokio::test]
async fn an_explicitly_invalid_slug_is_refused() {
    with_db!(|pool: PgPool| async move {
        let token = admin_token(&pool).await;
        let app = router(pool.clone());

        // A supplied slug is validated strictly rather than normalised: the
        // caller named something, and silently changing it would publish a URL
        // nobody asked for.
        for slug in ["Payments", "payments--api", "-payments", "payments api"] {
            let (status, body) = send(
                &app,
                "POST",
                "/api/v1/applications",
                Some(&token),
                Some(serde_json::json!({ "name": "Anything", "slug": slug })),
            )
            .await;
            assert_eq!(status, 400, "`{slug}` should be refused: {body}");
        }
    })
    .await;
}

#[tokio::test]
async fn an_environment_kind_is_a_closed_set() {
    with_db!(|pool: PgPool| async move {
        let token = admin_token(&pool).await;
        let app = router(pool.clone());

        let (status, prod) = send(
            &app,
            "POST",
            "/api/v1/environments",
            Some(&token),
            Some(serde_json::json!({ "name": "Production", "kind": "production" })),
        )
        .await;
        assert_eq!(status, 201, "{prod}");
        // `protected` is computed server-side so the guardrail does not depend
        // on every client implementing the rule.
        assert_eq!(prod["data"]["protected"], true);

        // A second production environment would mean a change guardrail
        // protects one and silently skips the other.
        let (status, _) = send(
            &app,
            "POST",
            "/api/v1/environments",
            Some(&token),
            Some(serde_json::json!({ "name": "Production 2", "kind": "production" })),
        )
        .await;
        assert_ne!(status, 201, "two production environments must be refused");

        // An unrecognised kind is refused rather than defaulted: defaulting to
        // production would flag harmless environments, and defaulting to
        // development would create an unguarded `PRODUCTION-1`.
        let (status, bogus) = send(
            &app,
            "POST",
            "/api/v1/environments",
            Some(&token),
            Some(serde_json::json!({ "name": "X", "kind": "production-2" })),
        )
        .await;
        assert_eq!(status, 400, "{bogus}");
    })
    .await;
}

#[tokio::test]
async fn an_application_or_environment_from_another_tenant_is_not_found() {
    with_db!(|pool: PgPool| async move {
        // Two tenants. Isolation is asserted as not-found rather than forbidden,
        // because a forbidden response confirms the id exists somewhere.
        let owner_a = admin_token(&pool).await;
        let email_b = format!("other-{}@example.com", Uuid::new_v4().simple());
        let hash_b = forge_auth::hash_password("other-password-123").unwrap();
        let user_b: Uuid = sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(&email_b)
        .bind(&hash_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        let tenant_b: Uuid =
            sqlx::query_scalar("INSERT INTO tenants (id, name) VALUES ($1, 'B') RETURNING id")
                .bind(Uuid::new_v4())
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'OWNER')",
        )
        .bind(user_b)
        .bind(tenant_b)
        .execute(&pool)
        .await
        .unwrap();
        let token_b = forge_auth::JwtService::new(SECRET.as_bytes())
            .issue(user_b, tenant_b, forge_auth::Role::Owner)
            .unwrap();

        let app = router(pool.clone());

        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/applications",
            Some(&owner_a),
            Some(serde_json::json!({ "name": "Secret App" })),
        )
        .await;
        assert_eq!(status, 201, "{created}");
        let id = created["data"]["id"].as_str().unwrap().to_string();

        // Tenant B cannot read, rename or delete tenant A's application.
        for (method, path) in [
            ("GET", format!("/api/v1/applications/{id}")),
            ("PATCH", format!("/api/v1/applications/{id}")),
            ("DELETE", format!("/api/v1/applications/{id}")),
        ] {
            let body = if method == "PATCH" {
                Some(serde_json::json!({ "name": "Stolen" }))
            } else {
                None
            };
            let (status, response) =
                send(&app, method, &path, Some(&token_b), body).await;
            assert_eq!(
                status, 404,
                "{method} {path} across tenants must read as not-found: {response}"
            );
        }

        // And B's own list does not contain A's application. B does see its own
        // auto-provisioned `default`, so the check is by identity rather than by
        // emptiness.
        let (_, list) = send(&app, "GET", "/api/v1/applications", Some(&token_b), None).await;
        let applications = list["data"]["applications"].as_array().unwrap();
        assert!(
            !applications.iter().any(|a| a["id"] == id.as_str()),
            "tenant B must not see tenant A's application: {list}"
        );
        assert!(
            applications.iter().all(|a| a["name"] != "Secret App"),
            "no application from tenant A may appear: {list}"
        );
    })
    .await;
}

#[tokio::test]
async fn an_environment_in_use_is_not_deleted() {
    with_db!(|pool: PgPool| async move {
        let token = admin_token(&pool).await;
        let app = router(pool.clone());

        let (status, env) = send(
            &app,
            "POST",
            "/api/v1/environments",
            Some(&token),
            Some(serde_json::json!({ "name": "Staging", "kind": "staging" })),
        )
        .await;
        assert_eq!(status, 201, "{env}");
        let env_id = env["data"]["id"].as_str().unwrap().to_string();

        // A job pointed at it.
        sqlx::query(
            "INSERT INTO jobs (id, tenant_id, name, status, environment_id)
             SELECT gen_random_uuid(), t.id, 'settle', 'DRAFT', $1
               FROM tenants t LIMIT 1",
        )
        .bind(Uuid::parse_str(&env_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();

        // The column is nullable so the database would allow the delete, but
        // orphaning a tenant's staging environment is never what was meant.
        let (status, refused) = send(
            &app,
            "DELETE",
            &format!("/api/v1/environments/{env_id}"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, 409, "{refused}");
        assert_eq!(refused["error"]["details"][0]["field"], "job_count");

        // Once the job moves away, the delete succeeds.
        sqlx::query("UPDATE jobs SET environment_id = NULL")
            .execute(&pool)
            .await
            .unwrap();
        let (status, deleted) = send(
            &app,
            "DELETE",
            &format!("/api/v1/environments/{env_id}"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, 200, "{deleted}");
    })
    .await;
}

#[tokio::test]
async fn an_application_holding_jobs_is_not_deleted() {
    with_db!(|pool: PgPool| async move {
        let token = admin_token(&pool).await;
        let app = router(pool.clone());

        let (status, created) = send(
            &app,
            "POST",
            "/api/v1/applications",
            Some(&token),
            Some(serde_json::json!({ "name": "Fraud" })),
        )
        .await;
        assert_eq!(status, 201);
        let id = created["data"]["id"].as_str().unwrap().to_string();

        sqlx::query(
            "INSERT INTO jobs (id, tenant_id, name, status, application_id)
             SELECT gen_random_uuid(), t.id, 'score', 'DRAFT', $1
               FROM tenants t LIMIT 1",
        )
        .bind(Uuid::parse_str(&id).unwrap())
        .execute(&pool)
        .await
        .unwrap();

        let (status, refused) = send(
            &app,
            "DELETE",
            &format!("/api/v1/applications/{id}"),
            Some(&token),
            None,
        )
        .await;
        assert_eq!(status, 409, "{refused}");
    })
    .await;
}

#[tokio::test]
async fn a_viewer_cannot_change_containers() {
    with_db!(|pool: PgPool| async move {
        // A VIEWER in the same tenant: this is an authorisation check, not an
        // isolation one.
        let owner = admin_token(&pool).await;
        let email = format!("viewer-{}@example.com", Uuid::new_v4().simple());
        let hash = forge_auth::hash_password("viewer-password-123").unwrap();
        let user_id: Uuid =
            sqlx::query_scalar("INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3) RETURNING id")
                .bind(Uuid::new_v4())
                .bind(&email)
                .bind(&hash)
                .fetch_one(&pool)
                .await
                .unwrap();
        let tenant_id: Uuid =
            sqlx::query_scalar("SELECT id FROM tenants LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'VIEWER')",
        )
        .bind(user_id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
        let viewer = forge_auth::JwtService::new(SECRET.as_bytes())
            .issue(user_id, tenant_id, forge_auth::Role::Viewer)
            .unwrap();

        let app = router(pool.clone());

        // Reading is allowed.
        let (status, _) = send(&app, "GET", "/api/v1/applications", Some(&viewer), None).await;
        assert_eq!(status, 200);

        // Creating is not.
        let (status, refused) = send(
            &app,
            "POST",
            "/api/v1/applications",
            Some(&viewer),
            Some(serde_json::json!({ "name": "Nope" })),
        )
        .await;
        assert_eq!(status, 403, "{refused}");

        // And an environment needs `settings:write`, which VIEWER lacks.
        let (status, refused) = send(
            &app,
            "POST",
            "/api/v1/environments",
            Some(&viewer),
            Some(serde_json::json!({ "name": "Nope", "kind": "staging" })),
        )
        .await;
        assert_eq!(status, 403, "{refused}");

        let _ = owner;
    })
    .await;
}