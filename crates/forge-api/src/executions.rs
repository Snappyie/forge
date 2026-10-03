//! Execution endpoints (spec 05 endpoints 17–23).

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_domain::{ErrorClass, ExecutionStatus};
use forge_storage::{ExecutionFilter, ExecutionRepository, LeaseRepository};

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::router::AppState;

/// Filters for `GET /executions` (spec 05 endpoint 17).
#[derive(Debug, Default, Deserialize)]
pub struct ListExecutionsQuery {
    pub job: Option<Uuid>,
    pub workflow: Option<Uuid>,
    pub status: Option<String>,
    pub worker: Option<Uuid>,
    pub queue: Option<Uuid>,
    pub correlation_id: Option<String>,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_filter(query: &ListExecutionsQuery) -> Result<ExecutionFilter, ApiError> {
    let status = match query.status.as_deref() {
        None => None,
        Some(raw) => Some(
            raw.parse::<ExecutionStatus>()
                .map_err(|_| ApiError::validation(format!("`{raw}` is not a valid status"))
                    .with_detail("status", "unknown execution status"))?,
        ),
    };
    Ok(ExecutionFilter {
        job_id: query.job,
        workflow_id: query.workflow,
        status,
        worker_id: query.worker,
        queue_id: query.queue,
        correlation_id: query.correlation_id.clone(),
        created_after: query.created_after,
        created_before: query.created_before,
    })
}

/// `GET /executions` (spec 05 endpoint 17).
pub async fn list(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:read")?;

    let page = ExecutionRepository::new(&state.pool)
        .list(
            auth.tenant_id,
            &parse_filter(&query)?,
            pagination.cursor.as_deref(),
            pagination.effective_limit(),
        )
        .await?;

    let items = page.items.iter().map(crate::jobs::execution_view).collect();
    Ok(Json(ListResponse::from_page(
        forge_storage::Page {
            items,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        },
        auth.request_id,
    )))
}

/// `GET /executions/{id}` (spec 05 endpoint 18).
pub async fn get(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:read")?;

    let row = ExecutionRepository::new(&state.pool)
        .get(auth.tenant_id, execution_id)
        .await?;

    Ok(Json(ApiResponse::new(
        crate::jobs::execution_view(&row),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/cancel` (spec 05 endpoint 19).
///
/// Spec 10.6: cancellation is cooperative. The server records the request; the
/// worker observes it and stops. A terminal execution cannot be cancelled, and
/// the domain state machine refuses.
pub async fn cancel(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:cancel")?;

    let row = ExecutionRepository::new(&state.pool)
        .request_cancel(auth.tenant_id, execution_id)
        .await?;

    Ok(Json(ApiResponse::new(
        crate::jobs::execution_view(&row),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize, Default)]
pub struct RetryRequest {
    /// Override the recorded error class, which decides whether a retry is
    /// even permitted.
    #[serde(default)]
    pub error_class: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /executions/{id}/retry` (spec 05 endpoint 20).
///
/// The attempt is re-armed through the domain state machine, so a retry of a
/// terminal execution must pass `ABANDONED`/`FAILED -> RETRY_SCHEDULED` first and
/// the policy must still permit another attempt.
pub async fn retry(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Json(body): Json<RetryRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:retry")?;

    let error_class = match body.error_class.as_deref() {
        None => ErrorClass::Transient,
        Some(raw) => raw.parse::<ErrorClass>().map_err(|_| {
            ApiError::validation(format!("`{raw}` is not a valid error class"))
                .with_detail("error_class", "unknown error class")
        })?,
    };

    let repo = ExecutionRepository::new(&state.pool);
    let row = repo
        .transition(
            auth.tenant_id,
            execution_id,
            ExecutionStatus::RetryScheduled,
            Some(error_class),
            body.reason.as_deref(),
        )
        .await?;

    Ok(Json(ApiResponse::new(
        crate::jobs::execution_view(&row),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/dead-letter` (spec 05 endpoint 21).
pub async fn dead_letter(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:retry")?;

    let row = ExecutionRepository::new(&state.pool)
        .transition(
            auth.tenant_id,
            execution_id,
            ExecutionStatus::DeadLettered,
            None,
            Some("dead-lettered by an operator"),
        )
        .await?;

    Ok(Json(ApiResponse::new(
        crate::jobs::execution_view(&row),
        auth.request_id,
    )))
}

/// `GET /executions/{id}/attempts` (spec 05 endpoint 22).
pub async fn list_attempts(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:read")?;

    // Confirm the execution is visible to this tenant before reading its
    // attempts, so the query cannot be used to probe another tenant's ids.
    ExecutionRepository::new(&state.pool)
        .get(auth.tenant_id, execution_id)
        .await?;

    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'attempt_number', attempt_number,
             'worker_id', worker_id,
             'status', status,
             'started_at', started_at,
             'ended_at', ended_at,
             'exit_code', exit_code,
             'error_class', error_class,
             'error_message', error_message
         )
         FROM execution_attempts
         WHERE execution_id = $1 AND tenant_id = $2
         ORDER BY attempt_number ASC",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let items = rows.into_iter().map(|(value,)| value).collect();
    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

/// `GET /executions/{id}/logs` (spec 05 endpoint 23).
pub async fn logs(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:read")?;

    ExecutionRepository::new(&state.pool)
        .get(auth.tenant_id, execution_id)
        .await?;

    let rows: Vec<(String, String, chrono::DateTime<chrono::Utc>,)> = sqlx::query_as(
        "SELECT stream, content, logged_at FROM execution_logs
         WHERE execution_id = $1 AND tenant_id = $2
         ORDER BY logged_at ASC",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let lines: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(stream, content, at)| json!({ "stream": stream, "content": content, "at": at }))
        .collect();

    Ok(Json(ApiResponse::new(
        json!({ "execution_id": execution_id, "lines": lines }),
        auth.request_id,
    )))
}

/// `POST /workers/{worker_id}/claim` — worker protocol (spec 10.1).
///
/// This endpoint is absent from the spec 05 table but required by the protocol
/// in spec 10.1; the addition is recorded in an ADR.
pub async fn claim(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(worker_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    let executions = ExecutionRepository::new(&state.pool);
    let Some(row) = executions
        .claim_next(auth.tenant_id, None, worker_id)
        .await?
    else {
        return Ok(Json(ApiResponse::new(
            json!({ "execution": null }),
            auth.request_id,
        )));
    };

    // A lease is taken at claim time so ownership is exclusive from the outset.
    let lease = LeaseRepository::new(&state.pool)
        .acquire(auth.tenant_id, row.id, worker_id, None, 20)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({
            "execution": crate::jobs::execution_view(&row),
            "lease": { "id": lease.id, "expires_at": lease.expires_at },
        }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub lease_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CompleteRequest {
    pub lease_id: Uuid,
    #[serde(default)]
    pub succeeded: bool,
    #[serde(default)]
    pub error_class: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub output: serde_json::Value,
}

/// `POST /executions/{id}/heartbeat` — worker protocol (spec 10.2).
pub async fn heartbeat(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Json(body): Json<HeartbeatRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    // The lease must belong to *this* execution. Renewing without the check
    // would let a worker present any lease it holds and keep another
    // execution looking alive.
    let lease = LeaseRepository::new(&state.pool)
        .active_for_execution(execution_id)
        .await?
        .ok_or_else(|| {
            ApiError::conflict("this execution has no active lease")
        })?;

    if lease.id != body.lease_id {
        return Err(ApiError::conflict(
            "the supplied lease does not belong to this execution",
        ));
    }

    let renewed = LeaseRepository::new(&state.pool)
        .renew(body.lease_id, auth.user_id, 20)
        .await
        .map_err(|_| {
            // Spec 10.3: only the holder may renew. A refusal must not say
            // whether the lease exists elsewhere.
            ApiError::conflict("the lease is not held by this worker, or has expired")
        })?;

    Ok(Json(ApiResponse::new(
        json!({ "lease_id": renewed.id, "expires_at": renewed.expires_at }),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/complete` — worker protocol (spec 10.4).
///
/// Spec 10.5: a completion carrying a stale lease must not overwrite state that
/// recovery has since moved on.
pub async fn complete(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Json(body): Json<CompleteRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    // Reject a stale completion before touching any state.
    let gate = forge_executor::CompletionGate::new(&state.pool);
    gate.check(execution_id, body.lease_id, auth.user_id)
        .await
        .map_err(|rejection| match rejection {
            forge_executor::CompletionRejection::LeaseExpired => {
                ApiError::conflict("the lease has expired; the result is stale")
            }
            forge_executor::CompletionRejection::ExecutionTerminal => {
                ApiError::conflict("the execution has already finished")
            }
            other => ApiError::conflict(format!(
                "the completion was rejected: {other:?}"
            )),
        })?;

    let repo = ExecutionRepository::new(&state.pool);
    let row = if body.succeeded {
        repo.transition(auth.tenant_id, execution_id, ExecutionStatus::Succeeded, None, None)
            .await?
    } else {
        let class = match body.error_class.as_deref() {
            None => ErrorClass::Permanent,
            Some(raw) => raw.parse::<ErrorClass>().map_err(|_| {
                ApiError::validation(format!("`{raw}` is not a valid error class"))
                    .with_detail("error_class", "unknown error class")
            })?,
        };
        repo.transition(
            auth.tenant_id,
            execution_id,
            ExecutionStatus::Failed,
            Some(class),
            body.error_message.as_deref(),
        )
        .await?
    };

    // The lease is released so the work is not reaped while finishing.
    let _ = LeaseRepository::new(&state.pool)
        .release(body.lease_id, auth.user_id)
        .await;

    Ok(Json(ApiResponse::new(
        crate::jobs::execution_view(&row),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/dispatch` — administrative dispatch (spec 10.10).
pub async fn dispatch(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(worker_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:admin")?;

    let row = ExecutionRepository::new(&state.pool)
        .claim_next(auth.tenant_id, None, worker_id)
        .await?;

    Ok(Json(ApiResponse::new(
        match row {
            Some(row) => json!({ "execution": crate::jobs::execution_view(&row) }),
            None => json!({ "execution": null }),
        },
        auth.request_id,
    )))
}

/// Ensures a status string is a known execution status.
pub fn is_known_status(raw: &str) -> bool {
    raw.parse::<ExecutionStatus>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn a_status_filter_is_validated() {
        let good: ListExecutionsQuery =
            serde_json::from_str(r#"{"status":"RUNNING"}"#).unwrap();
        assert_eq!(parse_filter(&good).unwrap().status, Some(ExecutionStatus::Running));

        let bad: ListExecutionsQuery =
            serde_json::from_str(r#"{"status":"NOT_A_STATUS"}"#).unwrap();
        let error = parse_filter(&bad).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.details[0].field, "status");
    }

    #[test]
    fn every_status_string_is_recognised() {
        for status in [
            "SCHEDULED", "QUEUED", "DISPATCHED", "RUNNING", "SUCCEEDED", "FAILED",
            "TIMED_OUT", "CANCEL_REQUESTED", "CANCELLED", "RETRY_SCHEDULED",
            "DEAD_LETTERED", "ABANDONED",
        ] {
            assert!(is_known_status(status), "{status} must be accepted");
        }
        assert!(!is_known_status("PENDING"));
    }

    #[test]
    fn execution_filters_pass_through() {
        let job = Uuid::new_v4();
        let worker = Uuid::new_v4();
        let query: ListExecutionsQuery = serde_json::from_str(&format!(
            r#"{{"job":"{job}","worker":"{worker}"}}"#
        ))
        .unwrap();

        let filter = parse_filter(&query).unwrap();
        assert_eq!(filter.job_id, Some(job));
        assert_eq!(filter.worker_id, Some(worker));
    }

    #[test]
    fn a_completion_error_class_is_validated() {
        // Exercised through the same parse the handler uses.
        let good = "TIMEOUT".parse::<ErrorClass>();
        assert!(good.is_ok());
        let bad = "SOMETHING".parse::<ErrorClass>();
        assert!(bad.is_err());
    }
}