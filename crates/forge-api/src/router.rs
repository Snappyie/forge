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

// Dummy shared state
pub struct AppState {
    pub version: String,
}

pub fn create_router() -> Router {
    let state = Arc::new(AppState {
        version: "1.0".to_string(),
    });

    Router::new()
        .route("/api/v1/health/live", get(health_live))
        .route("/api/v1/jobs", post(create_job).get(list_jobs))
        .with_state(state)
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
async fn create_job(Json(payload): Json<CreateJobPayload>) -> impl IntoResponse {
    // In reality, this would use forge-domain::Job and save via forge-storage
    let response = ApiResponse {
        data: serde_json::json!({
            "id": Uuid::new_v4(),
            "name": payload.name,
            "status": "Draft",
        }),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::CREATED, Json(response))
}

// GET /api/v1/jobs
async fn list_jobs() -> impl IntoResponse {
    let response = ApiResponse {
        data: serde_json::json!([]),
        request_id: Uuid::new_v4().to_string(),
    };
    (StatusCode::OK, Json(response))
}
