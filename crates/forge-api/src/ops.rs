//! API key rotation, job import/export, and the emergency control surface
//! (UI.md sections 41, 72, 76).
//!
//! Rotation issues a new secret and revokes the old one in a single step, so a
//! key is never simultaneously valid and invalid.

use axum::extract::{Path, State};
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
// API key rotation and expiry (UI.md section 76)
// ---------------------------------------------------------------------------

/// `POST /api-keys/{id}/rotate` — issue a replacement secret.
///
/// The old key stops working immediately. Rotation returning the new secret
/// once, like creation, keeps the "never display full credentials" rule.
pub async fn rotate_api_key(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(key_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT name, prefix FROM api_keys WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(tenant(&auth))
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((name, prefix)) = existing else {
        return Err(ApiError::not_found("active API key"));
    };

    let secret = generate_secret();
    let new_prefix: String = secret.chars().take(8).collect();

    sqlx::query(
        "UPDATE api_keys
         SET key_hash = $3, prefix = $4, last_used_at = NULL
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(key_id)
    .bind(tenant(&auth))
    .bind(hash(&secret))
    .bind(&new_prefix)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "id": key_id,
            "name": name,
            "previous_prefix": prefix,
            "prefix": new_prefix,
            "key": secret,
            "detail": "The previous secret no longer works. Store this one now.",
        }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct SetKeyExpiry {
    /// Days until the key stops working; null clears the expiry.
    pub expires_in_days: Option<i32>,
}

/// `PUT /api-keys/{id}/expiry` — set or clear a key's expiry.
pub async fn set_api_key_expiry(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(key_id): Path<Uuid>,
    Json(body): Json<SetKeyExpiry>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("settings:write")?;

    if let Some(days) = body.expires_in_days {
        if days <= 0 {
            return Err(ApiError::validation("expires_in_days must be positive")
                .with_detail("expires_in_days", "must be greater than zero"));
        }
    }

    let row: (Value,) = sqlx::query_as(
        "UPDATE api_keys
         SET expires_at = CASE WHEN $3::int IS NULL
                               THEN NULL
                               ELSE NOW() + make_interval(days => $3::int) END
         WHERE id = $1 AND tenant_id = $2
         RETURNING json_build_object(
             'id', id, 'name', name, 'prefix', prefix, 'expires_at', expires_at
         )",
    )
    .bind(key_id)
    .bind(tenant(&auth))
    .bind(body.expires_in_days)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("API key"))?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

/// A fresh API key, using the same generator as creation so a rotated key is
/// indistinguishable from a created one.
fn generate_secret() -> String {
    forge_auth::generate_api_key().raw
}

fn hash(secret: &str) -> String {
    forge_auth::hash_api_key(secret)
}

// ---------------------------------------------------------------------------
// Import / export (UI.md section 41)
// ---------------------------------------------------------------------------

/// `GET /jobs/export` — every job as JSON, for backup or Git/IaC use.
///
/// Export is a plain read, so it needs no extra permission beyond reading jobs.
pub async fn export_jobs(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object(
             'id', id, 'key', key, 'name', name, 'description', description,
             'status', status, 'priority', priority,
             'default_queue_id', default_queue_id, 'labels', labels
         )
         FROM jobs WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let schedules: Vec<Value> = sqlx::query_scalar(
        "SELECT json_build_object(
             'target_type', target_type, 'target_key', target_id,
             'cron_expression', cron_expression, 'timezone', timezone,
             'misfire_policy', misfire_policy, 'catch_up_policy', catch_up_policy
         )
         FROM schedules WHERE tenant_id = $1",
    )
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({ "jobs": rows, "schedules": schedules, "version": 1 }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct ImportJobs {
    pub jobs: Vec<ImportedJob>,
}

#[derive(Debug, Deserialize)]
pub struct ImportedJob {
    pub name: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub cron_expression: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
}

/// `POST /jobs/import` — create jobs from an exported document.
///
/// Reports per-job outcomes for the same reason bulk does: an import of twenty
/// jobs where two are invalid should say which two, not fail wholesale.
pub async fn import_jobs(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<ImportJobs>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("jobs:write")?;

    if body.jobs.is_empty() {
        return Err(
            ApiError::validation("jobs must not be empty").with_detail("jobs", "nothing to import")
        );
    }

    let mut imported = 0usize;
    let mut results: Vec<Value> = Vec::new();

    for job in &body.jobs {
        if job.name.trim().is_empty() {
            results.push(json!({ "name": job.name, "ok": false, "error": "name is empty" }));
            continue;
        }

        let priority = match job.priority.as_deref() {
            None => "NORMAL".to_string(),
            Some(raw) => {
                let normalised = raw.to_uppercase();
                if !["CRITICAL", "HIGH", "NORMAL", "LOW", "BACKGROUND"]
                    .contains(&normalised.as_str())
                {
                    results.push(json!({
                        "name": job.name, "ok": false,
                        "error": format!("`{raw}` is not a valid priority")
                    }));
                    continue;
                }
                normalised
            }
        };

        let id = Uuid::new_v4();
        let inserted = sqlx::query(
            "INSERT INTO jobs (id, tenant_id, key, name, description, status, priority, created_at)
             VALUES ($1, $2, $3, $4, $5, 'DRAFT', $6, NOW())
             ON CONFLICT (tenant_id, key) WHERE key IS NOT NULL DO NOTHING",
        )
        .bind(id)
        .bind(tenant(&auth))
        .bind(job.key.as_deref())
        .bind(job.name.trim())
        .bind(job.description.as_deref())
        .bind(&priority)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

        if inserted.rows_affected() == 0 {
            results.push(json!({
                "name": job.name, "ok": false,
                "error": "a job with that key already exists"
            }));
            continue;
        }

        // A schedule is optional; only attach one when the document supplied it.
        if let Some(expression) = job.cron_expression.as_deref() {
            // Only attach a schedule when the job has none, so re-importing a
            // document does not stack duplicate schedules.
            let existing: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM schedules WHERE job_id = $1 AND tenant_id = $2")
                    .bind(id)
                    .bind(tenant(&auth))
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(ApiError::from)?;

            if existing.is_none() {
                sqlx::query(
                    "INSERT INTO schedules
                         (id, tenant_id, target_type, target_id, schedule_type,
                          cron_expression, timezone, enabled, created_at)
                     VALUES ($1, $2, 'JOB', $3, 'CRON', $4, $5, TRUE, NOW())",
                )
                .bind(Uuid::new_v4())
                .bind(tenant(&auth))
                .bind(id)
                .bind(expression)
                .bind(job.timezone.as_deref().unwrap_or("UTC"))
                .execute(&state.pool)
                .await
                .map_err(ApiError::from)?;
            }
        }

        imported += 1;
        results.push(json!({ "name": job.name, "id": id, "ok": true }));
    }

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "imported": imported,
                "failed": body.jobs.len() - imported,
                "results": results,
            }),
            auth.request_id,
        )),
    ))
}

// ---------------------------------------------------------------------------
// Emergency controls (UI.md section 72)
// ---------------------------------------------------------------------------

/// `GET /emergency` — one place to see and act on every stop control.
///
/// The spec wants the emergency controls assembled rather than scattered. This
/// reports what is currently engaged so an operator can tell "I paused this"
/// from "someone paused this".
pub async fn emergency_state(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("jobs:read")?;

    let maintenance: Option<(Value,)> = sqlx::query_as(
        "SELECT json_build_object('reason', reason, 'started_at', started_at)
         FROM maintenance_windows WHERE tenant_id = $1 AND ended_at IS NULL LIMIT 1",
    )
    .bind(tenant(&auth))
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let queues: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'name', name, 'paused', paused,
             'depth', (SELECT COUNT(*) FROM executions e
                        WHERE e.queue_id = q.id AND e.status = 'QUEUED')
         )
         FROM queues q WHERE q.tenant_id = $1 ORDER BY q.name",
    )
    .bind(tenant(&auth))
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let running: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM executions
         WHERE tenant_id = $1 AND status IN ('DISPATCHED','RUNNING')",
    )
    .bind(tenant(&auth))
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let paused_schedules: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM schedules WHERE tenant_id = $1 AND enabled = FALSE",
    )
    .bind(tenant(&auth))
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "maintenance": maintenance.map(|(v,)| v),
            "queues": queues.into_iter().map(|(v,)| v).collect::<Vec<_>>(),
            "running_executions": running,
            "paused_schedules": paused_schedules,
        }),
        auth.request_id,
    )))
}

/// `POST /emergency/cancel-running` — cancel everything currently running.
///
/// Refuses without a reason: cancelling live work is destructive, and the spec
/// requires destructive actions be deliberate.
pub async fn cancel_all_running(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CancelAll>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("executions:write")?;

    if body.reason.trim().is_empty() {
        return Err(ApiError::validation("reason must not be empty")
            .with_detail("reason", "must not be empty"));
    }

    let cancelled = sqlx::query(
        "UPDATE executions
         SET status = 'CANCELLED',
             error_class = 'CANCELLATION',
             error_message = $2,
             ended_at = NOW(),
             updated_at = NOW()
         WHERE tenant_id = $1 AND status IN ('DISPATCHED','RUNNING')",
    )
    .bind(tenant(&auth))
    .bind(body.reason.trim())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    Ok(Json(ApiResponse::new(
        json!({ "cancelled": cancelled, "reason": body.reason.trim() }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CancelAll {
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_long_and_unique() {
        let a = generate_secret();
        let b = generate_secret();
        // forge-auth's generator emits a `forge_` prefix over 32 base64url
        // bytes: 6 + 43 characters.
        assert!(a.starts_with("forge_"), "keys carry a recognisable prefix");
        assert_eq!(a.len(), 49);
        assert_ne!(a, b, "each rotation must produce a distinct secret");
    }

    #[test]
    fn a_secret_is_stored_hashed() {
        let secret = generate_secret();
        let digest = hash(&secret);
        assert_ne!(digest, secret);
        // Deterministic so a presented key can be verified.
        assert_eq!(digest, hash(&secret));
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn the_prefix_is_the_first_eight_characters() {
        let secret = generate_secret();
        let prefix: String = secret.chars().take(8).collect();
        assert!(secret.starts_with(&prefix));
    }
}
