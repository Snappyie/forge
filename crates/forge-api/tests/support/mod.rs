//! Shared harness for the acceptance-matrix tests.
//!
//! Extracted rather than duplicated so the matrix exercises the same setup path
//! as `api_integration.rs`. A matrix with its own subtly different harness is a
//! matrix that can pass while the real one fails.

use axum::http::Request;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};
use tower::ServiceExt;
use uuid::Uuid;

pub const SECRET: &str = "acceptance-test-secret-not-for-production";
/// A stand-in for `FORGE_API_KEY_HASHING_SECRET`.
pub const API_KEY_PEPPER: &[u8] = b"acceptance-test-pepper";

/// A throwaway database, dropped on `cleanup`.
pub struct TestDb {
    pub pool: PgPool,
    admin_url: String,
    db_name: String,
}

impl TestDb {
    pub async fn new() -> Option<TestDb> {
        let base = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://forge:forgepassword@localhost:5432/forgedb".into());
        let db_name = format!("forge_accept_{}", Uuid::new_v4().simple());
        let (server, _) = base.rsplit_once('/').unwrap_or((base.as_str(), ""));
        let admin_url = format!("{}/postgres", server.trim_end_matches('/'));

        let admin = match PgPoolOptions::new().max_connections(1).connect(&admin_url).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping acceptance tests: cannot connect ({e})");
                return None;
            }
        };
        if admin
            .execute(sqlx::AssertSqlSafe(format!(r#"CREATE DATABASE "{db_name}""#)))
            .await
            .is_err()
        {
            eprintln!("skipping acceptance tests: cannot create database");
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
                eprintln!("skipping acceptance tests: cannot connect ({e})");
                return None;
            }
        };
        if let Err(e) = sqlx::migrate!("../forge-storage/migrations").run(&pool).await {
            eprintln!("skipping acceptance tests: migrations failed ({e})");
            return None;
        }

        Some(TestDb {
            pool,
            admin_url,
            db_name,
        })
    }

    /// The database's name, so a test can build a second connection string.
    pub fn db_name(&self) -> &str {
        &self.db_name
    }

    /// Drops the throwaway database.
    ///
    /// Failures are reported rather than swallowed: a silently skipped drop
    /// accumulates databases on every run until the server refuses to create more.
    pub async fn cleanup(self) {
        self.pool.close().await;
        let admin = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("could not drop {}: {e}", self.db_name);
                return;
            }
        };
        if let Err(e) = admin
            .execute(sqlx::AssertSqlSafe(format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                     WHERE datname = '{}' AND pid <> pg_backend_pid()",
                self.db_name
            )))
            .await
        {
            eprintln!("could not release connections to {}: {e}", self.db_name);
        }
        if let Err(e) = admin
            .execute(sqlx::AssertSqlSafe(format!(
                r#"DROP DATABASE IF EXISTS "{}""#,
                self.db_name
            )))
            .await
        {
            eprintln!("could not drop {}: {e}", self.db_name);
        }
        admin.close().await;
    }
}

/// Builds a router with test configuration.
pub fn router(pool: PgPool) -> Router {
    forge_api::create_router(
        pool,
        SECRET,
        "http://localhost:3000/api/v1",
        true,
        1024 * 1024,
        4 * 1024 * 1024,
        API_KEY_PEPPER,
    )
}

/// Issues a request and returns its status and parsed body.
///
/// `ConnectInfo` is attached because the rate limiter needs it; without it the
/// router rejects the request before any handler runs.
pub async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let payload = match body {
        Some(value) => axum::body::Body::from(value.to_string()),
        None => axum::body::Body::empty(),
    };
    let mut req = builder.body(payload).unwrap();
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:4000".parse::<std::net::SocketAddr>().unwrap(),
    ));

    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 4 << 20)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}

/// Creates an isolated tenant with an OWNER and returns a usable token.
///
/// The user is written directly rather than through `/auth/register`, because
/// that endpoint grants VIEWER (or claims the one-time bootstrap slot) and would
/// make every test in the matrix contend for the same single tenant.
pub async fn register_via_api(pool: &PgPool) -> (String, String) {
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Acceptance {}", tenant_id.simple()))
        .execute(pool)
        .await
        .expect("tenant insert");

    let user_id = Uuid::new_v4();
    let email = format!("user-{}@example.com", Uuid::new_v4().simple());
    let password = "AcceptancePassword123!";

    let hash = forge_auth::hash_password(password).expect("password hash");
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $2)",
    )
    .bind(user_id)
    .bind(&email)
    .bind(&hash)
    .execute(pool)
    .await
    .expect("user insert");

    sqlx::query(
        "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'OWNER')",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("membership insert");

    let token = forge_auth::JwtService::new(SECRET.as_bytes())
        .issue(user_id, tenant_id, forge_auth::Role::Owner)
        .expect("token");
    (token, tenant_id.to_string())
}

/// Issues a request and returns only its status code.
pub async fn send_status(
    app: &Router,
    token: &str,
    method: &str,
    path: &str,
    body: serde_json::Value,
) -> u16 {
    send(app, method, path, Some(token), Some(body)).await.0
}

/// Issues a request, returning `None` when the response carries no data.
pub async fn send_optional(
    app: &Router,
    token: &str,
    method: &str,
    path: &str,
    body: serde_json::Value,
) -> Option<serde_json::Value> {
    let (status, value) = send(app, method, path, Some(token), Some(body)).await;
    if status >= 300 {
        return None;
    }
    value.get("data").filter(|d| !d.is_null()).cloned()
}
