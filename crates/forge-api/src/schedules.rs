//! Schedule endpoints (spec 05 endpoints 10–16).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_scheduler::{CronSchedule, MisfirePolicy};
use forge_storage::ScheduleRepository;

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::router::AppState;

/// `POST /schedules` (spec 05 endpoint 10).
#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    /// A job or a workflow.
    #[serde(default)]
    pub target_type: String,
    pub target_id: Uuid,
    #[serde(default)]
    pub target_version_policy: String,
    #[serde(default)]
    pub schedule_type: String,
    /// Five-field cron. Ignored for one-time schedules.
    pub expression: Option<String>,
    /// IANA timezone. Required for recurring schedules (spec 09.5).
    pub timezone: String,
    #[serde(default)]
    pub misfire_policy: String,
    #[serde(default)]
    pub catch_up_limit: Option<u32>,
    /// For a one-time schedule.
    pub one_time_at: Option<DateTime<Utc>>,
    /// For an interval schedule, in seconds.
    pub interval_seconds: Option<i64>,
}

pub async fn create(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("schedules:write")?;

    let schedule_type = parse_schedule_type(&body.schedule_type)?;
    let misfire = MisfirePolicy::parse(&body.misfire_policy);

    // Both of these have defaults, so an omitted field must become the
    // documented default rather than an empty string that violates a CHECK.
    let target_type = match body.target_type.trim().to_ascii_uppercase().as_str() {
        "" | "JOB" => "JOB",
        "WORKFLOW" => "WORKFLOW",
        other => {
            return Err(ApiError::validation(format!(
                "`{other}` is not a valid target type"
            ))
            .with_detail("target_type", "expected JOB or WORKFLOW"))
        }
    };
    let version_policy = match body.target_version_policy.trim().to_ascii_uppercase().as_str() {
        "" | "LATEST_PUBLISHED" => "LATEST_PUBLISHED",
        "PINNED" => "PINNED",
        other => {
            return Err(ApiError::validation(format!(
                "`{other}` is not a valid version policy"
            ))
            .with_detail(
                "target_version_policy",
                "expected PINNED or LATEST_PUBLISHED",
            ))
        }
    };

    // A recurring schedule must be evaluable before it is stored, so an
    // invalid expression is rejected at creation rather than at fire time.
    if schedule_type == forge_domain::ScheduleType::Cron {
        let expression = body.expression.as_deref().ok_or_else(|| {
            ApiError::validation("a cron schedule requires an expression")
                .with_detail("expression", "required for CRON")
        })?;
        CronSchedule::parse(expression, &body.timezone).map_err(|e| {
            ApiError::validation(e.to_string()).with_detail("expression", "invalid cron expression")
        })?;
    }

    if let Some(seconds) = body.interval_seconds {
        if schedule_type == forge_domain::ScheduleType::Interval && seconds <= 0 {
            return Err(ApiError::validation("interval_seconds must be positive")
                .with_detail("interval_seconds", "must be greater than zero"));
        }
    }

    let next_run_at = compute_next_run(&body, schedule_type)?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO schedules
            (id, tenant_id, job_id, target_id, target_type, target_version_policy,
             schedule_type, cron_expression, timezone, misfire_policy, catch_up_policy,
             next_run_at, enabled)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, TRUE)",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    // `job_id` still carries a foreign key to `jobs`; a workflow-targeted
    // schedule leaves it null, which the column allows.
    .bind(if target_type == "JOB" {
        Some(body.target_id)
    } else {
        None
    })
    .bind(body.target_id)
    .bind(target_type)
    .bind(version_policy)
    .bind(schedule_type_name(schedule_type))
    .bind(&body.expression)
    .bind(&body.timezone)
    .bind(misfire_name(misfire))
    .bind(json!({ "max_occurrences": body.catch_up_limit.unwrap_or(100) }))
    .bind(next_run_at)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "id": id,
                "target_id": body.target_id,
                "target_type": target_type,
                "schedule_type": schedule_type_name(schedule_type),
                "expression": body.expression,
                "timezone": body.timezone,
                "misfire_policy": misfire_name(misfire),
                "catch_up_limit": body.catch_up_limit.unwrap_or(100),
                "enabled": true,
                "next_run_at": next_run_at,
            }),
            auth.request_id,
        )),
    ))
}

fn parse_schedule_type(raw: &str) -> Result<forge_domain::ScheduleType, ApiError> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "" | "CRON" => Ok(forge_domain::ScheduleType::Cron),
        "ONE_TIME" => Ok(forge_domain::ScheduleType::OneTime),
        "INTERVAL" => Ok(forge_domain::ScheduleType::Interval),
        other => Err(ApiError::validation(format!("`{other}` is not a schedule type"))
            .with_detail("schedule_type", "expected CRON, ONE_TIME or INTERVAL")),
    }
}

fn schedule_type_name(kind: forge_domain::ScheduleType) -> &'static str {
    match kind {
        forge_domain::ScheduleType::Cron => "CRON",
        forge_domain::ScheduleType::OneTime => "ONE_TIME",
        forge_domain::ScheduleType::Interval => "INTERVAL",
    }
}

fn misfire_name(policy: MisfirePolicy) -> &'static str {
    match policy {
        MisfirePolicy::Skip => "SKIP",
        MisfirePolicy::FireOnce => "FIRE_ONCE",
        MisfirePolicy::CatchUp => "CATCH_UP",
    }
}

/// Computes the first occurrence, using the same engine the scheduler will use
/// so the preview cannot disagree with reality (spec 9.13).
fn compute_next_run(
    body: &CreateScheduleRequest,
    kind: forge_domain::ScheduleType,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    let now = Utc::now();
    Ok(match kind {
        forge_domain::ScheduleType::Cron => {
            let cron = CronSchedule::parse(
                body.expression.as_deref().unwrap_or_default(),
                &body.timezone,
            )
            .map_err(|e| ApiError::validation(e.to_string()))?;
            cron.next_after(now)
        }
        forge_domain::ScheduleType::OneTime => body.one_time_at,
        forge_domain::ScheduleType::Interval => body
            .interval_seconds
            .map(|s| now + chrono::Duration::seconds(s)),
    })
}

/// `GET /schedules` (spec 05 endpoint 11).
pub async fn list(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:read")?;

    let rows = list_schedules(&state.pool, auth.tenant_id, &pagination).await?;
    Ok(Json(ListResponse::new(
        rows,
        Default::default(),
        auth.request_id,
    )))
}

async fn list_schedules(
    pool: &sqlx::PgPool,
    tenant: forge_domain::TenantId,
    pagination: &PaginationQuery,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'target_id', target_id, 'target_type', target_type,
             'expression', cron_expression, 'timezone', timezone,
             'misfire_policy', misfire_policy, 'catch_up_limit',
                 catch_up_policy->>'max_occurrences',
             'enabled', enabled, 'next_run_at', next_run_at,
             'last_run_at', last_run_at, 'created_at', created_at
         )
         FROM schedules
         WHERE tenant_id = $1
         ORDER BY next_run_at ASC NULLS LAST
         LIMIT $2",
    )
    .bind(tenant.into_uuid())
    .bind(pagination.effective_limit() as i64)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from)?;

    Ok(rows.into_iter().map(|(v,)| v).collect())
}

/// `GET /schedules/{id}` (spec 05 endpoint 12).
pub async fn get(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:read")?;

    let row: (serde_json::Value,) = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'target_id', target_id, 'target_type', target_type,
             'expression', cron_expression, 'timezone', timezone,
             'misfire_policy', misfire_policy, 'enabled', enabled,
             'next_run_at', next_run_at, 'last_run_at', last_run_at
         )
         FROM schedules WHERE id = $1 AND tenant_id = $2",
    )
    .bind(schedule_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("schedule"))?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    pub expression: Option<String>,
    pub timezone: Option<String>,
    pub misfire_policy: Option<String>,
    pub catch_up_limit: Option<u32>,
}

/// `PATCH /schedules/{id}` (spec 05 endpoint 13).
pub async fn update(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(schedule_id): Path<Uuid>,
    Json(body): Json<UpdateScheduleRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:write")?;

    let affected = sqlx::query(
        "UPDATE schedules SET
             cron_expression = COALESCE($3, cron_expression),
             timezone = COALESCE($4, timezone),
             misfire_policy = COALESCE($5, misfire_policy),
             catch_up_policy = COALESCE($6, catch_up_policy),
             updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(schedule_id)
    .bind(auth.tenant_id.into_uuid())
    .bind(&body.expression)
    .bind(&body.timezone)
    .bind(&body.misfire_policy)
    .bind(body.catch_up_limit.map(|n| json!({ "max_occurrences": n })))
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("schedule"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": schedule_id, "updated": true }),
        auth.request_id,
    )))
}

/// `POST /schedules/{id}/pause` (spec 05 endpoint 14).
///
/// Spec 09.11: pausing prevents new scheduled executions but does not cancel
/// anything already running.
pub async fn pause(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:write")?;

    ScheduleRepository::new(&state.pool)
        .pause(auth.tenant_id, schedule_id)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({ "id": schedule_id, "enabled": false }),
        auth.request_id,
    )))
}

/// `POST /schedules/{id}/resume` (spec 05 endpoint 15).
pub async fn resume(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:write")?;

    ScheduleRepository::new(&state.pool)
        .resume(auth.tenant_id, schedule_id)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({ "id": schedule_id, "enabled": true }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize, Default)]
pub struct PreviewRequest {
    #[serde(default = "default_preview_count")]
    pub count: usize,
}

fn default_preview_count() -> usize {
    10
}

/// `POST /schedules/{id}/preview` (spec 05 endpoint 16).
///
/// Spec 9.13 and 7.7: the preview calls the same engine the scheduler uses, so
/// it can never disagree with what actually fires. DST anomalies are flagged so
/// the UI can show them (spec 7.7).
pub async fn preview(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(schedule_id): Path<Uuid>,
    Json(body): Json<PreviewRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:read")?;

    let row: (Option<String>, String, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT cron_expression, timezone, next_run_at
         FROM schedules WHERE id = $1 AND tenant_id = $2",
    )
    .bind(schedule_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("schedule"))?;

    let (expression, timezone, next_run_at) = row;
    let Some(expression) = expression else {
        return Ok(Json(ApiResponse::new(
            json!({ "occurrences": [], "note": "this schedule has no cron expression" }),
            auth.request_id,
        )));
    };

    let count = body.count.clamp(1, 50);
    let explanation = forge_scheduler::explain_schedule(
        &expression,
        &timezone,
        Utc::now(),
    )
    .map_err(|e| ApiError::validation(e.to_string()))?;

    let occurrences = forge_scheduler::preview_occurrences(
        &expression,
        &timezone,
        Utc::now(),
        count,
    )
    .map_err(|e| ApiError::validation(e.to_string()))?;

    Ok(Json(ApiResponse::new(
        json!({
            "expression": expression,
            "timezone": timezone,
            "next_run_at": next_run_at,
            "occurrences": occurrences,
            "anomalies": dst_anomalies(&expression, &timezone, &occurrences),
            "engine": explanation.expression,
        }),
        auth.request_id,
    )))
}

/// Flags occurrences that fall in a DST gap or fold.
///
/// Spec 7.7 requires DST anomalies to be indicated explicitly rather than left
/// for the user to notice.
fn dst_anomalies(
    expression: &str,
    timezone: &str,
    occurrences: &[DateTime<Utc>],
) -> Vec<serde_json::Value> {
    let Ok(cron) = CronSchedule::parse(expression, timezone) else {
        return Vec::new();
    };
    use chrono::TimeZone;

    let mut anomalies = Vec::new();
    for occurrence in occurrences {
        let local = occurrence.with_timezone(&cron.timezone());
        // Rebuild the wall clock and ask the tz database what kind of time it
        // is.
        let naive = chrono::NaiveDateTime::new(
            local.date_naive(),
            local.time(),
        );
        match cron.classify_local(naive) {
            forge_scheduler::LocalTimeKind::Unique => {}
            forge_scheduler::LocalTimeKind::Nonexistent => anomalies.push(json!({
                "at": occurrence,
                "kind": "NONEXISTENT_LOCAL_TIME",
                "note": "this local time does not exist; it was shifted forward by an hour",
            })),
            forge_scheduler::LocalTimeKind::AmbiguousFirst
            | forge_scheduler::LocalTimeKind::AmbiguousSecond => anomalies.push(json!({
                "at": occurrence,
                "kind": "AMBIGUOUS_LOCAL_TIME",
                "note": "this local time occurs twice; it fires once, on the first occurrence",
            })),
        }
        // Silence an unused-import warning while keeping `TimeZone` available
        // for the `with_timezone` calls above.
        let _ = Utc.timestamp_opt(0, 0);
    }
    anomalies
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn schedule_types_parse() {
        assert_eq!(
            parse_schedule_type("").unwrap(),
            forge_domain::ScheduleType::Cron
        );
        assert_eq!(
            parse_schedule_type("cron").unwrap(),
            forge_domain::ScheduleType::Cron
        );
        assert_eq!(
            parse_schedule_type("ONE_TIME").unwrap(),
            forge_domain::ScheduleType::OneTime
        );
        assert!(parse_schedule_type("EVERY_FORTNIGHT").is_err());
    }

    #[test]
    fn misfire_names_match_the_storage_vocabulary() {
        assert_eq!(misfire_name(MisfirePolicy::Skip), "SKIP");
        assert_eq!(misfire_name(MisfirePolicy::FireOnce), "FIRE_ONCE");
        assert_eq!(misfire_name(MisfirePolicy::CatchUp), "CATCH_UP");
    }

    #[test]
    fn schedule_type_names_match_the_storage_vocabulary() {
        assert_eq!(
            schedule_type_name(forge_domain::ScheduleType::Cron),
            "CRON"
        );
        assert_eq!(
            schedule_type_name(forge_domain::ScheduleType::OneTime),
            "ONE_TIME"
        );
        assert_eq!(
            schedule_type_name(forge_domain::ScheduleType::Interval),
            "INTERVAL"
        );
    }

    #[test]
    fn a_cron_next_run_is_computed_from_the_shared_engine() {
        let body = CreateScheduleRequest {
            target_type: "JOB".into(),
            target_id: Uuid::new_v4(),
            target_version_policy: "LATEST_PUBLISHED".into(),
            schedule_type: "CRON".into(),
            expression: Some("0 2 * * *".into()),
            timezone: "UTC".into(),
            misfire_policy: "FIRE_ONCE".into(),
            catch_up_limit: None,
            one_time_at: None,
            interval_seconds: None,
        };
        let next = compute_next_run(&body, forge_domain::ScheduleType::Cron)
            .unwrap()
            .expect("a cron schedule always has a next occurrence");
        assert!(next > Utc::now());
    }

    #[test]
    fn a_one_time_schedule_uses_its_explicit_instant() {
        let at = Utc::now() + chrono::Duration::days(1);
        let body = CreateScheduleRequest {
            target_type: "JOB".into(),
            target_id: Uuid::new_v4(),
            target_version_policy: "LATEST_PUBLISHED".into(),
            schedule_type: "ONE_TIME".into(),
            expression: None,
            timezone: "UTC".into(),
            misfire_policy: "FIRE_ONCE".into(),
            catch_up_limit: None,
            one_time_at: Some(at),
            interval_seconds: None,
        };
        assert_eq!(
            compute_next_run(&body, forge_domain::ScheduleType::OneTime).unwrap(),
            Some(at)
        );
    }

    #[test]
    fn an_interval_schedule_uses_its_period() {
        let body = CreateScheduleRequest {
            target_type: "JOB".into(),
            target_id: Uuid::new_v4(),
            target_version_policy: "LATEST_PUBLISHED".into(),
            schedule_type: "INTERVAL".into(),
            expression: None,
            timezone: "UTC".into(),
            misfire_policy: "FIRE_ONCE".into(),
            catch_up_limit: None,
            one_time_at: None,
            interval_seconds: Some(3600),
        };
        let next = compute_next_run(&body, forge_domain::ScheduleType::Interval)
            .unwrap()
            .unwrap();
        assert!(next > Utc::now());
    }

    #[test]
    fn dst_anomalies_are_reported_for_an_ambiguous_time() {
        // 01:30 America/New_York occurs twice on 2026-11-01, at 05:30Z (EDT)
        // and 06:30Z (EST). Forge fires once, on the first, and the preview
        // flags it so an operator is not surprised (spec 7.7).
        let occurrences = vec![DateTime::parse_from_rfc3339("2026-11-01T05:30:00Z")
            .unwrap()
            .with_timezone(&Utc)];

        let anomalies = dst_anomalies("30 1 * * *", "America/New_York", &occurrences);
        assert_eq!(anomalies.len(), 1, "the ambiguous hour must be flagged");
        assert_eq!(anomalies[0]["kind"], "AMBIGUOUS_LOCAL_TIME");
    }

    /// A nonexistent local time cannot appear in the occurrence list at all:
    /// the cron engine skips the hour the clock springs over. Asserting that
    /// keeps the behaviour pinned rather than accidental.
    #[test]
    fn a_nonexistent_local_time_yields_no_occurrence() {
        // 02:30 America/New_York does not exist on 2026-03-08.
        let cron = forge_scheduler::CronSchedule::parse("30 2 * * *", "America/New_York").unwrap();
        let from = DateTime::parse_from_rfc3339("2026-03-08T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2026-03-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let occurrences = cron.occurrences_between(from, to, 20);
        let on_the_day: Vec<_> = occurrences
            .iter()
            .filter(|o| o.with_timezone(&cron.timezone()).day() == 8)
            .collect();

        assert!(
            on_the_day.is_empty(),
            "the skipped hour must produce no execution: {occurrences:?}"
        );
    }

    #[test]
    fn ordinary_occurrences_report_no_anomaly() {
        let occurrences = vec![DateTime::parse_from_rfc3339("2026-06-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)];
        assert!(dst_anomalies("0 12 * * *", "UTC", &occurrences).is_empty());
    }
}