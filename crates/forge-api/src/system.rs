//! Health, audit, API key, and metrics endpoints (spec 05 endpoints 49–62).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_storage::{AuditFilter, AuditRepository, IdempotencyRepository};

use crate::envelope::{ApiError, ApiResponse, ListResponse, PaginationQuery};
use crate::extract::Auth;
use crate::router::AppState;

/// `GET /health/live` (spec 05 endpoint 60) — liveness.
///
/// Answers as long as the process is running; it deliberately touches no
/// dependency, so a database outage does not cause an orchestrator to kill a
/// process that is merely waiting.
pub async fn health_live() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::new(
        json!({ "status": "ok" }),
        Uuid::new_v4().to_string(),
    ))
}

/// `GET /health/ready` (spec 05 endpoint 61) — readiness.
///
/// Unlike liveness this checks the dependency the server cannot serve without.
pub async fn health_ready(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let database = sqlx::query("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();

    let body = json!({
        "status": if database { "ready" } else { "degraded" },
        "checks": { "database": database },
        "version": state.version,
    });

    if database {
        Ok(Json(ApiResponse::new(body, Uuid::new_v4().to_string())))
    } else {
        Err(ApiError::unavailable("the database is not reachable")
            .with_detail("database", "unreachable"))
    }
}

/// `GET /health` (spec 05 endpoint 62) — detail for an authorised operator.
pub async fn health_detail(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("audit:read")?;

    let migrations: Vec<(i64, bool,)> = sqlx::query_as(
        "SELECT version, success FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "version": state.version,
            "database": sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok(),
            "migrations": migrations
                .iter()
                .map(|(v, ok)| json!({ "version": v, "applied": ok }))
                .collect::<Vec<_>>(),
            "all_migrations_applied": migrations.iter().all(|(_, ok)| *ok),
        }),
        auth.request_id,
    )))
}

/// `GET /metrics` — Prometheus text exposition (spec 12).
pub async fn metrics(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<String, ApiError> {
    auth.require("audit:read")?;

    let snapshot = load_snapshot(&state.pool, auth.tenant_id).await?;
    Ok(snapshot.to_prometheus())
}

/// Assembles the console metric snapshot (spec 7.4).
pub async fn load_snapshot(
    pool: &sqlx::PgPool,
    tenant: forge_domain::TenantId,
) -> Result<forge_observability::MetricsSnapshot, ApiError> {
    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE status = 'QUEUED'),
             COUNT(*) FILTER (WHERE status IN ('DISPATCHED','RUNNING')),
             COUNT(*) FILTER (WHERE status = 'SUCCEEDED' AND created_at > NOW() - INTERVAL '24 hours'),
             COUNT(*) FILTER (WHERE status IN ('FAILED','TIMED_OUT') AND created_at > NOW() - INTERVAL '24 hours'),
             COUNT(*) FILTER (WHERE status = 'DEAD_LETTERED' AND created_at > NOW() - INTERVAL '24 hours')
         FROM executions WHERE tenant_id = $1",
    )
    .bind(tenant.into_uuid())
    .fetch_one(pool)
    .await
    .map_err(ApiError::from)?;

    let oldest: (Option<DateTime<Utc>>,) = sqlx::query_as(
        "SELECT MIN(created_at) FROM executions
         WHERE tenant_id = $1 AND status = 'QUEUED'",
    )
    .bind(tenant.into_uuid())
    .fetch_one(pool)
    .await
    .map_err(ApiError::from)?;

    let latency: (Option<f64>,) = sqlx::query_as(
        "SELECT AVG(EXTRACT(EPOCH FROM (ended_at - started_at)) * 1000)
         FROM executions
         WHERE tenant_id = $1 AND started_at IS NOT NULL AND ended_at IS NOT NULL
           AND created_at > NOW() - INTERVAL '24 hours'",
    )
    .bind(tenant.into_uuid())
    .fetch_one(pool)
    .await
    .map_err(ApiError::from)?;

    let workers: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE status = 'READY' AND draining = FALSE),
                COUNT(*) FILTER (WHERE status IN ('OFFLINE','REVOKED'))
         FROM workers WHERE tenant_id = $1",
    )
    .bind(tenant.into_uuid())
    .fetch_one(pool)
    .await
    .map_err(ApiError::from)?;

    let queues: Vec<(String, i64)> = sqlx::query_as(
        "SELECT q.name,
                COUNT(e.id) FILTER (WHERE e.status = 'QUEUED')
         FROM queues q
         LEFT JOIN executions e ON e.queue_id = q.id
         WHERE q.tenant_id = $1
         GROUP BY q.name ORDER BY q.name",
    )
    .bind(tenant.into_uuid())
    .fetch_all(pool)
    .await
    .map_err(ApiError::from)?;

    let total = counts.2 + counts.3;
    let success_rate = if total > 0 {
        counts.2 as f64 / total as f64
    } else {
        1.0
    };

// How long the oldest queued execution has been waiting (spec 7.4).
    let oldest_age = oldest
        .0
        .map(|at| (Utc::now() - at).num_seconds())
        .unwrap_or(0);

    Ok(forge_observability::MetricsSnapshot {
        queue_depth: counts.0,
        oldest_queued_age_secs: oldest_age,
        running: counts.1,
        succeeded_24h: counts.2,
        failed_24h: counts.3,
        dead_lettered_24h: counts.4,
        success_rate,
        mean_latency_ms: latency.0.unwrap_or(0.0),
        workers_ready: workers.0,
        workers_offline: workers.1,
        queues: queues.into_iter().collect(),
    })
}

/// `GET /audit-events` (spec 05 endpoint 52) — read-only.
#[derive(Debug, Default, Deserialize)]
pub struct AuditQuery {
    pub actor: Option<Uuid>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub result: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

pub async fn list_audit_events(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Query(pagination): Query<PaginationQuery>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<ListResponse<forge_storage::AuditEventRow>>, ApiError> {
    auth.require("audit:read")?;

    let page = AuditRepository::new(&state.pool)
        .list(
            auth.tenant_id,
            &AuditFilter {
                actor_id: query.actor,
                action: query.action.clone(),
                resource_type: query.resource.clone(),
                created_after: query.created_after,
                created_before: query.created_before,
            },
            pagination.cursor.as_deref(),
            pagination.effective_limit(),
        )
        .await?;

    Ok(Json(ListResponse::from_page(page, auth.request_id)))
}

// ---------------------------------------------------------------------------
// API keys (spec 05 endpoints 49-51)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
}

/// `POST /api-keys` (spec 05 endpoint 49).
///
/// The raw key is returned exactly once; only its hash is stored, so it cannot
/// be recovered afterwards.
pub async fn create_api_key(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("users:write")?;

    if body.name.trim().is_empty() {
        return Err(ApiError::validation("name must not be empty")
            .with_detail("name", "must not be empty"));
    }

    let key = forge_auth::generate_api_key();
    let id = Uuid::new_v4();
    // A short prefix so an operator can tell keys apart in a listing without
    // the prefix being enough to authenticate with.
    let prefix: String = key.raw.chars().take(12).collect();

    sqlx::query(
        "INSERT INTO api_keys (id, tenant_id, name, key_hash, prefix, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(body.name.trim())
    .bind(&key.hash)
    .bind(&prefix)
    .bind(Utc::now() + chrono::Duration::days(365))
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "id": id,
                "name": body.name.trim(),
                // Shown once, never again.
                "key": key.raw,
                "prefix": prefix,
                "warning": "this key is shown only once; store it now",
            }),
            auth.request_id,
        )),
    ))
}

/// `GET /api-keys` (spec 05 endpoint 50) — metadata only.
pub async fn list_api_keys(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'name', name, 'prefix', prefix,
             'revoked', revoked_at IS NOT NULL,
             'expires_at', expires_at, 'created_at', created_at,
             'last_used_at', last_used_at
         )
         FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let items = rows.into_iter().map(|(v,)| v).collect();
    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

/// `POST /api-keys/{id}/revoke` (spec 05 endpoint 51).
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(key_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    let affected = sqlx::query(
        "UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(key_id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("api key"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": key_id, "revoked": true }),
        auth.request_id,
    )))
}

/// `GET /integrations` (spec 05 endpoint 53).
pub async fn list_integrations(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("settings:write")?;

    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'kind', kind, 'name', name,
             'secret_reference', secret_reference, 'enabled', enabled,
             'created_at', created_at
         )
         FROM integration_configurations WHERE tenant_id = $1 ORDER BY kind, name",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The secret reference is an identifier, never the secret itself.
    let items = rows.into_iter().map(|(v,)| v).collect();
    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateIntegrationRequest {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub config: serde_json::Value,
    /// An identifier pointing at stored secret material, never the value.
    #[serde(default)]
    pub secret_reference: Option<String>,
}

/// `POST /integrations` (spec 05 endpoint 54).
pub async fn create_integration(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateIntegrationRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("settings:write")?;

    if body.kind.trim().is_empty() || body.name.trim().is_empty() {
        return Err(ApiError::validation("kind and name must not be empty")
            .with_detail("kind", "required"));
    }

    // A configuration carrying what looks like raw secret material is rejected
    // rather than stored (spec 11: secrets are referenced, never inlined).
    let rendered = body.config.to_string().to_lowercase();
    for marker in ["\"password\"", "\"secret\"", "\"token\"", "\"api_key\""] {
        if rendered.contains(marker) {
            return Err(ApiError::validation(
                "configuration must reference secrets, not embed them",
            )
            .with_detail("config", "use secret_reference"));
        }
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO integration_configurations
            (id, tenant_id, kind, name, config, secret_reference, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(body.kind.trim())
    .bind(body.name.trim())
    .bind(&body.config)
    .bind(&body.secret_reference)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({ "id": id, "kind": body.kind, "name": body.name }),
            auth.request_id,
        )),
    ))
}

#[derive(Debug, Deserialize)]
pub struct UpdateIntegrationRequest {
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub secret_reference: Option<String>,
}

/// `PATCH /integrations/{id}` (spec 05 endpoint 55).
pub async fn update_integration(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateIntegrationRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("settings:write")?;

    let affected = sqlx::query(
        "UPDATE integration_configurations SET
             config = COALESCE($3, config),
             enabled = COALESCE($4, enabled),
             secret_reference = COALESCE($5, secret_reference),
             updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(&body.config)
    .bind(body.enabled)
    .bind(&body.secret_reference)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("integration"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": id, "updated": true }),
        auth.request_id,
    )))
}

/// `DELETE /integrations/{id}` (spec 05 endpoint 56).
pub async fn delete_integration(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("settings:write")?;

    let affected = sqlx::query(
        "DELETE FROM integration_configurations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("integration"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": id, "deleted": true }),
        auth.request_id,
    )))
}

/// Admin endpoint: purges expired idempotency records (spec 02.15 retention).
pub async fn purge_idempotency(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("settings:write")?;

    // Batched, so the table is never locked for long (spec 08.7).
    let purged = IdempotencyRepository::new(&state.pool)
        .purge_expired(1000)
        .await?;

    Ok(Json(ApiResponse::new(
        json!({ "purged": purged }),
        auth.request_id,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AT-API-004: invalid input returns a standard error.
    #[test]
    fn an_invalid_key_name_is_rejected_with_a_field() {
        let error = ApiError::validation("name must not be empty")
            .with_detail("name", "must not be empty");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.details[0].field, "name");
    }

    /// Spec 11: an integration must reference secrets, not embed them.
    #[test]
    fn embedded_secrets_in_a_configuration_are_refused() {
        for config in [
            json!({ "password": "hunter2" }),
            json!({ "nested": { "api_key": "k" } }),
        ] {
            let rendered = config.to_string().to_lowercase();
            let refused = ["\"password\"", "\"secret\"", "\"token\"", "\"api_key\""]
                .iter()
                .any(|m| rendered.contains(m));
            assert!(refused, "{config} should be refused");
        }
    }

    #[test]
    fn a_benign_configuration_is_accepted() {
        let config = json!({ "endpoint": "https://example.com", "retries": 3 });
        let rendered = config.to_string().to_lowercase();
        assert!(
            !["\"password\"", "\"secret\"", "\"token\"", "\"api_key\""]
                .iter()
                .any(|m| rendered.contains(m))
        );
    }
}