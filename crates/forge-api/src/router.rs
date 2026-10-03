use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;

// Standard Success Envelope (From 05-api-specification.md)
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
    pub request_id: String,
}

// Standard Error Envelope
#[derive(Serialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorDetail,
}

#[derive(Serialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    pub request_id: String,
}

// Shared state
#[derive(Clone)]
pub struct AppState {
    pub version: String,
    pub pool: sqlx::PgPool,
    /// JWT signing/verification service. Built once at startup from the session
    /// secret so the auth middleware never re-reads the environment per request.
    pub jwt: Arc<forge_auth::JwtService>,
}

use axum::middleware;
use crate::middleware::auth::require_auth;
use forge_auth::Role;
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
    GovernorLayer,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::cors::{CorsLayer, Any};

pub fn create_router(pool: sqlx::PgPool, session_secret: &str) -> Router {
    let state = Arc::new(AppState {
        version: "1.0".to_string(),
        pool,
        jwt: Arc::new(forge_auth::JwtService::new(session_secret)),
    });

    // Rate limiting, keyed on the authenticated tenant when available and on
    // the peer IP otherwise.
    //
    // The governor's default `PeerIpKeyExtractor` requires a `ConnectInfo`
    // request extension. `axum::serve` only inserts one when the router is
    // served `into_make_service_with_connect_info`, which the previous wiring
    // did not do — every request therefore failed the extractor and returned
    // 500. `SmartIpKeyExtractor` reads `ConnectInfo` when present and degrades
    // to a constant key otherwise, so the limiter works in both cases.
    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(100)
            .burst_size(200)
            .finish()
            .unwrap(),
    );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Endpoints that require authentication.
    // `from_fn_with_state` is required because the middleware reads the shared
    // `AppState` (for the JWT service) and the sub-router does not itself
    // carry that state type until the outer router injects it.
    let api_routes = Router::new()
        .route("/api/v1/invites", post(create_invite))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Public endpoints (health, invite acceptance which handles its own auth/token verification)
    Router::new()
        .route("/api/v1/health/live", get(health_live))
        .route("/api/v1/jobs", post(create_job).get(list_jobs)) // TEMPORARY for UI wireup testing
        .route("/api/v1/workers", get(list_workers)) // TEMPORARY for UI wireup testing
        .route("/api/v1/invites/accept", post(accept_invite))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .merge(api_routes)
        // Apply Rate Limiting globally across all routes
        .layer(ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(cors)
            .layer(GovernorLayer {
                config: Arc::new(*governor_conf),
            })
        )
        .with_state(state)
}

#[derive(Deserialize)]
pub struct CreateInvitePayload {
    pub email: String,
    pub role: Role,
}

// POST /api/v1/invites (Requires Auth, requires TenantAdmin)
async fn create_invite(
    // In reality, we'd extract the Claims from the Extension here to verify the inviter is a TenantAdmin
    Json(payload): Json<CreateInvitePayload>,
) -> impl IntoResponse {
    let response = ApiResponse {
        data: serde_json::json!({
            "invite_token": Uuid::new_v4(),
            "email": payload.email,
            "role": payload.role,
        }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::CREATED, Json(response))
}

#[derive(Deserialize)]
pub struct AcceptInvitePayload {
    pub token: String,
}

// POST /api/v1/invites/accept
async fn accept_invite(Json(_payload): Json<AcceptInvitePayload>) -> impl IntoResponse {
    // In reality, this endpoint verifies the invite token, creates a TenantMembership, 
    // and returns a newly minted JWT.
    let response = ApiResponse {
        data: serde_json::json!({
            "status": "accepted",
            "message": "TenantMembership created. You can now request a JWT."
        }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::OK, Json(response))
}

#[derive(Deserialize)]
pub struct AuthPayload {
    pub email: String,
    pub password: String, // Would be hashed with argon2 in actual handler
}

// POST /api/v1/auth/register
async fn register(Json(_payload): Json<AuthPayload>) -> impl IntoResponse {
    // 1. Hash password via argon2
    // 2. Insert into users table
    // 3. Return success
    let response = ApiResponse {
        data: serde_json::json!({"message": "User registered successfully"}),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::CREATED, Json(response))
}

// POST /api/v1/auth/login
async fn login(Json(_payload): Json<AuthPayload>) -> impl IntoResponse {
    // 1. Fetch user by email
    // 2. Verify argon2 hash
    // 3. Issue short-lived JWT access_token
    // 4. Issue long-lived refresh_token in DB
    let response = ApiResponse {
        data: serde_json::json!({
            "access_token": "mock.jwt.token",
            "refresh_token": Uuid::new_v4(),
        }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::OK, Json(response))
}

#[derive(Deserialize)]
pub struct RefreshPayload {
    pub refresh_token: Uuid,
}

// POST /api/v1/auth/refresh
async fn refresh(Json(_payload): Json<RefreshPayload>) -> impl IntoResponse {
    // 1. Lookup refresh token in DB
    // 2. Issue new access_token
    // 3. Rotate refresh_token (invalidate old, issue new)
    let response = ApiResponse {
        data: serde_json::json!({
            "access_token": "mock.jwt.token",
            "refresh_token": Uuid::new_v4(),
        }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::OK, Json(response))
}

// GET /api/v1/health/live
async fn health_live() -> impl IntoResponse {
    let response = ApiResponse {
        data: serde_json::json!({ "status": "ok" }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::OK, Json(response))
}

#[derive(Deserialize)]
pub struct CreateJobPayload {
    pub name: String,
    pub tenant_id: String,
}

/// Row shape for `jobs`. Runtime queries with `#[derive(FromRow)]` keep the
/// workspace buildable without a live database or a committed `.sqlx` cache.
#[derive(FromRow, Serialize)]
pub struct JobRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// POST /api/v1/jobs
async fn create_job(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(payload): Json<CreateJobPayload>
) -> Response {
    let tenant_id = Uuid::parse_str(&payload.tenant_id).unwrap_or_else(|_| Uuid::new_v4());
    let job_id = Uuid::new_v4();
    let status = "Draft";

    let query_result = sqlx::query_as::<_, JobRow>(
        "INSERT INTO jobs (id, tenant_id, name, status) VALUES ($1, $2, $3, $4)
         RETURNING id, tenant_id, name, status, created_at, updated_at",
    )
    .bind(job_id)
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(status)
    .fetch_one(&state.pool)
    .await;

    match query_result {
        Ok(job) => {
            let response = ApiResponse {
                data: job,
                request_id: Uuid::new_v4().to_string(),
            };
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            let err_response = ApiErrorResponse {
                error: ApiErrorDetail {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("Database error: {}", e),
                    request_id: Uuid::new_v4().to_string(),
                },
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err_response)).into_response()
        }
    }
}

// GET /api/v1/jobs
async fn list_jobs(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> Response {
    let records = sqlx::query_as::<_, JobRow>(
        "SELECT id, tenant_id, name, status, created_at, updated_at
         FROM jobs ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await;

    match records {
        Ok(rows) => {
            let response = ApiResponse {
                data: rows,
                request_id: Uuid::new_v4().to_string(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            let err_response = ApiErrorResponse {
                error: ApiErrorDetail {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("Database error: {}", e),
                    request_id: Uuid::new_v4().to_string(),
                },
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err_response)).into_response()
        }
    }
}

/// Row shape for `workers`.
#[derive(FromRow, Serialize)]
pub struct WorkerRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub hostname: String,
    pub status: String,
    pub last_heartbeat_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// GET /api/v1/workers
async fn list_workers(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> Response {
    let records = sqlx::query_as::<_, WorkerRow>(
        "SELECT id, tenant_id, hostname, status, last_heartbeat_at, created_at
         FROM workers ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await;

    match records {
        Ok(rows) => {
            let response = ApiResponse {
                data: rows,
                request_id: Uuid::new_v4().to_string(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            let err_response = ApiErrorResponse {
                error: ApiErrorDetail {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("Database error: {}", e),
                    request_id: Uuid::new_v4().to_string(),
                },
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(err_response)).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
    };
    use std::net::SocketAddr;
    use tower::ServiceExt; // for `oneshot`

    // The router needs a pool but these tests only exercise routes that never
    // touch the database, so a lazily-connected pool is enough.
    fn test_router() -> Router {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://forge:forgepassword@localhost:5432/forgedb")
            .expect("lazy pool");
        create_router(pool, "test_secret_not_used_in_production")
    }

    /// The governor rate limiter keys on the peer IP, so requests must carry a
    /// `ConnectInfo` extension. The real server gets this from `axum::serve`.
    fn get_request(uri: &str) -> Request<Body> {
        let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        req.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:1234".parse::<SocketAddr>().unwrap(),
        ));
        req
    }

    fn post_request(uri: &str, body: &'static str) -> Request<Body> {
        let mut req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:1234".parse::<SocketAddr>().unwrap(),
        ));
        req
    }

    #[tokio::test]
    async fn test_health_live() {
        let response = test_router().oneshot(get_request("/api/v1/health/live")).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_invites_require_auth() {
        // /api/v1/invites sits behind require_auth, so an unauthenticated call
        // must be rejected before any handler runs.
        let response = test_router()
            .oneshot(post_request(
                "/api/v1/invites",
                r#"{"email":"a@b.com","role":"Developer"}"#,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}