//! Job endpoints (spec 05 endpoints 1–9).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use forge_domain::{JobId, JobStatus, Priority};
use forge_storage::{
    ExecutionFilter, ExecutionRepository, JobFilter, JobRepository, JobVersionRepository,
    NewExecution, NewJobVersion,
};

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::idempotency;
use crate::router::AppState;

/// `POST /jobs` body.
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateJobRequest {
    pub name: String,
    /// Stable identifier, unique within the tenant.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    /// Which environment the job runs in. Optional so an existing client is
    /// unaffected; omitted means the tenant's default.
    #[serde(default)]
    pub environment: Option<String>,
    /// Which application groups it, by slug. Optional, and a job may be
    /// ungrouped.
    #[serde(default)]
    pub application: Option<String>,
    /// The queue the job's executions run in.
    ///
    /// Settable here because a job with no queue is unrunnable: `claim_next`
    /// filters on `default_queue_id`, so an unbound job is invisible to every
    /// worker's dequeue and its executions sit `QUEUED` forever. Accepting it at
    /// creation avoids a create-then-PATCH pair that leaves a window in which the
    /// job exists but cannot be claimed.
    #[serde(default)]
    pub default_queue_id: Option<Uuid>,
}

/// `POST /jobs` (spec 05 endpoint 1).
pub async fn create(
    State(state): State<AppState>,
    Auth(auth): Auth,
    headers: axum::http::HeaderMap,
    Json(body): Json<CreateJobRequest>,
) -> Result<axum::response::Response, ApiError> {
    auth.require("jobs:write")?;
    validate_name(&body.name)?;

    let priority = match body.priority.as_deref() {
        None => Priority::Normal,
        Some(raw) => parse_priority(raw).ok_or_else(|| {
            ApiError::validation(format!("`{raw}` is not a valid priority")).with_detail(
                "priority",
                "expected CRITICAL, HIGH, NORMAL, LOW or BACKGROUND",
            )
        })?,
    };

    let request_body = serde_json::to_value(&body).unwrap_or(json!({}));

    let outcome = idempotency::run(
        &state.pool,
        auth.tenant_id,
        header_key(&headers, idempotency::IDEMPOTENCY_HEADER),
        "POST /api/v1/jobs",
        &request_body,
        &auth.request_id,
        || async {
            // Resolved by slug and scoped to the caller's tenant, so a job
            // cannot be filed under another tenant's environment by naming it.
            let environment_id = match body.environment.as_deref() {
                Some(slug) => Some(
                    crate::applications::resolve_environment_slug(
                        &state,
                        auth.tenant_id,
                        slug,
                    )
                    .await?,
                ),
                None => None,
            };
            let application_id = match body.application.as_deref() {
                Some(slug) => Some(
                    forge_storage::ApplicationRepository::new(&state.pool)
                        .get_by_slug(auth.tenant_id, slug.trim())
                        .await?
                        .ok_or_else(|| ApiError::not_found("application"))?
                        .id,
                ),
                None => None,
            };

            // Verified as belonging to this tenant: a caller-supplied uuid must
            // not file a job into another tenant's queue.
            if let Some(queue_id) = body.default_queue_id {
                let owned: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM queues WHERE id = $1 AND tenant_id = $2)",
                )
                .bind(queue_id)
                .bind(auth.tenant_id.into_uuid())
                .fetch_one(&state.pool)
                .await?;
                if !owned {
                    return Err(ApiError::validation(
                        "that queue does not exist in this tenant",
                    )
                    .with_detail("default_queue_id", "not found"));
                }
            }

            let repo = JobRepository::new(&state.pool);
            let row = repo
                .create(
                    auth.tenant_id,
                    body.key.clone(),
                    &body.name,
                    body.description.clone(),
                    priority,
                    Some(auth.user_id),
                    environment_id,
                    application_id,
                )
                .await?;

            if let Some(queue_id) = body.default_queue_id {
                sqlx::query("UPDATE jobs SET default_queue_id = $2 WHERE id = $1")
                    .bind(row.id)
                    .bind(queue_id)
                    .execute(&state.pool)
                    .await?;
            }
            Ok((StatusCode::CREATED, JobView::from_row(&row), Some(row.id)))
        },
    )
    .await?;

    Ok(outcome.into_response(&auth.request_id))
}

/// `GET /jobs` (spec 05 endpoint 2).
pub async fn list(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
    Query(filter): Query<ListJobsQuery>,
) -> Result<Json<ListResponse<JobView>>, ApiError> {
    auth.require("jobs:read")?;

    let status = match filter.status.as_deref() {
        None => None,
        Some(raw) => Some(raw.parse::<JobStatus>().map_err(|_| {
            ApiError::validation(format!("`{raw}` is not a valid job status"))
                .with_detail("status", "unknown status")
        })?),
    };

    let label = match filter.label.as_deref() {
        Some("key:value") => filter
            .label_value
            .as_ref()
            .map(|v| ("key".to_string(), v.clone())),
        _ => None,
    };

    let repo = JobRepository::new(&state.pool);
    let page = repo
        .list(
            auth.tenant_id,
            &JobFilter {
                status,
                queue_id: filter.queue,
                owner_id: filter.owner,
                label,
                created_after: filter.created_after,
                created_before: filter.created_before,
                search: filter.search.clone(),
            },
            pagination.cursor.as_deref(),
            pagination.effective_limit(),
        )
        .await?;

    let views: Vec<JobView> = page.items.iter().map(JobView::from_row).collect();
    Ok(Json(ListResponse::from_page(
        forge_storage::Page {
            items: views,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        },
        auth.request_id,
    )))
}

#[derive(Debug, Default, Deserialize)]
pub struct ListJobsQuery {
    pub status: Option<String>,
    pub queue: Option<Uuid>,
    pub owner: Option<Uuid>,
    /// A label filter, written `label=key:value` with `label_value` supplied
    /// separately so the value may contain a colon.
    pub label: Option<String>,
    pub label_value: Option<String>,
    pub search: Option<String>,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
}

/// `GET /jobs/{job_id}` (spec 05 endpoint 3).
pub async fn get(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ApiResponse<JobView>>, ApiError> {
    auth.require("jobs:read")?;

    let row = JobRepository::new(&state.pool)
        .get(auth.tenant_id, JobId::from_uuid(job_id))
        .await?;

    Ok(Json(ApiResponse::new(
        JobView::from_row(&row),
        auth.request_id,
    )))
}

/// `PATCH /jobs/{job_id}` (spec 05 endpoint 4).
#[derive(Debug, Deserialize)]
pub struct UpdateJobRequest {
    pub name: Option<String>,
    /// `DRAFT`, `ACTIVE`, or `ARCHIVED`. Changing it records an undo entry
    /// (UI.md section 38) so a mistake can be reversed without a second edit.
    pub status: Option<String>,
    /// Double option: absent leaves it, `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
    pub key: Option<String>,
    pub priority: Option<String>,
    /// Optimistic concurrency: the `updated_at` the client last read.
    ///
    /// Accepted in either `expected_updated_at` or `expectedUpdatedAt` so a
    /// client written against the OpenAPI camelCase example and one written
    /// against the stored snake_case column both work.
    #[serde(alias = "expectedUpdatedAt")]
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}

/// Distinguishes an absent field from an explicit `null`.
///
/// A single `Option<String>` cannot express "leave unchanged" and "clear this"
/// at the same time. Only `String` fields need this, so it is specialised
/// rather than generic: a generic version would have to require
/// `DeserializeOwned`, which a borrowed deserialiser cannot always satisfy.
fn double_option<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(Some(None)),
        serde_json::Value::String(text) => Ok(Some(Some(text))),
        other => Err(serde::de::Error::custom(format!(
            "expected a string or null, got {other}"
        ))),
    }
}

pub async fn update(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
    Json(body): Json<UpdateJobRequest>,
) -> Result<Json<ApiResponse<JobView>>, ApiError> {
    auth.require("jobs:write")?;

    if let Some(name) = &body.name {
        validate_name(name)?;
    }
    let priority = match body.priority.as_deref() {
        None => None,
        Some(raw) => Some(parse_priority(raw).ok_or_else(|| {
            ApiError::validation(format!("`{raw}` is not a valid priority")).with_detail(
                "priority",
                "expected CRITICAL, HIGH, NORMAL, LOW or BACKGROUND",
            )
        })?),
    };

    let patch = forge_storage::JobPatch {
        name: body.name,
        description: body.description,
        key: body.key,
        priority,
    };

    // The patched row is not used: the response is re-read below so it reflects
    // a status change as well. What matters here is that the patch applied, so
    // a failed patch still returns the error.
    JobRepository::new(&state.pool)
        .update(
            auth.tenant_id,
            JobId::from_uuid(job_id),
            body.expected_updated_at,
            &patch,
        )
        .await?;

    // A status change is applied separately from the patch, and only after the
    // patch succeeded, so a rejected edit does not leave a misleading undo
    // entry behind.
    if let Some(raw) = body.status.as_deref() {
        let next = parse_status(raw).ok_or_else(|| {
            ApiError::validation(format!("`{raw}` is not a valid job status"))
                .with_detail("status", "expected DRAFT, ACTIVE or ARCHIVED")
        })?;

        // Capture the prior status so undo can restore exactly what was there.
        let prior: Option<(String,)> =
            sqlx::query_as("SELECT status FROM jobs WHERE id = $1 AND tenant_id = $2")
                .bind(job_id)
                .bind(auth.tenant_id.into_uuid())
                .fetch_optional(&state.pool)
                .await
                .map_err(ApiError::from)?;

        if let Some((previous,)) = prior {
            if previous != next.as_str() {
                sqlx::query("UPDATE jobs SET status = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
                    .bind(job_id)
                    .bind(auth.tenant_id.into_uuid())
                    .bind(next.as_str())
                    .execute(&state.pool)
                    .await
                    .map_err(ApiError::from)?;

                crate::parity::record_undo(
                    &state.pool,
                    auth.tenant_id.into_uuid(),
                    auth.user_id,
                    "JOB_STATUS",
                    "JOB",
                    job_id,
                    serde_json::json!({ "status": previous }),
                )
                .await?;
            }
        }
    }

    // Re-read through the repository so the response reflects the status
    // change rather than the row as it was before it.
    let fresh = JobRepository::new(&state.pool)
        .get(auth.tenant_id, JobId::from_uuid(job_id))
        .await?;

    Ok(Json(ApiResponse::new(
        JobView::from_row(&fresh),
        auth.request_id,
    )))
}

/// Parses a job status, returning `None` for an unknown value.
fn parse_status(raw: &str) -> Option<forge_domain::JobStatus> {
    match raw.to_uppercase().as_str() {
        "DRAFT" => Some(forge_domain::JobStatus::Draft),
        "ACTIVE" => Some(forge_domain::JobStatus::Active),
        "ARCHIVED" => Some(forge_domain::JobStatus::Archived),
        _ => None,
    }
}

/// `DELETE /jobs/{job_id}` (spec 05 endpoint 5) — a soft delete.
pub async fn archive(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:delete")?;

    JobRepository::new(&state.pool)
        .archive(auth.tenant_id, JobId::from_uuid(job_id))
        .await?;

    Ok(Json(ApiResponse::new(
        json!({ "archived": true, "id": job_id }),
        auth.request_id,
    )))
}

/// `POST /jobs/{job_id}/versions` (spec 05 endpoint 6).
#[derive(Debug, Deserialize)]
pub struct CreateVersionRequest {
    #[serde(default = "default_execution_type")]
    pub execution_type: String,
    #[serde(default)]
    pub execution_config: serde_json::Value,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: i32,
    /// An empty object means "use the default policy".
    #[serde(default)]
    pub retry_policy: serde_json::Value,
    #[serde(default)]
    pub concurrency_policy: serde_json::Value,
    #[serde(default)]
    pub resource_requirements: serde_json::Value,
    #[serde(default)]
    pub environment: serde_json::Value,
    #[serde(default)]
    pub secret_references: serde_json::Value,
}

/// Parses a policy from JSON, falling back to the default for an empty object.
///
/// A malformed policy is rejected rather than silently defaulted, so a typo
/// cannot quietly weaken a retry budget.
fn parse_policy<T>(field: &'static str, raw: &serde_json::Value) -> Result<T, ApiError>
where
    T: serde::de::DeserializeOwned + Default,
{
    if raw.is_null() {
        return Ok(T::default());
    }
    // An absent or empty object carries no override.
    match raw.as_object() {
        Some(map) if map.is_empty() => Ok(T::default()),
        _ => serde_json::from_value(raw.clone()).map_err(|e| {
            ApiError::validation(format!("invalid {field}: {e}")).with_detail(field, "invalid")
        }),
    }
}

fn default_execution_type() -> String {
    "WORKER_TASK".to_string()
}

fn default_timeout() -> i32 {
    3600
}

pub async fn create_version(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
    Json(body): Json<CreateVersionRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("job_versions:write")?;

    if body.timeout_seconds <= 0 {
        return Err(ApiError::validation("timeout_seconds must be positive")
            .with_detail("timeout_seconds", "must be greater than zero"));
    }
    if !["HTTP_REQUEST", "CONTAINER_COMMAND", "WORKER_TASK"].contains(&body.execution_type.as_str())
    {
        return Err(ApiError::validation(
            "execution_type must be HTTP_REQUEST, CONTAINER_COMMAND or WORKER_TASK",
        )
        .with_detail("execution_type", "unknown execution type"));
    }

    let repo = JobVersionRepository::new(&state.pool);
    let row = repo
        .create(
            auth.tenant_id,
            JobId::from_uuid(job_id),
            &NewJobVersion {
                execution_type: body.execution_type,
                execution_config: body.execution_config,
                input_schema: None,
                timeout_seconds: body.timeout_seconds,
                retry_policy: parse_policy("retry_policy", &body.retry_policy)?,
                concurrency_policy: parse_policy("concurrency_policy", &body.concurrency_policy)?,
                resource_requirements: body.resource_requirements,
                environment: body.environment,
                secret_references: body.secret_references,
                created_by: Some(auth.user_id),
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "id": row.id,
                "job_id": row.job_id,
                "version_number": row.version_number,
                "execution_type": row.execution_type,
                "timeout_seconds": row.timeout_seconds,
                "published_at": row.published_at,
            }),
            auth.request_id,
        )),
    ))
}

/// `GET /jobs/{job_id}/versions` (spec 05 endpoint 7).
pub async fn list_versions(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("job_versions:read")?;

    let rows = JobVersionRepository::new(&state.pool)
        .list(auth.tenant_id, JobId::from_uuid(job_id))
        .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "version_number": row.version_number,
                "execution_type": row.execution_type,
                "timeout_seconds": row.timeout_seconds,
                "published_at": row.published_at,
                "created_at": row.created_at,
            })
        })
        .collect();

    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

/// `POST /jobs/{job_id}/versions/{version_id}/publish` (spec 05 endpoint 8).
pub async fn publish_version(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path((job_id, version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("job_versions:write")?;

    let row = JobVersionRepository::new(&state.pool)
        .publish(
            auth.tenant_id,
            JobId::from_uuid(job_id),
            forge_domain::JobVersionId::from_uuid(version_id),
        )
        .await?;

    Ok(Json(ApiResponse::new(
        json!({
            "id": row.id,
            "version_number": row.version_number,
            "published_at": row.published_at,
        }),
        auth.request_id,
    )))
}

/// `POST /jobs/{job_id}/trigger` (spec 05 endpoint 9).
///
/// Spec 09.10: a manual trigger bypasses the calendar but still respects
/// authorization, concurrency, and idempotency — and must not alter the
/// schedule's future occurrences (AT-SCH-010).
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct TriggerRequest {
    #[serde(default)]
    pub input: serde_json::Value,
    /// Pin a specific published version rather than the current one.
    #[serde(default)]
    pub version_id: Option<Uuid>,
}

pub async fn trigger(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(body): Json<TriggerRequest>,
) -> Result<axum::response::Response, ApiError> {
    auth.require("jobs:trigger")?;

    let version_id = match body.version_id {
        Some(id) => forge_domain::JobVersionId::from_uuid(id),
        None => {
            let job = JobRepository::new(&state.pool)
                .get(auth.tenant_id, JobId::from_uuid(job_id))
                .await?;
            match job.current_version_id {
                Some(current) => forge_domain::JobVersionId::from_uuid(current),
                None => {
                    return Err(
                        ApiError::conflict("this job has no published version to run")
                            .with_detail("job", "publish a version first"),
                    )
                }
            }
        }
    };

    /*
     * The execution inherits the job's queue.
     *
     * `claim_next` filters on `executions.queue_id`, so an execution created
     * without one is invisible to every worker's dequeue and its work sits
     * QUEUED forever. Resolved once here, before the idempotency wrapper, so it
     * is in scope for the closure below.
     */
    let job_queue_id = JobRepository::new(&state.pool)
        .get(auth.tenant_id, JobId::from_uuid(job_id))
        .await?
        .default_queue_id;

    let request_body = serde_json::to_value(&body).unwrap_or(json!({}));

    let outcome = idempotency::run(
        &state.pool,
        auth.tenant_id,
        header_key(&headers, idempotency::IDEMPOTENCY_HEADER),
        "POST /api/v1/jobs/{id}/trigger",
        &request_body,
        &auth.request_id,
        || async {
            let executions = ExecutionRepository::new(&state.pool);
            let row = executions
                .create(NewExecution {
                    tenant_id: auth.tenant_id,
                    job_id: JobId::from_uuid(job_id),
                    job_version_id: version_id,
                    // Inherited from the job. This was hardcoded to `None`,
                    // which discarded the job's queue binding entirely:
                    // `claim_next` filters on `executions.queue_id`, so every
                    // triggered execution sat QUEUED with no queue and no worker
                    // could ever claim it. Read here because `job` above is
                    // scoped to the version-resolution branch.
                    queue_id: job_queue_id,
                    schedule_id: None,
                    trigger_source: forge_domain::TriggerSource::Manual,
                    priority: Priority::Normal,
                    scheduled_for: None,
                    correlation_id: Some(auth.request_id.clone()),
                    input: body.input.clone(),
                })
                .await?;
            Ok((
                StatusCode::ACCEPTED,
                json!({
                    "execution_id": row.id,
                    "job_id": row.job_id,
                    "job_version_id": row.job_version_id,
                    "status": row.status,
                    "trigger_source": row.trigger_source,
                }),
                Some(row.id),
            ))
        },
    )
    .await?;

    Ok(outcome.into_response(&auth.request_id))
}

/// `GET /jobs/{job_id}/executions` — a job's history.
pub async fn list_executions(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("executions:read")?;

    let page = ExecutionRepository::new(&state.pool)
        .list(
            auth.tenant_id,
            &ExecutionFilter {
                job_id: Some(job_id),
                ..Default::default()
            },
            pagination.cursor.as_deref(),
            pagination.effective_limit(),
        )
        .await?;

    let items = page.items.iter().map(execution_view).collect();
    Ok(Json(ListResponse::from_page(
        forge_storage::Page {
            items,
            next_cursor: page.next_cursor,
            has_more: page.has_more,
        },
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// A job as the API presents it.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub id: Uuid,
    pub key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub current_version_id: Option<Uuid>,
    pub default_queue_id: Option<Uuid>,
    pub priority: String,
    pub owner_id: Option<Uuid>,
    pub labels: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl JobView {
    pub fn from_row(row: &forge_storage::JobRow) -> Self {
        Self {
            id: row.id,
            key: row.key.clone(),
            name: row.name.clone(),
            description: row.description.clone(),
            status: row.status.clone(),
            current_version_id: row.current_version_id,
            default_queue_id: row.default_queue_id,
            priority: row.priority.clone(),
            owner_id: row.owner_id,
            labels: row.labels.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// An execution as the API presents it.
pub fn execution_view(row: &forge_storage::ExecutionRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "job_id": row.job_id,
        "job_version_id": row.job_version_id,
        "workflow_id": row.workflow_id,
        "status": row.status,
        "queue_id": row.queue_id,
        "worker_id": row.worker_id,
        "attempt_count": row.attempt_count,
        "priority": row.priority,
        "trigger_source": row.trigger_source,
        "scheduled_for": row.scheduled_for,
        "correlation_id": row.correlation_id,
        "error_class": row.error_class,
        "error_message": row.error_message,
        "created_at": row.created_at,
        "started_at": row.started_at,
        "ended_at": row.ended_at,
        // Spec 01.6: the input a worker must run with, and the output a
        // completed attempt produced. Without these a worker protocol client
        // receives an execution it cannot actually perform.
        "input": row.input,
        "output": row.output,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_name(name: &str) -> Result<(), ApiError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty")
        );
    }
    if trimmed.chars().count() > 255 {
        return Err(ApiError::validation("name must be at most 255 characters")
            .with_detail("name", "too long"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(
            ApiError::validation("name must not contain control characters")
                .with_detail("name", "contains a control character"),
        );
    }
    Ok(())
}

fn parse_priority(raw: &str) -> Option<Priority> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "CRITICAL" => Some(Priority::Critical),
        "HIGH" => Some(Priority::High),
        "NORMAL" => Some(Priority::Normal),
        "LOW" => Some(Priority::Low),
        "BACKGROUND" => Some(Priority::Background),
        _ => None,
    }
}

/// Reads a header as a UTF-8 string.
pub fn header_key<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|h| h.to_str().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_name_is_accepted() {
        assert!(validate_name("Settle daily").is_ok());
        assert!(validate_name("  padded  ").is_ok());
    }

    #[test]
    fn a_bad_name_is_rejected_with_a_field() {
        for bad in ["", "   ", &"x".repeat(300), "has\nnewline"] {
            let error = validate_name(bad).unwrap_err();
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
            assert_eq!(error.details[0].field, "name");
        }
    }

    #[test]
    fn priorities_parse_case_insensitively() {
        assert_eq!(parse_priority("critical"), Some(Priority::Critical));
        assert_eq!(parse_priority(" HIGH "), Some(Priority::High));
        assert_eq!(parse_priority("normal"), Some(Priority::Normal));
        assert_eq!(parse_priority("low"), Some(Priority::Low));
        assert_eq!(parse_priority("background"), Some(Priority::Background));
        assert_eq!(parse_priority("urgent"), None);
    }

    #[test]
    fn a_view_exposes_the_fields_the_ui_needs() {
        let row = forge_storage::JobRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            key: Some("settle".into()),
            name: "Settle".into(),
            description: None,
            status: "ACTIVE".into(),
            current_version_id: Some(Uuid::new_v4()),
            default_queue_id: None,
            priority: "HIGH".into(),
            owner_id: None,
            labels: json!({}),
            environment_id: Uuid::new_v4(),
            application_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let view = JobView::from_row(&row);
        assert_eq!(view.status, "ACTIVE");
        assert_eq!(view.priority, "HIGH");
        assert_eq!(view.key.as_deref(), Some("settle"));
    }

    #[test]
    fn an_execution_view_carries_its_classification() {
        // AT-OBS-004: a failure exposes a safe class.
        let row = forge_storage::ExecutionRow {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            job_id: Some(Uuid::new_v4()),
            job_version_id: Some(Uuid::new_v4()),
            workflow_id: None,
            status: "FAILED".into(),
            queue_id: None,
            worker_id: None,
            attempt_count: 1,
            priority: "NORMAL".into(),
            trigger_source: "MANUAL".into(),
            scheduled_for: None,
            enqueued_at: None,
            correlation_id: Some("corr-1".into()),
            error_class: Some("TIMEOUT".into()),
            error_code: None,
            error_message: Some("upstream timed out".into()),
            created_at: chrono::Utc::now(),
            started_at: None,
            ended_at: None,
            input: serde_json::json!({}),
            output: None,
            retry_at: None,
            deadline_at: None,
        };
        let view = execution_view(&row);
        assert_eq!(view["status"], "FAILED");
        assert_eq!(view["error_class"], "TIMEOUT");
        assert_eq!(view["correlation_id"], "corr-1");
    }

    #[test]
    fn a_version_request_defaults_to_a_worker_task() {
        let body: CreateVersionRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(body.execution_type, "WORKER_TASK");
        assert_eq!(body.timeout_seconds, 3600);
    }
}
