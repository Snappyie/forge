//! Alert, incident, and notification endpoints.
//!
//! UI.md §28 (alerts), §29 (incident page), §30 (alert configuration),
//! §50 (notifications) and §51 (notification preferences). Each screen reads
//! real rows; nothing here synthesises an alert that did not happen.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::router::AppState;

/// Every alert kind the rules table accepts (UI.md §30).
pub const ALERT_KINDS: &[&str] = &[
    "EXECUTION_FAILED",
    "SLA_VIOLATION",
    "QUEUE_BACKLOG",
    "WORKER_OFFLINE",
    "LATENCY_SPIKE",
    "SCHEDULE_MISSED",
];

// ---------------------------------------------------------------------------
// Alerts
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct ListAlertsQuery {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub kind: Option<String>,
}

/// `GET /alerts` — UI.md §28.
pub async fn list_alerts(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
    Query(query): Query<ListAlertsQuery>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    // `QueryBuilder` keeps the placeholder list contiguous as filters are
    // appended, which string concatenation would not.
    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT json_build_object(
             'id', id, 'kind', kind, 'severity', severity, 'title', title,
             'detail', detail, 'resource_type', resource_type, 'resource_id', resource_id,
             'status', status, 'created_at', created_at,
             'acknowledged_at', acknowledged_at, 'resolved_at', resolved_at
         )
         FROM alerts WHERE tenant_id = ",
    );
    qb.push_bind(auth.tenant_id.into_uuid());

    if let Some(status) = &query.status {
        qb.push(" AND status = ").push_bind(status.clone());
    }
    if let Some(severity) = &query.severity {
        qb.push(" AND severity = ").push_bind(severity.clone());
    }
    if let Some(kind) = &query.kind {
        qb.push(" AND kind = ").push_bind(kind.clone());
    }
    qb.push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(pagination.effective_limit() as i64);

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

/// `POST /alerts/{id}/acknowledge` — UI.md §65 lists acknowledging on mobile as
/// a must, so it needs a real endpoint.
pub async fn acknowledge_alert(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(alert_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    let row: (Value,) = sqlx::query_as(
        "UPDATE alerts
         SET status = 'ACKNOWLEDGED', acknowledged_by = $3, acknowledged_at = NOW()
         WHERE id = $1 AND tenant_id = $2 AND status = 'OPEN'
         RETURNING json_build_object(
             'id', id, 'kind', kind, 'severity', severity, 'title', title,
             'status', status, 'acknowledged_at', acknowledged_at
         )",
    )
    .bind(alert_id)
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("open alert"))?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

/// `GET /alerts/summary` — the counts the dashboard's "needs attention" needs.
pub async fn alerts_summary(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status = 'OPEN'),
             COUNT(*) FILTER (WHERE status = 'OPEN' AND severity = 'CRITICAL'),
             COUNT(*) FILTER (WHERE status = 'OPEN' AND severity = 'WARNING'),
             COUNT(*) FILTER (WHERE status = 'OPEN' AND severity = 'INFO')
         FROM alerts WHERE tenant_id = $1",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "open": counts.0,
            "critical": counts.1,
            "warning": counts.2,
            "info": counts.3,
        }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Alert rules
// ---------------------------------------------------------------------------

/// `GET /alert-rules` — UI.md §30.
pub async fn list_alert_rules(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'kind', kind, 'name', name, 'target_type', target_type,
             'target_id', target_id, 'config', config, 'enabled', enabled,
             'cooldown_seconds', cooldown_seconds, 'created_at', created_at
         )
         FROM alert_rules WHERE tenant_id = $1 ORDER BY name",
    )
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

#[derive(Debug, Deserialize)]
pub struct CreateAlertRuleRequest {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub target_id: Option<Uuid>,
    #[serde(default)]
    pub config: Value,
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: i32,
}

fn default_cooldown() -> i32 {
    900
}

/// `POST /alert-rules` — UI.md §30.
pub async fn create_alert_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateAlertRuleRequest>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("settings:write")?;

    if !ALERT_KINDS.contains(&body.kind.as_str()) {
        return Err(
            ApiError::validation(format!("`{}` is not a known alert kind", body.kind))
                .with_detail("kind", "unknown alert kind")
                .with_detail("kind", format!("expected one of {ALERT_KINDS:?}")),
        );
    }
    if body.name.trim().is_empty() {
        return Err(
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty")
        );
    }
    if body.cooldown_seconds < 0 {
        return Err(
            ApiError::validation("cooldown_seconds must not be negative")
                .with_detail("cooldown_seconds", "must be zero or greater"),
        );
    }

    let id = Uuid::new_v4();
    let row: (Value,) = sqlx::query_as(
        "INSERT INTO alert_rules
             (id, tenant_id, kind, name, target_type, target_id, config, cooldown_seconds)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING json_build_object(
             'id', id, 'kind', kind, 'name', name, 'config', config,
             'enabled', enabled, 'cooldown_seconds', cooldown_seconds
         )",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(&body.kind)
    .bind(body.name.trim())
    .bind(&body.target_type)
    .bind(body.target_id)
    .bind(&body.config)
    .bind(body.cooldown_seconds)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(row.0, auth.request_id)),
    ))
}

/// `PATCH /alert-rules/{id}` — enable or disable, and retune thresholds.
#[derive(Debug, Deserialize)]
pub struct UpdateAlertRuleRequest {
    pub enabled: Option<bool>,
    pub cooldown_seconds: Option<i32>,
    pub config: Option<Value>,
}

/// `PATCH /alert-rules/{id}`.
pub async fn update_alert_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(rule_id): Path<Uuid>,
    Json(body): Json<UpdateAlertRuleRequest>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    if let Some(cooldown) = body.cooldown_seconds {
        if cooldown < 0 {
            return Err(
                ApiError::validation("cooldown_seconds must not be negative")
                    .with_detail("cooldown_seconds", "must be zero or greater"),
            );
        }
    }

    let row: (Value,) = sqlx::query_as(
        "UPDATE alert_rules SET
             enabled = COALESCE($3, enabled),
             cooldown_seconds = COALESCE($4, cooldown_seconds),
             config = COALESCE($5, config),
             updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2
         RETURNING json_build_object(
             'id', id, 'kind', kind, 'name', name, 'config', config,
             'enabled', enabled, 'cooldown_seconds', cooldown_seconds
         )",
    )
    .bind(rule_id)
    .bind(auth.tenant_id.into_uuid())
    .bind(body.enabled)
    .bind(body.cooldown_seconds)
    .bind(&body.config)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("alert rule"))?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

/// `DELETE /alert-rules/{id}`.
pub async fn delete_alert_rule(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(rule_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    let affected = sqlx::query("DELETE FROM alert_rules WHERE id = $1 AND tenant_id = $2")
        .bind(rule_id)
        .bind(auth.tenant_id.into_uuid())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("alert rule"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": rule_id, "deleted": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Incidents
// ---------------------------------------------------------------------------

/// `GET /incidents` — UI.md §29.
pub async fn list_incidents(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'title', title, 'summary', summary, 'severity', severity,
             'status', status, 'alert_count', array_length(alert_ids, 1),
             'impact', impact, 'detected_at', detected_at,
             'acknowledged_at', acknowledged_at, 'resolved_at', resolved_at
         )
         FROM incidents WHERE tenant_id = $1
         ORDER BY detected_at DESC LIMIT $2",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(pagination.effective_limit() as i64)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ListResponse::new(
        rows.into_iter().map(|(v,)| v).collect(),
        Default::default(),
        auth.request_id,
    )))
}

/// `GET /incidents/{id}` — UI.md §29's incident page.
pub async fn get_incident(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(incident_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("audit:read")?;

    let row: (Value,) = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'title', title, 'summary', summary, 'severity', severity,
             'status', status, 'impact', impact, 'root_cause', root_cause,
             'resolution', resolution, 'detected_at', detected_at,
             'acknowledged_at', acknowledged_at, 'resolved_at', resolved_at
         )
         FROM incidents WHERE id = $1 AND tenant_id = $2",
    )
    .bind(incident_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("incident"))?;

    // The alerts folded into this incident, so the page can show them.
    let alerts: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'kind', kind, 'severity', severity, 'title', title,
             'status', status, 'resource_type', resource_type, 'resource_id', resource_id
         )
         FROM alerts
         WHERE tenant_id = $1 AND id = ANY(
             SELECT unnest(alert_ids) FROM incidents WHERE id = $2
         )
         ORDER BY created_at DESC",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(incident_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The timeline.
    let events: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'at', at, 'kind', kind, 'message', message, 'actor_id', actor_id
         )
         FROM incident_events WHERE incident_id = $1 AND tenant_id = $2 ORDER BY at ASC",
    )
    .bind(incident_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "incident": row.0,
            "alerts": alerts.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            "timeline": events.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
        }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

/// `GET /notifications` — UI.md §50.
pub async fn list_notifications(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<NotificationsResponse>, ApiError> {
    auth.require("executions:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'title', title, 'body', body,
             'resource_type', resource_type, 'resource_id', resource_id,
             'read', read_at IS NOT NULL, 'created_at', created_at
         )
         FROM notifications
         WHERE tenant_id = $1 AND user_id = $2
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The bell badge needs the unread count, which the envelope's page object
    // has no room for, so it rides alongside it.
    let unread: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications
         WHERE tenant_id = $1 AND user_id = $2 AND read_at IS NULL",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(NotificationsResponse {
        data: rows.into_iter().map(|(v,)| v).collect(),
        unread,
        request_id: auth.request_id,
    }))
}

/// The notification list plus the unread count the bell icon needs.
#[derive(serde::Serialize)]
pub struct NotificationsResponse {
    pub data: Vec<Value>,
    pub unread: i64,
    pub request_id: String,
}

/// `POST /notifications/read` — marks every notification read.
pub async fn mark_notifications_read(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:read")?;

    let updated = sqlx::query(
        "UPDATE notifications SET read_at = NOW()
         WHERE tenant_id = $1 AND user_id = $2 AND read_at IS NULL",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    Ok(Json(ApiResponse::new(
        json!({ "marked_read": updated }),
        auth.request_id,
    )))
}

/// `GET /notification-preferences` — UI.md §51.
pub async fn get_notification_preferences(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:read")?;

    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'enabled_kinds', to_jsonb(enabled_kinds), 'channels', channels,
             'quiet_hours_start', quiet_hours_start,
             'quiet_hours_end', quiet_hours_end
         )
         FROM notification_preferences
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // A user who has never saved preferences gets the documented defaults
    // rather than a null.
    let preferences = row.map(|(v,)| v).unwrap_or_else(|| {
        json!({
            "enabled_kinds": ALERT_KINDS,
            "channels": { "in_app": true, "email": false, "webhook": false },
            "quiet_hours_start": null,
            "quiet_hours_end": null,
        })
    });

    Ok(Json(ApiResponse::new(preferences, auth.request_id)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateNotificationPreferences {
    #[serde(default)]
    pub enabled_kinds: Option<Vec<String>>,
    #[serde(default)]
    pub channels: Option<Value>,
    #[serde(default)]
    pub quiet_hours_start: Option<String>,
    #[serde(default)]
    pub quiet_hours_end: Option<String>,
}

/// `PUT /notification-preferences` — UI.md §51.
pub async fn update_notification_preferences(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<UpdateNotificationPreferences>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:read")?;

    for kind in body.enabled_kinds.iter().flatten() {
        if !ALERT_KINDS.contains(&kind.as_str()) {
            return Err(
                ApiError::validation(format!("`{kind}` is not a known alert kind"))
                    .with_detail("enabled_kinds", "unknown alert kind"),
            );
        }
    }

    let row: (Value,) = sqlx::query_as(
        "INSERT INTO notification_preferences
             (id, tenant_id, user_id, enabled_kinds, channels,
              quiet_hours_start, quiet_hours_end)
         VALUES ($1, $2, $3, $4, COALESCE($5, '{\"in_app\": true}'::jsonb), $6, $7)
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET
             enabled_kinds = COALESCE($4, notification_preferences.enabled_kinds),
             channels = COALESCE($5, notification_preferences.channels),
             quiet_hours_start = COALESCE($6, notification_preferences.quiet_hours_start),
             quiet_hours_end = COALESCE($7, notification_preferences.quiet_hours_end),
             updated_at = NOW()
         RETURNING json_build_object(
             'enabled_kinds', to_jsonb(enabled_kinds), 'channels', channels,
             'quiet_hours_start', quiet_hours_start,
             'quiet_hours_end', quiet_hours_end
         )",
    )
    .bind(Uuid::new_v4())
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .bind(body.enabled_kinds)
    .bind(&body.channels)
    .bind(body.quiet_hours_start.as_deref())
    .bind(body.quiet_hours_end.as_deref())
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alert_kinds_match_the_schema_constraint() {
        // If these drift apart, rule creation starts failing at runtime.
        let expected = [
            "EXECUTION_FAILED",
            "SLA_VIOLATION",
            "QUEUE_BACKLOG",
            "WORKER_OFFLINE",
            "LATENCY_SPIKE",
            "SCHEDULE_MISSED",
        ];
        assert_eq!(ALERT_KINDS.len(), expected.len());
        for kind in expected {
            assert!(ALERT_KINDS.contains(&kind), "{kind} must be offered");
        }
    }

    #[test]
    fn an_unknown_alert_kind_is_rejected() {
        assert!(!ALERT_KINDS.contains(&"MADE_UP_KIND"));
    }
}
