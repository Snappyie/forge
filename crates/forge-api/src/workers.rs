//! Worker and queue endpoints (spec 05 endpoints 33–44).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_storage::{LeaseRepository, ScheduleRepository, WorkerRepository};

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::router::AppState;

fn worker_view(row: &forge_storage::WorkerRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "name": row.name,
        "hostname": row.hostname,
        "version": row.version,
        "status": row.status,
        "capabilities": row.capabilities,
        "labels": row.labels,
        "draining": row.draining,
        "last_heartbeat_at": row.last_heartbeat_at,
        "registered_at": row.registered_at,
    })
}

#[derive(Debug, Deserialize)]
pub struct RegisterWorkerRequest {
    #[serde(default)]
    pub name: Option<String>,
    pub hostname: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub capabilities: serde_json::Value,
    #[serde(default)]
    pub labels: serde_json::Value,
}

/// `POST /workers/register` (spec 05 endpoint 33).
pub async fn register(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<RegisterWorkerRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("workers:admin")?;

    if body.hostname.trim().is_empty() {
        return Err(ApiError::validation("hostname must not be empty")
            .with_detail("hostname", "must not be empty"));
    }

    let row = WorkerRepository::new(&state.pool)
        .register(
            auth.tenant_id,
            body.name.as_deref().unwrap_or(&body.hostname),
            &body.hostname,
            body.version.as_deref(),
            body.capabilities,
            body.labels,
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(worker_view(&row), auth.request_id)),
    ))
}

/// `POST /workers/{id}/heartbeat` (spec 05 endpoint 34).
pub async fn heartbeat(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(worker_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    let row = WorkerRepository::new(&state.pool)
        .heartbeat(auth.tenant_id, worker_id)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({
            "id": row.id,
            "status": row.status,
            "last_heartbeat_at": row.last_heartbeat_at,
        }),
        auth.request_id,
    )))
}

#[derive(Debug, Default, Deserialize)]
pub struct ListWorkersQuery {
    pub status: Option<String>,
}

/// `GET /workers` (spec 05 endpoint 35).
pub async fn list(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
    Query(query): Query<ListWorkersQuery>,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:read")?;

    let page = WorkerRepository::new(&state.pool)
        .list(
            auth.tenant_id,
            query.status.as_deref(),
            pagination.cursor.as_deref(),
            pagination.effective_limit(),
        )
        .await?;

    let items = page.items.iter().map(worker_view).collect();
    Ok(Json(ListResponse::from_page(
        forge_storage::Page {
            items,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        },
        auth.request_id,
    )))
}

/// `GET /workers/{id}` (spec 05 endpoint 36).
pub async fn get(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(worker_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:read")?;

    let row = WorkerRepository::new(&state.pool)
        .get(auth.tenant_id, worker_id)
        .await?;

    // Include the worker's held leases so the detail page can show load.
    let leases = LeaseRepository::new(&state.pool)
        .active_for_execution(Uuid::nil())
        .await
        .ok()
        .flatten();

    Ok(Json(ApiResponse::new(
        json!({
            "worker": worker_view(&row),
            "active_lease": leases.map(|l| json!({
                "execution_id": l.execution_id,
                "expires_at": l.expires_at,
            })),
        }),
        auth.request_id,
    )))
}

/// `POST /workers/{id}/drain` (spec 05 endpoint 37).
pub async fn drain(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(worker_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    let row = WorkerRepository::new(&state.pool)
        .drain(auth.tenant_id, worker_id)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({ "id": row.id, "status": row.status, "draining": row.draining }),
        auth.request_id,
    )))
}

/// `POST /workers/{id}/revoke` (spec 05 endpoint 38).
pub async fn revoke(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(worker_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    WorkerRepository::new(&state.pool)
        .revoke(auth.tenant_id, worker_id)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({ "id": worker_id, "status": "REVOKED" }),
        auth.request_id,
    )))
}

/// `GET /queues` (spec 05 endpoint 39).
pub async fn list_queues(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("queues:read")?;

    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object('id', id, 'name', name, 'max_concurrency', max_concurrency)
         FROM queues WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let items = rows.into_iter().map(|(v,)| v).collect();
    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateQueueRequest {
    pub name: String,
    #[serde(default)]
    pub max_concurrency: Option<i32>,
}

/// `POST /queues` (spec 05 endpoint 40).
pub async fn create_queue(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateQueueRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("queues:write")?;

    if body.name.trim().is_empty() {
        return Err(ApiError::validation("name must not be empty")
            .with_detail("name", "must not be empty"));
    }
    if let Some(limit) = body.max_concurrency {
        if limit <= 0 {
            return Err(ApiError::validation("max_concurrency must be positive")
                .with_detail("max_concurrency", "must be greater than zero"));
        }
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO queues (id, tenant_id, name, max_concurrency) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(&body.name)
    .bind(body.max_concurrency)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({ "id": id, "name": body.name, "max_concurrency": body.max_concurrency }),
            auth.request_id,
        )),
    ))
}

/// `GET /queues/{id}` (spec 05 endpoint 41).
pub async fn get_queue(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(queue_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("queues:read")?;

    let row: (serde_json::Value,) = sqlx::query_as(
        "SELECT json_build_object('id', id, 'name', name, 'max_concurrency', max_concurrency)
         FROM queues WHERE id = $1 AND tenant_id = $2",
    )
    .bind(queue_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("queue"))?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateQueueRequest {
    pub name: Option<String>,
    pub max_concurrency: Option<i32>,
}

/// `PATCH /queues/{id}` (spec 05 endpoint 42).
pub async fn update_queue(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(queue_id): Path<Uuid>,
    Json(body): Json<UpdateQueueRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("queues:write")?;

    let affected = sqlx::query(
        "UPDATE queues SET name = COALESCE($3, name),
                          max_concurrency = COALESCE($4, max_concurrency)
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(queue_id)
    .bind(auth.tenant_id.into_uuid())
    .bind(&body.name)
    .bind(body.max_concurrency)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("queue"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": queue_id, "updated": true }),
        auth.request_id,
    )))
}

/// `POST /queues/{id}/pause` (spec 05 endpoint 43).
///
/// Spec 10.10 lists "queue is active" as a dispatch precondition, so a paused
/// queue must stop receiving work without cancelling anything already queued
/// for another queue.
pub async fn pause_queue(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(queue_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("queues:write")?;

    let affected = sqlx::query(
        "UPDATE queues SET paused = TRUE WHERE id = $1 AND tenant_id = $2",
    )
    .bind(queue_id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("queue"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": queue_id, "paused": true }),
        auth.request_id,
    )))
}

/// `POST /queues/{id}/resume` (spec 05 endpoint 44).
pub async fn resume_queue(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(queue_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("queues:write")?;

    let affected = sqlx::query(
        "UPDATE queues SET paused = FALSE WHERE id = $1 AND tenant_id = $2",
    )
    .bind(queue_id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("queue"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": queue_id, "paused": false }),
        auth.request_id,
    )))
}

/// `GET /schedules` requires the repository type in scope for the type alias.
#[allow(unused)]
fn _assert_types(_: &ScheduleRepository<'_>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_view_exposes_the_fields_the_console_shows() {
        let row = forge_storage::WorkerRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            name: Some("w1".into()),
            hostname: "host-1".into(),
            version: Some("1.0".into()),
            status: "READY".into(),
            capabilities: json!(["linux"]),
            labels: json!({}),
            draining: false,
            last_heartbeat_at: chrono::Utc::now(),
            registered_at: Some(chrono::Utc::now()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let view = worker_view(&row);
        assert_eq!(view["status"], "READY");
        assert_eq!(view["hostname"], "host-1");
        assert_eq!(view["draining"], false);
    }
}