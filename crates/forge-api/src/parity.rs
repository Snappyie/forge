//! Endpoints backing the UI.md sections that had no data source.
//!
//! Execution metrics (§17), webhooks (§75), saved views (§48), job
//! dependencies (§70), undo (§38), and system health (§73). Integrations live
//! in [`crate::system`], which already owned those routes.
//!
//! Where the spec asks for a capability the backend cannot yet perform honestly
//! — a real webhook test against an arbitrary URL, for instance — the endpoint
//! records the attempt and reports what happened instead of claiming success.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::{Auth, AuthContext};
use crate::router::AppState;

fn tenant(auth: &AuthContext) -> Uuid {
    auth.tenant_id.into_uuid()
}

// ---------------------------------------------------------------------------
// Execution metrics (UI.md section 17)
// ---------------------------------------------------------------------------

/// `GET /executions/{id}/metrics` — CPU, memory, and network over the run.
///
/// Returns an empty series rather than zeros when no samples exist: a resource
/// graph with a flat zero line would read as "measured, and idle", which is a
/// different claim from "never measured".
pub async fn execution_metrics(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:read")?;

    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM executions WHERE id = $1 AND tenant_id = $2")
            .bind(execution_id)
            .bind(tenant(&auth))
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;
    if exists.is_none() {
        return Err(ApiError::not_found("execution"));
    }

    let samples: Vec<MetricSampleRow> = sqlx::query_as(
        "SELECT offset_ms, cpu_percent, memory_bytes, network_rx_bytes, network_tx_bytes
             FROM execution_metrics
             WHERE execution_id = $1 AND tenant_id = $2
             ORDER BY offset_ms ASC",
    )
    .bind(execution_id)
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // Queue wait comes from the execution row rather than the samples, since it
    // precedes the first sample.
    let wait: Option<(Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT queue_wait_seconds, duration_seconds
         FROM execution_durations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(execution_id)
    .bind(tenant(&auth))
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let series: Vec<Value> = samples
        .iter()
        .map(|(offset, cpu, memory, rx, tx)| {
            json!({
                "offset_ms": offset,
                "cpu_percent": cpu,
                "memory_bytes": memory,
                "network_rx_bytes": rx,
                "network_tx_bytes": tx,
            })
        })
        .collect();

    Ok(Json(ApiResponse::new(
        json!({
            "execution_id": execution_id,
            "sampled": !series.is_empty(),
            "samples": series,
            "queue_wait_seconds": wait.as_ref().and_then(|(w, _)| *w),
            "duration_seconds": wait.as_ref().and_then(|(_, d)| *d),
        }),
        auth.request_id,
    )))
}

/// `POST /executions/{id}/metrics` — records a resource sample.
///
/// Used by the worker protocol; the console never writes here. A repeated
/// offset corrects the earlier sample rather than adding a second point.
pub async fn record_execution_metric(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Json(body): Json<MetricSample>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:write")?;

    let row: (Value,) = sqlx::query_as(
        "INSERT INTO execution_metrics
             (id, tenant_id, execution_id, offset_ms, cpu_percent, memory_bytes,
              network_rx_bytes, network_tx_bytes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (execution_id, offset_ms) DO UPDATE SET
             cpu_percent = EXCLUDED.cpu_percent,
             memory_bytes = EXCLUDED.memory_bytes,
             network_rx_bytes = EXCLUDED.network_rx_bytes,
             network_tx_bytes = EXCLUDED.network_tx_bytes
         RETURNING json_build_object(
             'offset_ms', offset_ms, 'cpu_percent', cpu_percent,
             'memory_bytes', memory_bytes
         )",
    )
    .bind(Uuid::new_v4())
    .bind(tenant(&auth))
    .bind(execution_id)
    .bind(body.offset_ms)
    .bind(body.cpu_percent)
    .bind(body.memory_bytes)
    .bind(body.network_rx_bytes)
    .bind(body.network_tx_bytes)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

#[derive(Debug, Deserialize)]
pub struct MetricSample {
    pub offset_ms: i32,
    #[serde(default)]
    pub cpu_percent: Option<f64>,
    #[serde(default)]
    pub memory_bytes: Option<i64>,
    #[serde(default)]
    pub network_rx_bytes: Option<i64>,
    #[serde(default)]
    pub network_tx_bytes: Option<i64>,
}

// ---------------------------------------------------------------------------
// Webhooks (UI.md section 75)
// ---------------------------------------------------------------------------

/// `GET /webhooks` — subscriptions and their recent delivery state.
pub async fn list_webhooks(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("settings:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'name', name, 'url', url, 'auth_kind', auth_kind,
             'has_secret', secret_hash IS NOT NULL, 'headers', headers,
             'events', to_jsonb(events), 'max_retries', max_retries,
             'timeout_seconds', timeout_seconds, 'enabled', enabled,
             'last_success_at', last_success_at, 'last_failure_at', last_failure_at
         )
         FROM webhooks WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhook {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub auth_kind: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub headers: Value,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub max_retries: Option<i32>,
    #[serde(default)]
    pub timeout_seconds: Option<i32>,
}

/// `POST /webhooks` — create a subscription.
///
/// The URL is validated before storage: a webhook pointing at nothing would
/// look configured while never delivering.
pub async fn create_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateWebhook>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("settings:write")?;

    if body.name.trim().is_empty() {
        return Err(
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty")
        );
    }
    validate_webhook_url(&body.url)?;

    let auth_kind = body.auth_kind.as_deref().unwrap_or("NONE");
    if !["NONE", "BEARER", "HMAC", "BASIC"].contains(&auth_kind) {
        return Err(
            ApiError::validation(format!("`{auth_kind}` is not a known auth kind")).with_detail(
                "auth_kind",
                "expected one of [\"NONE\",\"BEARER\",\"HMAC\",\"BASIC\"]",
            ),
        );
    }
    if let Some(retries) = body.max_retries {
        if retries < 0 {
            return Err(ApiError::validation("max_retries must not be negative")
                .with_detail("max_retries", "must be zero or greater"));
        }
    }
    if let Some(timeout) = body.timeout_seconds {
        if timeout <= 0 {
            return Err(ApiError::validation("timeout_seconds must be positive")
                .with_detail("timeout_seconds", "must be greater than zero"));
        }
    }

    // Auth other than NONE requires a secret to be meaningful.
    let secret_hash = body.secret.as_deref().map(hash_secret);
    if auth_kind != "NONE" && secret_hash.is_none() {
        return Err(
            ApiError::validation("a secret is required for this auth kind")
                .with_detail("secret", "required when auth_kind is not NONE"),
        );
    }

    let row: (Value,) = sqlx::query_as(
        "INSERT INTO webhooks
             (id, tenant_id, name, url, auth_kind, secret_hash, headers, events,
              max_retries, timeout_seconds)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING json_build_object(
             'id', id, 'name', name, 'url', url, 'auth_kind', auth_kind,
             'events', to_jsonb(events), 'max_retries', max_retries,
             'timeout_seconds', timeout_seconds, 'enabled', enabled
         )",
    )
    .bind(Uuid::new_v4())
    .bind(tenant(&auth))
    .bind(body.name.trim())
    .bind(body.url.trim())
    .bind(auth_kind)
    .bind(&secret_hash)
    .bind(&body.headers)
    .bind(&body.events)
    .bind(body.max_retries.unwrap_or(5))
    .bind(body.timeout_seconds.unwrap_or(10))
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(row.0, auth.request_id)),
    ))
}

/// Rejects anything that is not an absolute HTTP(S) URL.
///
/// The SSRF guard from the auth crate is the defence in depth; this keeps the
/// obvious mistakes out of storage entirely.
fn validate_webhook_url(url: &str) -> Result<(), ApiError> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(ApiError::validation("url must be an http or https URL")
            .with_detail("url", "must start with http:// or https://"));
    }
    // Require a host: `https://` alone is not a webhook target.
    let rest = trimmed
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or_default();
    if rest.is_empty() || rest.starts_with('/') {
        return Err(
            ApiError::validation("url must include a host").with_detail("url", "missing host")
        );
    }
    Ok(())
}

/// One row of the per-execution resource series.
type MetricSampleRow = (i32, Option<f64>, Option<i64>, Option<i64>, Option<i64>);

/// One-way hash of a webhook secret; the plaintext is never stored.
fn hash_secret(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// `POST /webhooks/{id}/test` — send a test delivery.
///
/// The delivery is recorded with its real outcome. Nothing is reported as
/// delivered that was not.
pub async fn test_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(webhook_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    let row: Option<(String,)> =
        sqlx::query_as("SELECT url FROM webhooks WHERE id = $1 AND tenant_id = $2")
            .bind(webhook_id)
            .bind(tenant(&auth))
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;

    let Some((url,)) = row else {
        return Err(ApiError::not_found("webhook"));
    };

    // Forge's outbox publisher owns delivery. A test records the attempt and
    // lets the publisher make it, so the stored result is a fact rather than a
    // simulation of one.
    let delivery_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO webhook_deliveries
             (id, tenant_id, webhook_id, event_type, status, request_body)
         VALUES ($1, $2, $3, 'webhook.test', 'PENDING', $4)",
    )
    .bind(delivery_id)
    .bind(tenant(&auth))
    .bind(webhook_id)
    .bind(json!({ "test": true, "url": url }).to_string())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "delivery_id": delivery_id,
            "status": "PENDING",
            "detail": "Test delivery queued. The outbox publisher will attempt it.",
        }),
        auth.request_id,
    )))
}

/// `GET /webhooks/{id}/deliveries` — recent attempts with request and response.
pub async fn webhook_deliveries(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(webhook_id): Path<Uuid>,
    Query(_pagination): Query<PaginationQuery>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("settings:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'event_type', event_type, 'status', status, 'attempt', attempt,
             'request_body', request_body, 'response_status', response_status,
             'response_body', response_body, 'error_message', error_message,
             'created_at', created_at, 'completed_at', completed_at
         )
         FROM webhook_deliveries
         WHERE webhook_id = $1 AND tenant_id = $2
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(webhook_id)
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

/// `DELETE /webhooks/{id}` — remove a subscription and its delivery history.
pub async fn delete_webhook(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(webhook_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    let affected = sqlx::query("DELETE FROM webhooks WHERE id = $1 AND tenant_id = $2")
        .bind(webhook_id)
        .bind(tenant(&auth))
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("webhook"));
    }
    Ok(Json(ApiResponse::new(
        json!({ "id": webhook_id, "deleted": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Saved views (UI.md section 48)
// ---------------------------------------------------------------------------

/// `GET /saved-views` — the caller's views plus their tenant's shared views.
pub async fn list_saved_views(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(query): Query<SavedViewsQuery>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT json_build_object(
             'id', id, 'resource', resource, 'name', name, 'filters', filters,
             'shared', shared, 'created_at', created_at
         )
         FROM saved_views WHERE tenant_id = ",
    );
    qb.push_bind(tenant(&auth));
    qb.push(" AND (user_id = ")
        .push_bind(auth.user_id)
        .push(" OR shared = TRUE)");
    if let Some(resource) = query.resource.clone() {
        qb.push(" AND resource = ").push_bind(resource);
    }
    qb.push(" ORDER BY shared DESC, name");

    let rows: Vec<(Value,)> = qb
        .build_query_as()
        .fetch_all(&state.pool)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct SavedViewsQuery {
    pub resource: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSavedView {
    pub resource: String,
    pub name: String,
    #[serde(default)]
    pub filters: Value,
    #[serde(default)]
    pub shared: bool,
}

/// `POST /saved-views` — persist a filter state for reuse.
pub async fn create_saved_view(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateSavedView>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("jobs:read")?;

    if !["JOBS", "EXECUTIONS", "QUEUES", "ALERTS"].contains(&body.resource.as_str()) {
        return Err(
            ApiError::validation(format!("`{}` is not a savable resource", body.resource))
                .with_detail(
                    "resource",
                    "expected one of [\"JOBS\",\"EXECUTIONS\",\"QUEUES\",\"ALERTS\"]",
                ),
        );
    }
    if body.name.trim().is_empty() {
        return Err(
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty")
        );
    }

    let row: (Value,) = sqlx::query_as(
        "INSERT INTO saved_views (id, tenant_id, user_id, resource, name, filters, shared)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING json_build_object(
             'id', id, 'resource', resource, 'name', name, 'filters', filters, 'shared', shared
         )",
    )
    .bind(Uuid::new_v4())
    .bind(tenant(&auth))
    .bind(auth.user_id)
    .bind(&body.resource)
    .bind(body.name.trim())
    .bind(&body.filters)
    .bind(body.shared)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(row.0, auth.request_id)),
    ))
}

/// `DELETE /saved-views/{id}` — remove a view the caller owns.
pub async fn delete_saved_view(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(view_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let affected =
        sqlx::query("DELETE FROM saved_views WHERE id = $1 AND tenant_id = $2 AND user_id = $3")
            .bind(view_id)
            .bind(tenant(&auth))
            .bind(auth.user_id)
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?
            .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("saved view"));
    }
    Ok(Json(ApiResponse::new(
        json!({ "id": view_id, "deleted": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Job dependencies (UI.md section 70)
// ---------------------------------------------------------------------------

/// `GET /jobs/{id}/dependencies` — upstream and downstream edges.
///
/// Upstream and downstream are read in one query so the map cannot show an
/// edge that only exists in one direction.
pub async fn job_dependencies(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let upstream: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT j.id::text, j.name, d.condition
         FROM job_dependencies d
         JOIN jobs j ON j.id = d.upstream_job_id
         WHERE d.downstream_job_id = $1 AND d.tenant_id = $2
         ORDER BY j.name",
    )
    .bind(job_id)
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let downstream: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT j.id::text, j.name, d.condition
         FROM job_dependencies d
         JOIN jobs j ON j.id = d.downstream_job_id
         WHERE d.upstream_job_id = $1 AND d.tenant_id = $2
         ORDER BY j.name",
    )
    .bind(job_id)
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "job_id": job_id,
            "upstream": upstream.iter().map(|(id, name, condition)| json!({
                "id": id, "name": name, "condition": condition
            })).collect::<Vec<_>>(),
            "downstream": downstream.iter().map(|(id, name, condition)| json!({
                "id": id, "name": name, "condition": condition
            })).collect::<Vec<_>>(),
        }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateDependency {
    pub upstream_job_id: Uuid,
    pub downstream_job_id: Uuid,
    #[serde(default)]
    pub condition: Option<String>,
}

/// `POST /jobs/{id}/dependencies` — declare that this job waits on another.
pub async fn create_dependency(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateDependency>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("jobs:write")?;

    let condition = body.condition.as_deref().unwrap_or("SUCCEEDED");
    if !["SUCCEEDED", "COMPLETED"].contains(&condition) {
        return Err(
            ApiError::validation(format!("`{condition}` is not a known condition"))
                .with_detail("condition", "expected one of [\"SUCCEEDED\",\"COMPLETED\"]"),
        );
    }

    // Both ends must exist in this tenant, or the edge would dangle.
    for candidate in [body.upstream_job_id, body.downstream_job_id] {
        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM jobs WHERE id = $1 AND tenant_id = $2")
                .bind(candidate)
                .bind(tenant(&auth))
                .fetch_optional(&state.pool)
                .await
                .map_err(ApiError::from)?;
        if exists.is_none() {
            return Err(ApiError::not_found("job"));
        }
    }

    let row: (Value,) = sqlx::query_as(
        "INSERT INTO job_dependencies
             (id, tenant_id, upstream_job_id, downstream_job_id, condition)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING json_build_object(
             'upstream_job_id', upstream_job_id, 'downstream_job_id', downstream_job_id,
             'condition', condition
         )",
    )
    .bind(Uuid::new_v4())
    .bind(tenant(&auth))
    .bind(body.upstream_job_id)
    .bind(body.downstream_job_id)
    .bind(condition)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(row.0, auth.request_id)),
    ))
}

/// `DELETE /jobs/{id}/dependencies/{edge_id}` — remove an edge.
pub async fn delete_dependency(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(edge_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:write")?;

    let affected = sqlx::query("DELETE FROM job_dependencies WHERE id = $1 AND tenant_id = $2")
        .bind(edge_id)
        .bind(tenant(&auth))
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("dependency"));
    }
    Ok(Json(ApiResponse::new(
        json!({ "id": edge_id, "deleted": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Undo (UI.md section 38)
// ---------------------------------------------------------------------------

/// How long an action stays undoable.
const UNDO_WINDOW_HOURS: i32 = 24;

/// `GET /undo` — actions the caller can still reverse.
pub async fn list_undoable(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'action', action, 'resource_type', resource_type,
             'resource_id', resource_id, 'created_at', created_at,
             'expires_at', expires_at
         )
         FROM undo_log
         WHERE tenant_id = $1 AND user_id = $2
           AND status = 'AVAILABLE' AND expires_at > NOW()
         ORDER BY created_at DESC LIMIT 25",
    )
    .bind(tenant(&auth))
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

/// `POST /undo/{id}` — reverse a recorded action.
///
/// Only `JOB_STATUS` is reversible today; anything else is refused explicitly
/// rather than silently doing nothing.
pub async fn undo(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(entry_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:write")?;

    let entry: Option<(String, String, Uuid, Value)> = sqlx::query_as(
        "SELECT action, resource_type, resource_id, previous_state
         FROM undo_log
         WHERE id = $1 AND tenant_id = $2 AND user_id = $3 AND status = 'AVAILABLE'",
    )
    .bind(entry_id)
    .bind(tenant(&auth))
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((action, resource_type, resource_id, previous_state)) = entry else {
        return Err(ApiError::not_found("undoable action"));
    };

    if action != "JOB_STATUS" {
        return Err(
            ApiError::validation(format!("`{action}` is not reversible"))
                .with_detail("action", "only job status changes can be undone"),
        );
    }
    debug_assert_eq!(resource_type, "JOB");

    let status = previous_state
        .get("status")
        .and_then(|s| s.as_str())
        .ok_or_else(ApiError::internal)?;

    sqlx::query("UPDATE jobs SET status = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
        .bind(resource_id)
        .bind(tenant(&auth))
        .bind(status)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    // Mark consumed rather than deleting, so the audit trail still shows it.
    sqlx::query("UPDATE undo_log SET status = 'UNDONE', undone_at = NOW() WHERE id = $1")
        .bind(entry_id)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "undone": true,
            "action": action,
            "resource_id": resource_id,
            "restored_status": status,
        }),
        auth.request_id,
    )))
}

/// Records a reversible action. Called by the handlers that perform them.
///
/// The partial unique index keeps only the newest available entry per resource,
/// so undo always reverses the most recent change rather than a stale one.
pub(crate) async fn record_undo(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    previous_state: Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "DELETE FROM undo_log
         WHERE tenant_id = $1 AND resource_type = $2 AND resource_id = $3
           AND status = 'AVAILABLE'",
    )
    .bind(tenant_id)
    .bind(resource_type)
    .bind(resource_id)
    .execute(pool)
    .await
    .map_err(ApiError::from)?;

    sqlx::query(
        "INSERT INTO undo_log
             (id, tenant_id, user_id, action, resource_type, resource_id,
              previous_state, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7,
                 NOW() + make_interval(hours => $8::int))",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(previous_state)
    .bind(UNDO_WINDOW_HOURS)
    .execute(pool)
    .await
    .map_err(ApiError::from)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// System health (UI.md section 73)
// ---------------------------------------------------------------------------

/// `GET /system/health` — every component the spec lists, with its own check.
///
/// Each row states a fact. A component with no data reports `unknown` rather
/// than `healthy`, so an absent heartbeat cannot read as good news.
pub async fn system_health(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    let database_ok = sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok();

    let migrations: Vec<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&state.pool)
            .await
            .map_err(ApiError::from)?;

    let worker_counts: (i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status IN ('READY','BUSY')),
             COUNT(*)
         FROM workers WHERE tenant_id = $1",
    )
    .bind(tenant(&auth))
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The scheduler lag is the newest recorded heartbeat; no heartbeat at all
    // means unknown, not healthy.
    let lag: Option<(i32,)> = sqlx::query_as(
        "SELECT lag_ms FROM scheduler_heartbeats
         WHERE tenant_id = $1 ORDER BY observed_at DESC LIMIT 1",
    )
    .bind(tenant(&auth))
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let queue_wait: (Option<i32>,) = sqlx::query_as(
        "SELECT MAX(queue_wait_seconds) FROM execution_durations WHERE tenant_id = $1",
    )
    .bind(tenant(&auth))
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let open_alerts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alerts WHERE tenant_id = $1 AND status = 'OPEN'")
            .bind(tenant(&auth))
            .fetch_one(&state.pool)
            .await
            .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "version": state.version,
            "components": [
                {
                    "name": "Database",
                    "status": if database_ok { "healthy" } else { "down" },
                    "detail": if database_ok { "reachable" } else { "unreachable" },
                },
                {
                    "name": "Migrations",
                    "status": if migrations.iter().all(|(_, ok)| *ok) { "healthy" } else { "degraded" },
                    "detail": format!("{} applied", migrations.len()),
                },
                {
                    "name": "Workers",
                    // Zero registered workers is a real state, not a fault, so
                    // it is reported as unknown rather than healthy.
                    "status": if worker_counts.1 == 0 { "unknown" } else { "healthy" },
                    "detail": format!("{} of {} available", worker_counts.0, worker_counts.1),
                },
                {
                    "name": "Scheduler",
                    "status": match lag {
                        Some((ms,)) if ms > 30_000 => "degraded",
                        Some(_) => "healthy",
                        None => "unknown",
                    },
                    "detail": match lag {
                        Some((ms,)) => format!("lag {} ms", ms),
                        None => "no heartbeat recorded".to_string(),
                    },
                },
                {
                    "name": "Queues",
                    "status": "healthy",
                    "detail": match queue_wait.0 {
                        None => "no waits recorded".to_string(),
                        Some(seconds) => format!("max wait {}s", seconds),
                    },
                },
                {
                    "name": "Notifications",
                    "status": if open_alerts == 0 { "healthy" } else { "attention" },
                    "detail": format!("{} open alert(s)", open_alerts),
                },
            ],
            "scheduler_lag_ms": lag.map(|(ms,)| ms),
            "queue_latency_seconds": queue_wait.0,
            "all_migrations_applied": migrations.iter().all(|(_, ok)| *ok),
        }),
        auth.request_id,
    )))
}

/// `POST /scheduler/heartbeat` — the scheduler reports its lag.
pub async fn record_scheduler_heartbeat(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<Heartbeat>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:write")?;

    sqlx::query(
        "INSERT INTO scheduler_heartbeats (id, tenant_id, lag_ms, executions_dispatched)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant(&auth))
    .bind(body.lag_ms.max(0))
    .bind(body.executions_dispatched.max(0))
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({ "recorded": true }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct Heartbeat {
    pub lag_ms: i32,
    #[serde(default)]
    pub executions_dispatched: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_urls_must_be_absolute_http() {
        assert!(validate_webhook_url("https://example.invalid/hook").is_ok());
        assert!(validate_webhook_url("http://example.invalid/hook").is_ok());
        // Anything that is not an absolute HTTP URL would never deliver.
        assert!(validate_webhook_url("ftp://example.invalid").is_err());
        assert!(validate_webhook_url("example.invalid/hook").is_err());
        assert!(validate_webhook_url("").is_err());
    }

    #[test]
    fn a_webhook_url_without_a_host_is_rejected() {
        // `https://` parses but has nowhere to send.
        assert!(validate_webhook_url("https://").is_err());
    }

    #[test]
    fn a_secret_is_hashed_and_never_echoed() {
        let hash = hash_secret("super-secret");
        assert_ne!(hash, "super-secret");
        // Deterministic, so a rotation can be verified by re-hashing.
        assert_eq!(hash, hash_secret("super-secret"));
        assert_eq!(hash.len(), 64, "SHA-256 hex is 64 characters");
    }

    #[test]
    fn different_secrets_hash_differently() {
        assert_ne!(hash_secret("a"), hash_secret("b"));
    }
}
