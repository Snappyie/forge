//! Schedule endpoints (spec 05 endpoints 10–16).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_scheduler::{CronSchedule, MisfirePolicy, RecurrenceSpec};
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
    /// The exact version a `PINNED` schedule runs (spec 02.4). Required when
    /// the policy is `PINNED`; without it the policy could not be honoured.
    #[serde(default, alias = "pinned_version_id")]
    pub target_version_id: Option<Uuid>,
    #[serde(default)]
    pub schedule_type: String,
    /// Five-field cron. Required for a CRON schedule, ignored otherwise.
    pub expression: Option<String>,
    /// IANA timezone. Required for recurring schedules (spec 09.5).
    pub timezone: String,
    #[serde(default)]
    pub misfire_policy: String,
    #[serde(default)]
    pub catch_up_limit: Option<u32>,
    /// For a one-time schedule: the single UTC instant to fire at.
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
            return Err(
                ApiError::validation(format!("`{other}` is not a valid target type"))
                    .with_detail("target_type", "expected JOB or WORKFLOW"),
            )
        }
    };
    let version_policy = version_policy_name(&body)?;

    // Validate the recurrence configuration before storing it, so a schedule
    // that cannot be evaluated is rejected at creation rather than at fire
    // time (spec 09.5 requires an explicit, valid timezone).
    let spec = recurrence_spec(&body, schedule_type)?;
    let now = Utc::now();
    let next_run_at = compute_next_run(&spec, now)?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO schedules
            (id, tenant_id, job_id, target_id, target_type, target_version_policy,
             pinned_version_id, schedule_type, cron_expression, interval_seconds, one_time_at,
             timezone, misfire_policy, catch_up_policy, next_run_at, enabled)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, TRUE)",
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
    .bind(body.target_version_id)
    .bind(schedule_type_name(schedule_type))
    .bind(spec.expression.clone())
    .bind(spec.interval_seconds)
    .bind(spec.one_time_at)
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
                "target_version_policy": version_policy,
                "target_version_id": body.target_version_id,
                "schedule_type": schedule_type_name(schedule_type),
                "expression": spec.expression.clone(),
                "description": spec.describe(),
                "interval_seconds": spec.interval_seconds,
                "one_time_at": spec.one_time_at,
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
        other => Err(
            ApiError::validation(format!("`{other}` is not a schedule type"))
                .with_detail("schedule_type", "expected CRON, ONE_TIME or INTERVAL"),
        ),
    }
}

fn schedule_type_name(kind: forge_domain::ScheduleType) -> &'static str {
    match kind {
        forge_domain::ScheduleType::Cron => "CRON",
        forge_domain::ScheduleType::OneTime => "ONE_TIME",
        forge_domain::ScheduleType::Interval => "INTERVAL",
    }
}

/// Parses the storage vocabulary written by [`schedule_type_name`].
fn parse_stored_schedule_type(raw: &str) -> forge_domain::ScheduleType {
    match raw {
        "ONE_TIME" => forge_domain::ScheduleType::OneTime,
        "INTERVAL" => forge_domain::ScheduleType::Interval,
        _ => forge_domain::ScheduleType::Cron,
    }
}

fn misfire_name(policy: MisfirePolicy) -> &'static str {
    match policy {
        MisfirePolicy::Skip => "SKIP",
        MisfirePolicy::FireOnce => "FIRE_ONCE",
        MisfirePolicy::CatchUp => "CATCH_UP",
    }
}

/// Validates `target_version_policy` and returns the storage vocabulary value.
///
/// Spec 02.4: `PINNED` means "run this exact version", so the version is part
/// of the schedule's configuration. Accepting the policy without it would store
/// a claim the engine cannot honour.
fn version_policy_name(body: &CreateScheduleRequest) -> Result<&'static str, ApiError> {
    let policy = match body
        .target_version_policy
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "" | "LATEST_PUBLISHED" => "LATEST_PUBLISHED",
        "PINNED" => "PINNED",
        other => {
            return Err(
                ApiError::validation(format!("`{other}` is not a valid version policy"))
                    .with_detail(
                        "target_version_policy",
                        "expected PINNED or LATEST_PUBLISHED",
                    ),
            )
        }
    };
    if policy == "PINNED" && body.target_version_id.is_none() {
        return Err(
            ApiError::validation("a PINNED schedule requires target_version_id").with_detail(
                "target_version_id",
                "required when target_version_policy is PINNED",
            ),
        );
    }
    Ok(policy)
}

/// Validates a create request's recurrence fields and returns the
/// configuration the shared engine will evaluate.
///
/// Spec 01.5 requires one-time, fixed-interval and cron schedules, so each kind
/// has its own required input; a missing or impossible one is a validation
/// error here rather than a row the scheduler has to disable later.
fn recurrence_spec(
    body: &CreateScheduleRequest,
    kind: forge_domain::ScheduleType,
) -> Result<RecurrenceSpec, ApiError> {
    match kind {
        forge_domain::ScheduleType::Cron => {
            let expression = body.expression.as_deref().ok_or_else(|| {
                ApiError::validation("a cron schedule requires an expression")
                    .with_detail("expression", "required for CRON")
            })?;
            let spec = RecurrenceSpec::cron(expression, &body.timezone);
            // Build the calculator once so the expression and the timezone are
            // both validated against the same engine the scheduler uses.
            spec.calculator(Utc::now()).map_err(|e| {
                ApiError::validation(e.to_string())
                    .with_detail("expression", "invalid cron expression or timezone")
            })?;
            Ok(spec)
        }
        forge_domain::ScheduleType::Interval => {
            let seconds = body.interval_seconds.ok_or_else(|| {
                ApiError::validation("an interval schedule requires interval_seconds")
                    .with_detail("interval_seconds", "required for INTERVAL")
            })?;
            if seconds <= 0 {
                return Err(ApiError::validation("interval_seconds must be positive")
                    .with_detail("interval_seconds", "must be greater than zero"));
            }
            // Spec 09.5: a recurring schedule names an IANA timezone, even
            // though interval arithmetic itself is in UTC.
            forge_scheduler::validate_timezone(&body.timezone).map_err(|e| {
                ApiError::validation(e.to_string()).with_detail("timezone", "unknown IANA timezone")
            })?;
            Ok(RecurrenceSpec::interval(seconds, &body.timezone))
        }
        forge_domain::ScheduleType::OneTime => {
            let at = body.one_time_at.ok_or_else(|| {
                ApiError::validation("a one-time schedule requires one_time_at")
                    .with_detail("one_time_at", "required for ONE_TIME")
            })?;
            Ok(RecurrenceSpec::one_time(at, &body.timezone))
        }
    }
}

/// Computes the first occurrence, using the same engine the scheduler will use
/// so the preview cannot disagree with reality (spec 9.13).
///
/// A one-time instant is stored even when it is already in the past: the
/// misfire policy decides how a late one-shot is handled, exactly as it does
/// for a cron occurrence that was missed while the system was down.
fn compute_next_run(
    spec: &RecurrenceSpec,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    if spec.schedule_type == forge_domain::ScheduleType::OneTime {
        return Ok(spec.one_time_at);
    }
    let calculator = spec
        .calculator(now)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    Ok(calculator.next_after(now))
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
             'target_version_policy', target_version_policy,
             'target_version_id', pinned_version_id,
             'schedule_type', schedule_type,
             'expression', cron_expression, 'timezone', timezone,
             'interval_seconds', interval_seconds, 'one_time_at', one_time_at,
             'misfire_policy', misfire_policy, 'catch_up_limit',
                 catch_up_policy->>'max_occurrences',
             'enabled', enabled, 'disabled_reason', disabled_reason,
             'next_run_at', next_run_at,
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
             'target_version_policy', target_version_policy,
             'target_version_id', pinned_version_id,
             'schedule_type', schedule_type,
             'expression', cron_expression, 'timezone', timezone,
             'interval_seconds', interval_seconds, 'one_time_at', one_time_at,
             'misfire_policy', misfire_policy, 'enabled', enabled,
             'disabled_reason', disabled_reason,
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

/// The stored configuration a preview evaluates, in one row.
#[derive(Debug, sqlx::FromRow)]
struct PreviewRow {
    schedule_type: String,
    cron_expression: Option<String>,
    timezone: String,
    interval_seconds: Option<i64>,
    one_time_at: Option<DateTime<Utc>>,
    next_run_at: Option<DateTime<Utc>>,
    disabled_reason: Option<String>,
}

/// `POST /schedules/{id}/preview` (spec 05 endpoint 16).
///
/// Spec 9.13 and 7.7: the preview calls the same engine the scheduler uses, so
/// it can never disagree with what actually fires — for all three schedule
/// kinds. DST anomalies are flagged so the UI can show them (spec 7.7); they
/// are meaningful for cron only, since interval and one-time arithmetic is in
/// UTC.
pub async fn preview(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(schedule_id): Path<Uuid>,
    Json(body): Json<PreviewRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:read")?;

    let row: PreviewRow = sqlx::query_as(
        "SELECT schedule_type, cron_expression, timezone, interval_seconds, one_time_at,
                next_run_at, disabled_reason
         FROM schedules WHERE id = $1 AND tenant_id = $2",
    )
    .bind(schedule_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("schedule"))?;

    let PreviewRow {
        schedule_type,
        cron_expression: expression,
        timezone,
        interval_seconds,
        one_time_at,
        next_run_at,
        disabled_reason: reason,
    } = row;
    let kind = parse_stored_schedule_type(&schedule_type);
    let spec = RecurrenceSpec {
        schedule_type: kind,
        expression,
        timezone,
        interval_seconds,
        one_time_at,
        blackout_dates: Vec::new(),
        time_window: None,
    };

    // An interval series is anchored at the schedule's stored `next_run_at`,
    // which is exactly what the loop uses; a one-time schedule ignores it.
    let now = Utc::now();
    let anchor = next_run_at.unwrap_or(now);
    let count = body.count.clamp(1, 50);

    let occurrences =
        forge_scheduler::preview_recurrence(&spec, anchor, now, count).map_err(|e| {
            ApiError::validation(e.to_string())
                .with_detail("schedule_type", "the stored schedule cannot be evaluated")
        })?;
    let explanation =
        forge_scheduler::explain_recurrence(&spec, anchor, now, count).map_err(|e| {
            ApiError::validation(e.to_string())
                .with_detail("schedule_type", "the stored schedule cannot be evaluated")
        })?;

    let anomalies = match (&spec.schedule_type, spec.expression.as_deref()) {
        (forge_domain::ScheduleType::Cron, Some(expression)) => {
            dst_anomalies(expression, &spec.timezone, &occurrences)
        }
        _ => Vec::new(),
    };

    Ok(Json(ApiResponse::new(
        json!({
            "schedule_type": schedule_type,
            "expression": spec.expression,
            "description": spec.describe(),
            "timezone": spec.timezone,
            "next_run_at": next_run_at,
            "occurrences": occurrences,
            "anomalies": anomalies,
            "engine": explanation.expression,
            "disabled_reason": reason,
        }),
        auth.request_id,
    )))
}

/// A preview request for an expression that has not been saved yet.
#[derive(Debug, Deserialize)]
pub struct ExplainRequest {
    /// Defaults to CRON so an existing caller that sends only an expression
    /// keeps working.
    #[serde(default)]
    pub schedule_type: String,
    /// Five-field cron. Required for a CRON schedule.
    pub expression: Option<String>,
    /// Required for recurring schedules (spec 09.5): a schedule with no
    /// timezone is a validation error, never a silent fallback to the server's
    /// local time. That rule holds for a preview too, or the preview would
    /// quietly disagree with what the scheduler later does.
    pub timezone: String,
    /// For an INTERVAL preview.
    #[serde(default)]
    pub interval_seconds: Option<i64>,
    /// For a ONE_TIME preview.
    #[serde(default)]
    pub one_time_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub blackout_dates: Vec<String>,
    #[serde(default)]
    pub time_window: Option<forge_domain::TimeWindow>,
    #[serde(default = "default_preview_count")]
    pub count: usize,
}

/// `POST /schedules/explain` — preview an unsaved schedule.
///
/// `POST /schedules/{id}/preview` needs a saved schedule, which makes it useless
/// while authoring one: the builder's whole point is showing the next runs
/// *before* the schedule exists. This evaluates the same
/// `forge_scheduler::preview_recurrence` the scheduler itself uses (spec 09.13),
/// so a preview still cannot disagree with what actually fires, for any of the
/// three kinds.
///
/// It reads nothing and writes nothing, so it needs no schedule to exist and
/// cannot be used to probe another tenant's schedules.
pub async fn explain(
    Auth(auth): Auth,
    Json(body): Json<ExplainRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("schedules:read")?;

    let kind = parse_schedule_type(&body.schedule_type)?;
    let spec = RecurrenceSpec {
        schedule_type: kind,
        expression: body.expression.clone(),
        timezone: body.timezone.clone(),
        interval_seconds: body.interval_seconds,
        one_time_at: body.one_time_at,
        blackout_dates: body.blackout_dates.clone(),
        time_window: body.time_window,
    };

    if kind == forge_domain::ScheduleType::Cron && spec.timezone.trim().is_empty() {
        return Err(ApiError::validation(
            "a timezone is required; it is never inferred from the server",
        ));
    }
    if kind == forge_domain::ScheduleType::Interval {
        forge_scheduler::validate_timezone(&spec.timezone).map_err(|e| {
            ApiError::validation(e.to_string()).with_detail("timezone", "unknown IANA timezone")
        })?;
    }

    let count = body.count.clamp(1, 50);
    let after = Utc::now();
    // An unsaved schedule has no stored anchor. For an interval that means the
    // series starts at `after`, which is the same first run a freshly created
    // schedule would get; cron and one-time are absolute and ignore it.
    let anchor = after;

    let occurrences =
        forge_scheduler::preview_recurrence(&spec, anchor, after, count).map_err(|e| {
            ApiError::validation(e.to_string())
                .with_detail("schedule_type", "the schedule cannot be evaluated")
        })?;
    let explanation =
        forge_scheduler::explain_recurrence(&spec, anchor, after, count).map_err(|e| {
            ApiError::validation(e.to_string())
                .with_detail("schedule_type", "the schedule cannot be evaluated")
        })?;

    let anomalies = match (kind, spec.expression.as_deref()) {
        (forge_domain::ScheduleType::Cron, Some(expression)) => {
            dst_anomalies(expression, &spec.timezone, &occurrences)
        }
        _ => Vec::new(),
    };

    Ok(Json(ApiResponse::new(
        json!({
            "schedule_type": schedule_type_name(kind),
            "expression": spec.expression,
            "description": spec.describe(),
            "timezone": spec.timezone,
            "next_run_at": occurrences.first(),
            "occurrences": occurrences,
            "anomalies": anomalies,
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
        let naive = chrono::NaiveDateTime::new(local.date_naive(), local.time());
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
        assert_eq!(schedule_type_name(forge_domain::ScheduleType::Cron), "CRON");
        assert_eq!(
            schedule_type_name(forge_domain::ScheduleType::OneTime),
            "ONE_TIME"
        );
        assert_eq!(
            schedule_type_name(forge_domain::ScheduleType::Interval),
            "INTERVAL"
        );
    }

    fn body(kind: &str) -> CreateScheduleRequest {
        CreateScheduleRequest {
            target_type: "JOB".into(),
            target_id: Uuid::new_v4(),
            target_version_policy: "LATEST_PUBLISHED".into(),
            target_version_id: None,
            schedule_type: kind.into(),
            expression: None,
            timezone: "UTC".into(),
            misfire_policy: "FIRE_ONCE".into(),
            catch_up_limit: None,
            one_time_at: None,
            interval_seconds: None,
        }
    }

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn a_cron_next_run_is_computed_from_the_shared_engine() {
        let mut request = body("CRON");
        request.expression = Some("0 2 * * *".into());
        let spec = recurrence_spec(&request, forge_domain::ScheduleType::Cron).unwrap();
        let next = compute_next_run(&spec, Utc::now())
            .unwrap()
            .expect("a cron schedule always has a next occurrence");
        assert!(next > Utc::now());
    }

    #[test]
    fn a_one_time_schedule_uses_its_explicit_instant() {
        let instant = Utc::now() + chrono::Duration::days(1);
        let mut request = body("ONE_TIME");
        request.one_time_at = Some(instant);
        let spec = recurrence_spec(&request, forge_domain::ScheduleType::OneTime).unwrap();
        assert_eq!(compute_next_run(&spec, Utc::now()).unwrap(), Some(instant));
    }

    #[test]
    fn an_interval_schedule_uses_its_period() {
        let mut request = body("INTERVAL");
        request.interval_seconds = Some(3600);
        let spec = recurrence_spec(&request, forge_domain::ScheduleType::Interval).unwrap();
        let next = compute_next_run(&spec, Utc::now()).unwrap().unwrap();
        assert!(next > Utc::now());
    }

    /// Spec 01.5: a one-time schedule without an instant can never fire, so it
    /// is refused at creation rather than stored as a dead row.
    #[test]
    fn a_one_time_schedule_without_an_instant_is_rejected() {
        let request = body("ONE_TIME");
        assert!(recurrence_spec(&request, forge_domain::ScheduleType::OneTime).is_err());
    }

    #[test]
    fn an_interval_schedule_requires_a_positive_period() {
        let mut request = body("INTERVAL");
        assert!(
            recurrence_spec(&request, forge_domain::ScheduleType::Interval).is_err(),
            "a missing period is not a schedule"
        );

        for bad in [0_i64, -60] {
            request.interval_seconds = Some(bad);
            assert!(
                recurrence_spec(&request, forge_domain::ScheduleType::Interval).is_err(),
                "a {bad}s period must be rejected"
            );
        }

        request.interval_seconds = Some(60);
        assert!(recurrence_spec(&request, forge_domain::ScheduleType::Interval).is_ok());
    }

    /// Spec 09.5: every recurring schedule names a real IANA timezone.
    #[test]
    fn an_interval_schedule_rejects_an_unknown_timezone() {
        let mut request = body("INTERVAL");
        request.interval_seconds = Some(60);
        request.timezone = "Mars/Olympus_Mons".into();
        assert!(recurrence_spec(&request, forge_domain::ScheduleType::Interval).is_err());
    }

    /// Spec 02.4: PINNED without a version cannot be honoured.
    #[test]
    fn a_pinned_schedule_requires_a_version() {
        let mut request = body("CRON");
        request.expression = Some("0 2 * * *".into());
        request.target_version_policy = "PINNED".into();
        assert!(version_policy_name(&request).is_err());

        request.target_version_id = Some(Uuid::new_v4());
        assert_eq!(version_policy_name(&request).unwrap(), "PINNED");
    }

    #[test]
    fn latest_published_needs_no_version() {
        let request = body("CRON");
        assert_eq!(version_policy_name(&request).unwrap(), "LATEST_PUBLISHED");
        assert!(version_policy_name(&request).is_ok());
    }

    /// Spec 09.13: the interval preview uses the same engine as the loop, so
    /// its occurrences are the schedule's actual runs.
    #[test]
    fn an_interval_schedule_previews_successive_occurrences() {
        let spec = RecurrenceSpec::interval(600, "UTC");
        let anchor = at("2026-10-03T00:00:00Z");
        let occurrences = forge_scheduler::preview_recurrence(&spec, anchor, anchor, 3).unwrap();
        assert_eq!(
            occurrences,
            vec![
                at("2026-10-03T00:10:00Z"),
                at("2026-10-03T00:20:00Z"),
                at("2026-10-03T00:30:00Z"),
            ]
        );
    }

    #[test]
    fn a_one_time_schedule_previews_its_single_instant() {
        let instant = at("2026-10-04T09:00:00Z");
        let spec = RecurrenceSpec::one_time(instant, "UTC");
        let occurrences =
            forge_scheduler::preview_recurrence(&spec, instant, at("2026-10-03T00:00:00Z"), 5)
                .unwrap();
        assert_eq!(occurrences, vec![instant]);
    }

    #[test]
    fn an_unusable_interval_spec_cannot_be_previewed() {
        let spec = RecurrenceSpec::interval(0, "UTC");
        let now = Utc::now();
        assert!(forge_scheduler::preview_recurrence(&spec, now, now, 3).is_err());
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
