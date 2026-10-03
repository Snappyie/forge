use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
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
}

use axum::middleware;
use crate::middleware::auth::require_auth;
use forge_auth::Role;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::cors::{CorsLayer, Any};

pub fn create_router(pool: sqlx::PgPool) -> Router {
    let state = Arc::new(AppState {
        version: "1.0".to_string(),
        pool,
    });

    // Configure rate limiting (e.g. 100 requests per second per IP)
    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(10)
            .finish()
            .unwrap(),
    );

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Endpoints that require authentication
    let api_routes = Router::new()
        .route("/api/v1/invites", post(create_invite))
        .route_layer(middleware::from_fn(require_auth));

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

// POST /api/v1/jobs
async fn create_job(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(payload): Json<CreateJobPayload>
) -> impl IntoResponse {
    let tenant_id = Uuid::parse_str(&payload.tenant_id).unwrap_or_else(|_| Uuid::new_v4());
    let job_id = Uuid::new_v4();
    let status = "Draft";

    let query_result = sqlx::query!(
        "INSERT INTO jobs (id, tenant_id, name, status) VALUES ($1, $2, $3, $4)",
        job_id,
        tenant_id,
        payload.name,
        status
    )
    .execute(&state.pool)
    .await;

    if let Err(e) = query_result {
        let err_response = ApiErrorResponse {
            error: ApiErrorDetail {
                code: "INTERNAL_ERROR".to_string(),
                message: format!("Database error: {}", e),
                request_id: Uuid::new_v4().to_string(),
            }
        };
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!(err_response)));
    }

    let response = ApiResponse {
        data: serde_json::json!({
            "id": job_id,
            "name": payload.name,
            "status": status,
        }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::CREATED, Json(serde_json::json!(response)))
}

// GET /api/v1/jobs
async fn list_jobs(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> impl IntoResponse {
    let records = sqlx::query!("SELECT id, tenant_id, name, status, created_at, updated_at FROM jobs ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await;
        
    match records {
        Ok(rows) => {
            let jobs: Vec<_> = rows.into_iter().map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "tenant_id": row.tenant_id,
                    "name": row.name,
                    "status": row.status,
                    "created_at": row.created_at,
                    "updated_at": row.updated_at,
                })
            }).collect();
            
            let response = ApiResponse {
                data: serde_json::json!(jobs),
                request_id: Uuid::new_v4().to_string(),
            };
            (StatusCode::OK, Json(serde_json::json!(response)))
        },
        Err(e) => {
            let err_response = ApiErrorResponse {
                error: ApiErrorDetail {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("Database error: {}", e),
                    request_id: Uuid::new_v4().to_string(),
                }
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!(err_response)))
        }
    }
}

// GET /api/v1/workers
async fn list_workers(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>
) -> impl IntoResponse {
    let records = sqlx::query!("SELECT id, tenant_id, hostname, status, last_heartbeat_at, created_at FROM workers ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await;
        
    match records {
        Ok(rows) => {
            let workers: Vec<_> = rows.into_iter().map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "tenant_id": row.tenant_id,
                    "hostname": row.hostname,
                    "status": row.status,
                    "last_heartbeat_at": row.last_heartbeat_at,
                    "created_at": row.created_at,
                })
            }).collect();
            
            let response = ApiResponse {
                data: serde_json::json!(workers),
                request_id: Uuid::new_v4().to_string(),
            };
            (StatusCode::OK, Json(serde_json::json!(response)))
        },
        Err(e) => {
            let err_response = ApiErrorResponse {
                error: ApiErrorDetail {
                    code: "INTERNAL_ERROR".to_string(),
                    message: format!("Database error: {}", e),
                    request_id: Uuid::new_v4().to_string(),
                }
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!(err_response)))
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use axum::{
//         body::Body,
//         extract::ConnectInfo,
//         http::{Request, StatusCode},
//     };
//     use std::net::SocketAddr;
//     use tower::ServiceExt; // for `oneshot`
// 
//     fn add_ip<T>(mut req: Request<T>) -> Request<T> {
//         let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
//         req.extensions_mut().insert(ConnectInfo(addr));
//         req
//     }
// 
//     #[tokio::test]
//     async fn test_health_live() {
//         // ...
//     }
// }
