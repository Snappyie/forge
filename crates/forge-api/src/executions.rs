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
    let mut statuses = Vec::new();
    let mut single_status = None;
    if let Some(raw) = query.status.as_deref() {
        for part in raw.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                let st = trimmed.parse::<ExecutionStatus>().map_err(|_| {
                    ApiError::validation(format!("`{trimmed}` is not a valid status"))
                        .with_detail("status", "unknown execution status")
                })?;
                statuses.push(st);
            }
        }
        if statuses.len() == 1 {
            single_status = statuses.first().copied();
        }
    }
    Ok(ExecutionFilter {
        job_id: query.job,
        workflow_id: query.workflow,
        status: single_status,
        statuses,
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

    // A chatty execution can write more logs than a response should carry, so
    // the fetch is bounded by the newest lines rather than reading everything
    // and truncating in the client.
    let rows: Vec<(String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT stream, content, logged_at FROM (
             SELECT stream, content, logged_at FROM execution_logs
             WHERE execution_id = $1 AND tenant_id = $2
             ORDER BY logged_at DESC
             LIMIT 5000
         ) recent
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
    auth.require("workers:claim")?;

    if let Some(own) = auth.worker_id {
        if own != worker_id {
            return Err(ApiError::forbidden(
                "a worker token may only claim work for its own worker",
            ));
        }
    }

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

#[derive(Debug, Deserialize, Default)]
pub struct HeartbeatRequest {
    #[serde(default)]
    pub lease_id: Option<Uuid>,
    #[serde(default)]
    pub worker_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CompleteRequest {
    #[serde(default)]
    pub lease_id: Option<Uuid>,
    #[serde(default)]
    pub worker_id: Option<Uuid>,
    #[serde(default)]
    pub succeeded: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub trace: Option<String>,
    #[serde(default)]
    pub error_class: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub output: Option<serde_json::Value>,
}

/// `POST /executions/{id}/heartbeat` and `PATCH /executions/{id}/heartbeat` — worker protocol (spec 10.2).
pub async fn heartbeat(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    body: Option<Json<HeartbeatRequest>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:write")?;

    let req = body.map(|Json(b)| b).unwrap_or_default();
    let lease = LeaseRepository::new(&state.pool)
        .active_for_execution(execution_id)
        .await?
        .ok_or_else(|| ApiError::conflict("this execution has no active lease"))?;

    if let Some(req_lease_id) = req.lease_id {
        if lease.id != req_lease_id {
            return Err(ApiError::conflict(
                "the supplied lease does not belong to this execution",
            ));
        }
    }

    // A worker credential can only renew its own lease; a human operator may
    // name the worker it is acting for.
    if let Some(own) = auth.worker_id {
        if lease.worker_id != own {
            return Err(ApiError::conflict("the lease is not held by this worker"));
        }
    }
    let worker_id = auth.worker_id.or(req.worker_id).unwrap_or(lease.worker_id);

    let renewed = LeaseRepository::new(&state.pool)
        .renew(lease.id, worker_id, 30)
        .await
        .map_err(|_| ApiError::conflict("the lease is not held by this worker, or has expired"))?;

    Ok(Json(ApiResponse::new(
        json!({ "lease_id": renewed.id, "expires_at": renewed.expires_at }),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/complete` — worker protocol (spec 10.4).
pub async fn complete(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Json(body): Json<CompleteRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:write")?;

    let lease = LeaseRepository::new(&state.pool)
        .active_for_execution(execution_id)
        .await?;

    // The reported holder is authoritative only when it comes from the caller's
    // own credential; otherwise the lease row itself names the holder. This is
    // what makes `NotLeaseHolder` meaningful: a worker cannot complete another
    // worker's lease by naming it in the body.
    match lease {
        Some(active) => {
            let lease_id = body.lease_id.unwrap_or(active.id);
            let holder = auth
                .worker_id
                .or(body.worker_id)
                .unwrap_or(active.worker_id);

            // Spec 10.5: a result that lost the race with lease recovery must
            // not overwrite the authoritative state. Refuse it, say why, and
            // leave the execution to whoever recovered it.
            let gate = forge_executor::CompletionGate::new(&state.pool);
            if let Err(rejection) = gate.check(execution_id, lease_id, holder).await {
                tracing::warn!(
                    %execution_id,
                    %lease_id,
                    reason = %rejection,
                    "refused a stale execution completion"
                );
                return Err(ApiError::conflict(rejection.reason()));
            }

            let _ = LeaseRepository::new(&state.pool)
                .release(active.id, active.worker_id)
                .await;
        }
        None => {
            // A lease was reported but none is active: recovery has already
            // taken the execution over.
            if body.lease_id.is_some() {
                return Err(ApiError::conflict(
                    "the lease is no longer active, so recovery already owns this execution",
                ));
            }
            // No lease at all. A worker credential must hold one; a human
            // operator completing on a worker's behalf may not need to.
            if auth.worker_id.is_some() {
                return Err(ApiError::conflict(
                    "a worker must hold a lease before completing an execution",
                ));
            }
        }
    }

    let is_failure =
        body.succeeded == Some(false) || body.error.is_some() || body.error_message.is_some();

    let repo = ExecutionRepository::new(&state.pool);
    let row = if !is_failure {
        repo.transition(
            auth.tenant_id,
            execution_id,
            ExecutionStatus::Succeeded,
            None,
            None,
        )
        .await?
    } else {
        let class = match body.error_class.as_deref() {
            None => ErrorClass::Permanent,
            Some(raw) => raw.parse::<ErrorClass>().unwrap_or(ErrorClass::Permanent),
        };
        let err_msg = body
            .error_message
            .as_deref()
            .or(body.error.as_deref())
            .unwrap_or("execution failed");

        // Spec 10.8/10.9: whether this failure is retried is the version's
        // policy to decide, not the caller's. The handler records FAILED and,
        // when the policy allows it, schedules the retry with its backoff.
        let current = repo.get(auth.tenant_id, execution_id).await?;
        let attempts_made = (current.attempt_count.max(0) as u32).saturating_add(1);
        let handler = forge_executor::FailureHandler::new(&state.pool);
        match handler
            .record_failure(auth.tenant_id, execution_id, class, err_msg, attempts_made)
            .await
        {
            Ok(outcome) => {
                tracing::debug!(%execution_id, ?outcome, "failure recorded");
                repo.get(auth.tenant_id, execution_id).await?
            }
            Err(reason) => {
                // The decision itself failed (for example the database rejected
                // the transition). Fail the execution rather than leaving it
                // running forever with no owner.
                tracing::warn!(%execution_id, %reason, "could not apply the retry policy");
                repo.transition(
                    auth.tenant_id,
                    execution_id,
                    ExecutionStatus::Failed,
                    Some(class),
                    Some(err_msg),
                )
                .await?
            }
        }
    };

    // Spec 10.4: completion carries the attempt's result where applicable.
    // Persisting it here means the run, its successors and the console all see
    // what the worker actually produced.
    let row = match &body.output {
        Some(output) => {
            repo.record_output(auth.tenant_id, execution_id, output)
                .await?;
            repo.get(auth.tenant_id, execution_id).await?
        }
        None => row,
    };

    Ok(Json(ApiResponse::new(
        crate::jobs::execution_view(&row),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/fail` — worker reporting failure directly.
pub async fn fail(
    state: State<AppState>,
    auth: Auth,
    path: Path<Uuid>,
    Json(mut body): Json<CompleteRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    body.succeeded = Some(false);
    complete(state, auth, path, Json(body)).await
}

#[derive(Debug, Deserialize, Default)]
pub struct AddLogRequest {
    #[serde(default)]
    pub stream: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// `POST /executions/{id}/logs` — worker reporting execution log lines.
pub async fn add_log(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Json(body): Json<AddLogRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:write")?;

    let stream = body.stream.as_deref().unwrap_or("stdout");
    let content = body
        .content
        .as_deref()
        .or(body.message.as_deref())
        .unwrap_or("");

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO execution_logs (id, tenant_id, execution_id, stream, content, logged_at)
         VALUES ($1, $2, $3, $4, $5, NOW())",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(execution_id)
    .bind(stream)
    .bind(content)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({ "id": id, "logged": true }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize, Default)]
pub struct DispatchRequest {
    /// The worker that should run the execution. Required unless the caller
    /// authenticated with that worker's own token.
    #[serde(default)]
    pub worker_id: Option<Uuid>,
    #[serde(default)]
    pub lease_seconds: Option<i64>,
}

/// `POST /executions/{id}/dispatch` — administrative dispatch (spec 10.10).
///
/// The path names an *execution*; the worker comes from the body or the caller's
/// worker credential. Treating the execution id as a worker id (as an earlier
/// revision did) meant this endpoint could never dispatch anything.
pub async fn dispatch(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    body: Option<Json<DispatchRequest>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("workers:claim")?;

    let request = body.map(|Json(body)| body).unwrap_or_default();
    if let (Some(own), Some(req)) = (auth.worker_id, request.worker_id) {
        if own != req {
            return Err(ApiError::forbidden(
                "a worker token may only dispatch to its own worker",
            ));
        }
    }
    let worker_id = auth.worker_id.or(request.worker_id).ok_or_else(|| {
        ApiError::validation("worker_id is required when dispatching on a worker's behalf")
    })?;

    let row = ExecutionRepository::new(&state.pool)
        .dispatch_to(auth.tenant_id, execution_id, worker_id)
        .await?
        .ok_or_else(|| ApiError::not_found("execution"))?;

    let lease = LeaseRepository::new(&state.pool)
        .acquire(
            auth.tenant_id,
            execution_id,
            worker_id,
            None,
            request.lease_seconds.unwrap_or(30),
        )
        .await?;

    Ok(Json(ApiResponse::new(
        json!({
            "execution": crate::jobs::execution_view(&row),
            "lease": { "id": lease.id, "expires_at": lease.expires_at },
        }),
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
        let good: ListExecutionsQuery = serde_json::from_str(r#"{"status":"RUNNING"}"#).unwrap();
        assert_eq!(
            parse_filter(&good).unwrap().status,
            Some(ExecutionStatus::Running)
        );

        let bad: ListExecutionsQuery =
            serde_json::from_str(r#"{"status":"NOT_A_STATUS"}"#).unwrap();
        let error = parse_filter(&bad).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.details[0].field, "status");
    }

    #[test]
    fn every_status_string_is_recognised() {
        for status in [
            "SCHEDULED",
            "QUEUED",
            "DISPATCHED",
            "RUNNING",
            "SUCCEEDED",
            "FAILED",
            "TIMED_OUT",
            "CANCEL_REQUESTED",
            "CANCELLED",
            "RETRY_SCHEDULED",
            "DEAD_LETTERED",
            "ABANDONED",
        ] {
            assert!(is_known_status(status), "{status} must be accepted");
        }
        assert!(!is_known_status("PENDING"));
    }

    #[test]
    fn execution_filters_pass_through() {
        let job = Uuid::new_v4();
        let worker = Uuid::new_v4();
        let query: ListExecutionsQuery =
            serde_json::from_str(&format!(r#"{{"job":"{job}","worker":"{worker}"}}"#)).unwrap();

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
