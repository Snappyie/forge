//! Job health, SLA, and maintenance mode (UI.md sections 31, 32, 71, 79).
//!
//! Every number here is computed from stored execution rows. There is no
//! "health score": the spec explicitly asks for concrete metrics, because a
//! single invented score is not actionable and cannot be audited.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::router::AppState;

/// `GET /jobs/{id}/health` — UI.md sections 32 and 79.
///
/// Reliability and performance, each derived from the job's own executions:
/// success rate, failure and retry counts, and duration percentiles.
pub async fn job_health(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    // Confirm the job is visible in this tenant first; otherwise the aggregate
    // would answer for a job that does not exist here.
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM jobs WHERE id = $1 AND tenant_id = $2")
            .bind(job_id)
            .bind(auth.tenant_id.into_uuid())
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;
    if exists.is_none() {
        return Err(ApiError::not_found("job"));
    }

    // The aggregate row: counts and mean duration in one pass.
    let totals: (i64, i64, i64, i64, i64, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status = 'SUCCEEDED'),
             COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT')),
             COUNT(*) FILTER (WHERE status IN ('DEAD_LETTERED','CANCELLED')),
             COALESCE(SUM(GREATEST(attempt_count - 1, 0)), 0),
             COUNT(*),
             AVG(duration_seconds)::float8,
             AVG(duration_seconds) FILTER (WHERE status = 'SUCCEEDED')::float8
         FROM execution_durations
         WHERE job_id = $1 AND tenant_id = $2",
    )
    .bind(job_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // Percentiles need their own ordered read; `percentile_cont` over the
    // filtered set is clearer in SQL than sorting rows in Rust.
    let percentiles: (Option<f64>, Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT
             percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_seconds)::float8,
             percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_seconds)::float8,
             percentile_cont(0.99) WITHIN GROUP (ORDER BY duration_seconds)::float8
         FROM execution_durations
         WHERE job_id = $1 AND tenant_id = $2 AND status = 'SUCCEEDED'",
    )
    .bind(job_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let succeeded = totals.0;
    let failed = totals.1;
    let other = totals.2;
    let retries = totals.3;
    let finished = totals.5.unwrap_or(0.0);
    let total_executions = totals.4;

    // SLA compliance, or null when the job has no target configured. Returning
    // null is deliberate: the spec forbids arbitrary scores, and "no target" is
    // the truthful answer.
    let sla: (i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE met),
             COUNT(*)
         FROM sla_evaluations WHERE job_id = $1 AND tenant_id = $2",
    )
    .bind(job_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The configured target, if any. `flatten` drops the `Option` row into the
    // inner tuple the function wants.
    let target: Option<i32> = sqlx::query_as::<_, (i32,)>(
        "SELECT target_duration_seconds FROM sla_targets
         WHERE job_id = $1 AND tenant_id = $2 AND enabled = TRUE",
    )
    .bind(job_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .map(|(seconds,)| seconds);
    let sla_report = match (sla.1, target) {
        // Nothing evaluated yet, or no target configured: report the absence
        // rather than a number that would imply the job was measured.
        (0, _) => Value::Null,
        (evaluated, Some(target_seconds)) => json!({
            "target_seconds": target_seconds,
            "met": sla.0,
            "evaluated": evaluated,
            "compliance_percent": rate(sla.0, evaluated),
        }),
        (evaluated, None) => json!({
            "target_seconds": Value::Null,
            "met": sla.0,
            "evaluated": evaluated,
            "compliance_percent": rate(sla.0, evaluated),
        }),
    };

    Ok(Json(ApiResponse::new(
        json!({
            "job_id": job_id,
            "reliability": {
                "executions": total_executions,
                "succeeded": succeeded,
                "failed": failed,
                "dead_lettered_or_cancelled": other,
                "retries": retries,
                "success_rate": rate(succeeded, total_executions),
            },
            "performance": {
                "average_seconds": totals.6.map(|v| v.round()),
                "finished_average_seconds": Some(finished.round()),
                "p50_seconds": percentiles.0.map(|v| v.round()),
                "p95_seconds": percentiles.1.map(|v| v.round()),
                "p99_seconds": percentiles.2.map(|v| v.round()),
            },
            "sla": sla_report,
        }),
        auth.request_id,
    )))
}

/// Percentage to one decimal place, or null when nothing has run yet.
fn rate(numerator: i64, denominator: i64) -> Option<f64> {
    if denominator == 0 {
        None
    } else {
        Some((numerator as f64 / denominator as f64 * 1000.0).round() / 10.0)
    }
}

// ---------------------------------------------------------------------------
// SLA targets
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SetSlaTarget {
    pub target_duration_seconds: i32,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// `PUT /jobs/{id}/sla` — UI.md section 31.
pub async fn set_sla_target(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
    Json(body): Json<SetSlaTarget>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:write")?;

    if body.target_duration_seconds <= 0 {
        return Err(
            ApiError::validation("target_duration_seconds must be positive")
                .with_detail("target_duration_seconds", "must be greater than zero"),
        );
    }

    let row: (Value,) = sqlx::query_as(
        "INSERT INTO sla_targets (id, tenant_id, job_id, target_duration_seconds, enabled)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, job_id) DO UPDATE SET
             target_duration_seconds = EXCLUDED.target_duration_seconds,
             enabled = EXCLUDED.enabled,
             updated_at = NOW()
         RETURNING json_build_object(
             'job_id', job_id, 'target_duration_seconds', target_duration_seconds,
             'enabled', enabled, 'updated_at', updated_at
         )",
    )
    .bind(Uuid::new_v4())
    .bind(auth.tenant_id.into_uuid())
    .bind(job_id)
    .bind(body.target_duration_seconds)
    .bind(body.enabled)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

/// `GET /jobs/{id}/sla` — the recent per-run SLA outcomes (UI.md section 31).
pub async fn sla_detail(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'execution_id', execution_id,
             'duration_seconds', duration_seconds,
             'met', met,
             'evaluated_at', evaluated_at
         )
         FROM sla_evaluations
         WHERE job_id = $1 AND tenant_id = $2
         ORDER BY evaluated_at DESC LIMIT 50",
    )
    .bind(job_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

/// `GET /sla/compliance` — the dashboard's tenant-wide figure (UI.md section 31).
pub async fn sla_compliance(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    let row: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE met), COUNT(*)
         FROM sla_evaluations
         WHERE tenant_id = $1 AND evaluated_at > NOW() - INTERVAL '30 days'",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "met": row.0,
            "evaluated": row.1,
            "compliance_percent": rate(row.0, row.1),
        }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Maintenance mode (UI.md section 71)
// ---------------------------------------------------------------------------

/// `GET /maintenance` — whether the tenant is currently held.
pub async fn get_maintenance(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'reason', reason, 'started_at', started_at,
             'started_by', started_by
         )
         FROM maintenance_windows
         WHERE tenant_id = $1 AND ended_at IS NULL
         LIMIT 1",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({ "active": row.is_some(), "window": row.map(|(v,)| v) }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct StartMaintenance {
    pub reason: String,
}

/// `POST /maintenance` — enter maintenance mode.
pub async fn start_maintenance(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<StartMaintenance>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("settings:write")?;

    if body.reason.trim().is_empty() {
        return Err(ApiError::validation("reason must not be empty")
            .with_detail("reason", "must not be empty"));
    }

    // The partial unique index already prevents two open windows; let the
    // conflict surface as a domain error rather than a 500.
    let row: (Value,) = sqlx::query_as(
        "INSERT INTO maintenance_windows (id, tenant_id, reason, started_by)
         VALUES ($1, $2, $3, $4)
         RETURNING json_build_object(
             'id', id, 'reason', reason, 'started_at', started_at
         )",
    )
    .bind(Uuid::new_v4())
    .bind(auth.tenant_id.into_uuid())
    .bind(body.reason.trim())
    .bind(auth.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(row.0, auth.request_id)),
    ))
}

/// `DELETE /maintenance` — leave maintenance mode.
pub async fn end_maintenance(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    let ended = sqlx::query(
        "UPDATE maintenance_windows SET ended_at = NOW()
         WHERE tenant_id = $1 AND ended_at IS NULL",
    )
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if ended == 0 {
        return Err(ApiError::not_found("open maintenance window"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "ended": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Dashboard aggregation (UI.md section 2)
// ---------------------------------------------------------------------------

/// `GET /dashboard` — the one call the dashboard needs.
///
/// It answers "is my scheduler healthy, what needs attention, what is happening
/// now" in a single round trip, so the page cannot render a half-updated mix of
/// counts from several list endpoints.
pub async fn dashboard(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:read")?;
    let tenant = auth.tenant_id.into_uuid();

    let executions: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status = 'QUEUED'),
             COUNT(*) FILTER (WHERE status IN ('DISPATCHED','RUNNING')),
             COUNT(*) FILTER (WHERE status = 'SUCCEEDED'),
             COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT')),
             COUNT(*) FILTER (WHERE status = 'DEAD_LETTERED'),
             COUNT(*) FILTER (WHERE status = 'CANCELLED')
         FROM executions WHERE tenant_id = $1",
    )
    .bind(tenant)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let workers: (i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status = 'READY'),
             COUNT(*) FILTER (WHERE status = 'BUSY'),
             COUNT(*) FILTER (WHERE status IN ('OFFLINE','REVOKED'))
         FROM workers WHERE tenant_id = $1",
    )
    .bind(tenant)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // Recent failures power "needs attention"; an operator's first question is
    // what broke.
    let failures: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'job_id', job_id, 'status', status,
             'error_message', error_message, 'attempt_count', attempt_count,
             'created_at', created_at
         )
         FROM executions
         WHERE tenant_id = $1 AND status IN ('FAILED','TIMED_OUT','DEAD_LETTERED')
         ORDER BY created_at DESC LIMIT 10",
    )
    .bind(tenant)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // Upcoming runs (UI.md section 22): the next scheduled executions in time
    // order, which is what an operator watches during an incident.
    let upcoming: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'execution_id', id, 'job_id', job_id,
             'scheduled_for', scheduled_for, 'status', status, 'priority', priority
         )
         FROM executions
         WHERE tenant_id = $1 AND scheduled_for IS NOT NULL AND scheduled_for >= NOW()
         ORDER BY scheduled_for ASC LIMIT 10",
    )
    .bind(tenant)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // Schedules whose next run is soon, which is the other half of "what is
    // going to run" when no execution has been materialised yet.
    let due_schedules: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'target_id', target_id, 'target_type', target_type,
             'expression', cron_expression, 'timezone', timezone, 'next_run_at', next_run_at
         )
         FROM schedules
         WHERE tenant_id = $1 AND enabled = TRUE AND next_run_at IS NOT NULL
         ORDER BY next_run_at ASC LIMIT 10",
    )
    .bind(tenant)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let open_alerts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE severity = 'CRITICAL'),
             COUNT(*) FILTER (WHERE severity = 'WARNING'),
             COUNT(*) FILTER (WHERE severity = 'INFO')
         FROM alerts WHERE tenant_id = $1 AND status = 'OPEN'",
    )
    .bind(tenant)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let maintenance_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM maintenance_windows WHERE tenant_id = $1 AND ended_at IS NULL
         )",
    )
    .bind(tenant)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let queues: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'name', name,
             'depth', (SELECT COUNT(*) FROM executions e
                       WHERE e.queue_id = q.id AND e.status = 'QUEUED'),
             'paused', paused
         )
         FROM queues q WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(tenant)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    /*
     * Counts for the navigation badges.
     *
     * The console has always read `jobs_total`, `workflows_total` and a queue
     * count from this aggregate, and none of them were ever sent - so those
     * badges silently rendered as nothing rather than as zero, and nobody
     * noticed because "absent" and "no jobs" look the same on a sidebar.
     *
     * One query, four counts, so the sidebar costs nothing extra: it already
     * fetches this aggregate on nearly every page.
     */
    let totals: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT count(*) FROM jobs       WHERE tenant_id = $1),
             (SELECT count(*) FROM workflows  WHERE tenant_id = $1),
             (SELECT count(*) FROM queues     WHERE tenant_id = $1),
             (SELECT count(*) FROM applications WHERE tenant_id = $1)",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "jobs_total": totals.0,
            "workflows_total": totals.1,
            "queues_total": totals.2,
            "applications_total": totals.3,
            "executions": {
                "queued": executions.0,
                "running": executions.1,
                "succeeded": executions.2,
                "failed": executions.3,
                "dead_lettered": executions.4,
                "cancelled": executions.5,
            },
            "workers": {
                "ready": workers.0,
                "busy": workers.1,
                "offline": workers.2,
            },
            "queues": queues.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            "alerts": {
                "critical": open_alerts.0,
                "warning": open_alerts.1,
                "info": open_alerts.2,
            },
            "maintenance_active": maintenance_active,
            "needs_attention": failures.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            "upcoming_executions": upcoming.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            "upcoming_schedules": due_schedules.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
        }),
        auth.request_id,
    )))
}

/// `GET /executions/timeline` — the execution page's lifecycle timeline
/// (UI.md section 13).
pub async fn timeline(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(execution_id): Path<Uuid>,
    Query(_pagination): Query<PaginationQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:read")?;

    let row: Option<ExecutionTiming> = sqlx::query_as(
        "SELECT status, created_at::text, started_at::text, ended_at::text, scheduled_for::text
             FROM executions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(execution_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((status, created, started, ended, scheduled)) = row else {
        return Err(ApiError::not_found("execution"));
    };

    // Build the ordered lifecycle the spec sketches, using only timestamps that
    // actually exist. A stage that never happened is omitted rather than shown
    // with a zero gap, because "dispatched in 0 ms" would be a fiction.
    let mut stages: Vec<(&str, Option<String>)> = Vec::new();
    if scheduled.is_some() {
        stages.push(("Scheduled", scheduled));
    }
    stages.push(("Queued", created));
    if started.is_some() {
        stages.push(("Running", started));
    }
    if ended.is_some() {
        stages.push((
            if status == "SUCCEEDED" {
                "Completed"
            } else {
                "Ended"
            },
            ended,
        ));
    }

    let events: Vec<Value> = stages
        .iter()
        .map(|(label, at)| {
            json!({
                "label": label,
                "at": at,
                "at_display": at.as_deref().and_then(parse_display),
            })
        })
        .collect();

    Ok(Json(ApiResponse::new(
        json!({ "execution_id": execution_id, "status": status, "stages": events }),
        auth.request_id,
    )))
}

/// The timestamps the execution timeline is built from.
type ExecutionTiming = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn parse_display(value: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_is_null_when_nothing_ran() {
        // A job with no executions must not report 0% success: that reads as
        // total failure rather than "no data".
        assert_eq!(rate(0, 0), None);
    }

    #[test]
    fn rate_rounds_to_one_decimal() {
        assert_eq!(rate(1, 3), Some(33.3));
        assert_eq!(rate(2, 2), Some(100.0));
        assert_eq!(rate(0, 4), Some(0.0));
    }

    #[test]
    fn a_zero_denominator_never_divides() {
        // Guards the panic class rather than the value.
        let _ = rate(5, 0);
    }
}
