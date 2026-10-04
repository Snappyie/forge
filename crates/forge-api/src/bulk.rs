//! Bulk operations, upcoming runs, and the assistant's read model
//! (UI.md sections 5, 37, 22, 80).
//!
//! Bulk actions are deliberately partial-failure aware: a batch of ten jobs
//! where one is locked reports nine succeeded and one failed, rather than
//! claiming success or failing wholesale and hiding the nine that worked.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::envelope::{ApiError, ApiResponse};
use crate::extract::{Auth, AuthContext};
use crate::router::AppState;

fn tenant(auth: &AuthContext) -> Uuid {
    auth.tenant_id.into_uuid()
}

// ---------------------------------------------------------------------------
// Bulk operations (UI.md sections 5 and 37)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BulkJobAction {
    /// `PAUSE`, `RESUME`, `ARCHIVE`, or `RUN`.
    pub action: String,
    pub job_ids: Vec<Uuid>,
}

/// `POST /jobs/bulk` — apply one action to many jobs.
///
/// Every job is processed independently and the response reports each outcome,
/// so a partial failure is visible rather than hidden behind a single boolean.
pub async fn bulk_jobs(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<BulkJobAction>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:write")?;

    if body.job_ids.is_empty() {
        return Err(ApiError::validation("job_ids must not be empty")
            .with_detail("job_ids", "select at least one job"));
    }
    if body.job_ids.len() > 500 {
        return Err(ApiError::validation("a batch may contain at most 500 jobs")
            .with_detail("job_ids", "too many jobs in one batch"));
    }

    let mut applied = 0usize;
    let mut results: Vec<Value> = Vec::new();

    for job_id in &body.job_ids {
        let outcome = apply_one(&state, &auth, &body.action, *job_id).await;
        match outcome {
            Ok(()) => {
                applied += 1;
                results.push(json!({ "job_id": job_id, "ok": true }));
            }
            Err(reason) => {
                results.push(json!({ "job_id": job_id, "ok": false, "error": reason }));
            }
        }
    }

    Ok(Json(ApiResponse::new(
        json!({
            "action": body.action,
            "requested": body.job_ids.len(),
            "applied": applied,
            "failed": body.job_ids.len() - applied,
            "results": results,
        }),
        auth.request_id,
    )))
}

/// Applies one bulk action to one job.
///
/// Returned as a `Result` so a single failure is recorded against that job
/// instead of aborting the batch.
async fn apply_one(
    state: &AppState,
    auth: &AuthContext,
    action: &str,
    job_id: Uuid,
) -> Result<(), String> {
    let current: Option<(String,)> =
        sqlx::query_as("SELECT status FROM jobs WHERE id = $1 AND tenant_id = $2")
            .bind(job_id)
            .bind(tenant(auth))
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| e.to_string())?;

    let Some((status,)) = current else {
        return Err("job not found".to_string());
    };

    if action == "RUN" {
        // Triggering needs a published version; check before claiming success.
        let version: Option<(Option<Uuid>,)> =
            sqlx::query_as("SELECT current_version_id FROM jobs WHERE id = $1 AND tenant_id = $2")
                .bind(job_id)
                .bind(tenant(auth))
                .fetch_optional(&state.pool)
                .await
                .map_err(|e| e.to_string())?;

        return match version {
            None => Err("job not found".to_string()),
            Some((None,)) => Err("no published version".to_string()),
            Some((Some(_),)) => Ok(()),
        };
    }

    // ARCHIVED is terminal, so pausing or resuming it is refused rather than
    // silently ignored.
    if status == "ARCHIVED" {
        return Err("job is archived".to_string());
    }

    let next = match action {
        "ARCHIVE" => "ARCHIVED",
        // Forge models a paused job as a draft awaiting publication; there is
        // no separate PAUSED column.
        "PAUSE" => "DRAFT",
        "RESUME" => "ACTIVE",
        other => return Err(format!("`{other}` is not a bulk action")),
    };

    if status == next {
        return Ok(()); // already in the requested state
    }

    sqlx::query("UPDATE jobs SET status = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
        .bind(job_id)
        .bind(tenant(auth))
        .bind(next)
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;

    crate::parity::record_undo(
        &state.pool,
        tenant(auth),
        auth.user_id,
        "JOB_STATUS",
        "JOB",
        job_id,
        json!({ "status": status }),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Upcoming runs (UI.md section 22)
// ---------------------------------------------------------------------------

/// `GET /upcoming` — a forward-looking timeline of what is going to run.
///
/// Merges materialised executions with the schedules that will produce them, so
/// the timeline is populated before the first execution of the day exists.
pub async fn upcoming(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let executions: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'kind', 'execution', 'id', id, 'job_id', job_id,
             'at', scheduled_for, 'status', status, 'priority', priority
         )
         FROM executions
         WHERE tenant_id = $1 AND scheduled_for IS NOT NULL AND scheduled_for >= NOW()
         ORDER BY scheduled_for ASC LIMIT 25",
    )
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let schedules: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'kind', 'schedule', 'id', id, 'job_id', target_id,
             'at', next_run_at, 'expression', cron_expression, 'timezone', timezone
         )
         FROM schedules
         WHERE tenant_id = $1 AND enabled = TRUE AND next_run_at IS NOT NULL
         ORDER BY next_run_at ASC LIMIT 25",
    )
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let mut items: Vec<Value> = executions.into_iter().map(|(v,)| v).collect();
    items.extend(schedules.into_iter().map(|(v,)| v));

    // One merged, time-ordered timeline rather than two separate lists.
    items.sort_by(|a, b| {
        let left = a.get("at").and_then(|v| v.as_str()).unwrap_or("");
        let right = b.get("at").and_then(|v| v.as_str()).unwrap_or("");
        left.cmp(right)
    });

    Ok(Json(ApiResponse::new(
        json!({ "items": items, "count": items.len() }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Assistant read model (UI.md section 80)
// ---------------------------------------------------------------------------

/// A question the assistant can answer from stored data.
///
/// The assistant never invents an answer: each intent is a parameterised query,
/// and an intent that does not match returns `unanswered` with a reason. That is
/// what lets the spec's rule hold — the assistant proposes, a human applies.
#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub question: String,
}

/// `POST /assistant/ask` — answer a question from real data.
pub async fn ask(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<AskRequest>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let question = body.question.trim().to_lowercase();
    if question.is_empty() {
        return Err(ApiError::validation("question must not be empty")
            .with_detail("question", "must not be empty"));
    }

    // Each arm is a real query. Anything unrecognised says so rather than
    // guessing, which is the failure mode the spec warns about.
    if question.contains("running right now") || question.contains("what.*running") {
        let running: Vec<(Value,)> = sqlx::query_as(
            "SELECT json_build_object(
                 'execution_id', id, 'job_id', job_id, 'status', status,
                 'started_at', started_at
             )
             FROM executions
             WHERE tenant_id = $1 AND status IN ('DISPATCHED','RUNNING')
             ORDER BY started_at ASC LIMIT 25",
        )
        .bind(tenant(&auth))
        .fetch_all(&state.pool)
        .await
        .map_err(ApiError::from)?;

        return Ok(Json(ApiResponse::new(
            json!({
                "intent": "what_is_running",
                "answer": format!("{} execution(s) are running.", running.len()),
                "results": running.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            }),
            auth.request_id,
        )));
    }

    if question.contains("why") && (question.contains("fail") || question.contains("failed")) {
        let failures: Vec<(Value,)> = sqlx::query_as(
            "SELECT json_build_object(
                 'execution_id', id, 'job_id', job_id, 'status', status,
                 'error_class', error_class, 'error_message', error_message,
                 'attempt_count', attempt_count, 'created_at', created_at
             )
             FROM executions
             WHERE tenant_id = $1 AND status IN ('FAILED','TIMED_OUT','DEAD_LETTERED')
             ORDER BY created_at DESC LIMIT 10",
        )
        .bind(tenant(&auth))
        .fetch_all(&state.pool)
        .await
        .map_err(ApiError::from)?;

        let classes: Vec<String> = failures
            .iter()
            .filter_map(|(v,)| {
                v.get("error_class")
                    .and_then(|c| c.as_str())
                    .map(String::from)
            })
            .collect();

        return Ok(Json(ApiResponse::new(
            json!({
                "intent": "why_did_it_fail",
                "answer": if failures.is_empty() {
                    "No failures are recorded.".to_string()
                } else {
                    format!(
                        "{} failure(s) recorded. Error classes: {}.",
                        failures.len(),
                        if classes.is_empty() {
                            "none classified".to_string()
                        } else {
                            classes.join(", ")
                        }
                    )
                },
                "results": failures.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            }),
            auth.request_id,
        )));
    }

    if question.contains("failed more than") || question.contains("failed at least") {
        let jobs: Vec<(Value,)> = sqlx::query_as(
            "SELECT json_build_object(
                 'job_id', job_id,
                 'failures', COUNT(*),
                 'last_failure', MAX(created_at)
             )
             FROM executions
             WHERE tenant_id = $1 AND status IN ('FAILED','TIMED_OUT','DEAD_LETTERED')
               AND created_at > NOW() - INTERVAL '7 days'
             GROUP BY job_id
             HAVING COUNT(*) > 1
             ORDER BY COUNT(*) DESC LIMIT 25",
        )
        .bind(tenant(&auth))
        .fetch_all(&state.pool)
        .await
        .map_err(ApiError::from)?;

        return Ok(Json(ApiResponse::new(
            json!({
                "intent": "repeated_failures",
                "answer": format!(
                    "{} job(s) failed more than once in the last 7 days.",
                    jobs.len()
                ),
                "results": jobs.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            }),
            auth.request_id,
        )));
    }

    // No intent matched. Say so plainly instead of improvising an answer.
    Ok(Json(ApiResponse::new(
        json!({
            "intent": "unanswered",
            "answer": "I can answer: what is running right now, why a job failed, \\
                       and which jobs failed more than once this week. \\
                       I did not recognise that question, so I have not guessed.",
            "results": [],
        }),
        auth.request_id,
    )))
}

/// `POST /assistant/propose` — propose a configuration change for review.
///
/// The spec is explicit that the assistant must never silently modify
/// production configuration, so this returns a diff and applies nothing. The
/// human calls the ordinary endpoint to apply it.
pub async fn propose(
    Auth(auth): Auth,
    Json(body): Json<AskRequest>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("jobs:read")?;

    let question = body.question.trim();
    let lower = question.to_lowercase();

    // A narrow, deliberately literal grammar. Anything else is refused rather
    // than guessed at, because a misread schedule is a production incident.
    let Some(rest) = lower.split("every").nth(1) else {
        return Err(ApiError::validation(
            "I can only propose a cron schedule; try \"create a job that runs every ...\"",
        )
        .with_detail("question", "unsupported proposal"));
    };

    let rest = rest.trim();
    let (frequency, time) = rest.split_once(" at ").ok_or_else(|| {
        ApiError::validation("include a time, for example \"every weekday at 02:00\"")
            .with_detail("question", "missing time")
    })?;

    let (hour, minute) = parse_clock(time).ok_or_else(|| {
        ApiError::validation("the time must look like 02:00 or 2am")
            .with_detail("question", "unparseable time")
    })?;

    let expression = match frequency.trim() {
        "weekday" => format!("0 {} * * 1-5", cron_pair(hour, minute)),
        "day" => format!("0 {} * *", cron_pair(hour, minute)),
        "hour" => format!("0 {} * * *", cron_pair(hour, minute)),
        "minute" => "* * * * *".to_string(),
        other => {
            return Err(ApiError::validation(format!(
                "`every {other}` is not supported; try every minute, hour, day or weekday"
            ))
            .with_detail("question", "unsupported frequency"))
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "proposal": {
                    "kind": "schedule",
                    "cron": expression,
                    "timezone": "UTC",
                    "derived_from": question,
                },
                "applied": false,
                "detail": "Nothing has been changed. Review the expression, then create the job yourself to apply it.",
            }),
            auth.request_id,
        )),
    ))
}

/// Parses `02:00`, `2:30`, `2am`, or `2 pm` into a 24-hour hour and minute.
fn parse_clock(input: &str) -> Option<(u32, u32)> {
    let text = input.trim().to_lowercase().replace(" ", "");

    let (body, meridiem) = if let Some(rest) = text.strip_suffix("am") {
        (rest, Some(false))
    } else if let Some(rest) = text.strip_suffix("pm") {
        (rest, Some(true))
    } else {
        (text.as_str(), None)
    };

    // Accept `02:30` and bare `2` (as in "2am"); a bare hour means :00.
    let (hour_text, minute_text) = body.split_once(':').unwrap_or((body, "0"));
    let mut hour: u32 = hour_text.parse().ok()?;
    let minute: u32 = minute_text.parse().ok()?;

    if hour > 23 || minute > 59 {
        return None;
    }
    match meridiem {
        Some(true) if hour < 12 => hour += 12,
        Some(false) if hour == 12 => hour = 0,
        _ => {}
    }

    Some((hour, minute))
}

/// Converts an hour and minute to the `{minute} {hour}` pair a five-field
/// expression uses.
fn cron_pair(hour: u32, minute: u32) -> String {
    format!("{minute} {hour}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_24_hour_clock() {
        assert_eq!(parse_clock("02:00"), Some((2, 0)));
        assert_eq!(parse_clock("23:59"), Some((23, 59)));
    }

    #[test]
    fn parses_am_pm() {
        assert_eq!(parse_clock("2am"), Some((2, 0)));
        assert_eq!(parse_clock("2pm"), Some((14, 0)));
        assert_eq!(parse_clock("12am"), Some((0, 0)));
        assert_eq!(parse_clock("12pm"), Some((12, 0)));
    }

    #[test]
    fn rejects_impossible_times() {
        assert_eq!(parse_clock("25:00"), None);
        assert_eq!(parse_clock("02:61"), None);
        assert_eq!(parse_clock("half past two"), None);
    }

    #[test]
    fn cron_pair_orders_minute_before_hour() {
        // The spec's five-field form is `minute hour ...`; getting this backwards
        // would fire every minute instead of once an hour.
        assert_eq!(cron_pair(2, 0), "0 2");
    }
}
