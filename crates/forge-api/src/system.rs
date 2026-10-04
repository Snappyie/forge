//! Health, audit, API key, and metrics endpoints (spec 05 endpoints 49–62).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
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
    let database = sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok();

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

    let migrations: Vec<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
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
pub async fn metrics(State(state): State<AppState>, Auth(auth): Auth) -> Result<String, ApiError> {
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
    /// The role the key acts as (spec 11.4: granular permissions).
    ///
    /// Defaults to the creator's own role capped at ADMIN, so a leaked service
    /// key is never the tenant OWNER and a key cannot be used to escalate.
    #[serde(default)]
    pub role: Option<String>,
    /// How long the key stays valid. Defaults to 365 days, capped at 10 years.
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

/// The role an API key gets when the caller does not name one.
///
/// OWNER is deliberately excluded: spec 11.1's least-privilege principle means a
/// machine credential should not be able to delete the tenant.
fn default_api_key_role(creator: forge_auth::Role) -> forge_auth::Role {
    match creator {
        forge_auth::Role::Owner => forge_auth::Role::Admin,
        other => other,
    }
}

/// Refuses a key that would hold more than the person creating it.
fn authorize_key_role(
    creator: forge_auth::Role,
    requested: forge_auth::Role,
) -> Result<(), ApiError> {
    if creator == forge_auth::Role::Owner {
        return Ok(());
    }
    if requested == forge_auth::Role::Owner {
        return Err(ApiError::forbidden(
            "only a tenant owner may create an owner-scoped API key",
        ));
    }
    let within_creator = requested
        .permissions()
        .iter()
        .all(|permission| creator.allows(permission));
    if within_creator {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "{} may not grant a key the {} role",
            creator, requested
        )))
    }
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
        return Err(
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty")
        );
    }

    let role = match body.role.as_deref() {
        None => default_api_key_role(auth.role),
        Some(raw) => forge_auth::Role::parse(raw).ok_or_else(|| {
            ApiError::validation(format!("`{raw}` is not a valid role"))
                .with_detail("role", "invalid")
        })?,
    };
    authorize_key_role(auth.role, role)?;

    let days = body.expires_in_days.unwrap_or(365);
    if !(1..=3650).contains(&days) {
        return Err(
            ApiError::validation("expires_in_days must be between 1 and 3650")
                .with_detail("expires_in_days", "out of range"),
        );
    }

    let key = forge_auth::generate_api_key(&state.api_key_pepper);
    let id = Uuid::new_v4();
    // A short prefix so an operator can tell keys apart in a listing without
    // the prefix being enough to authenticate with.
    let prefix: String = key.raw.chars().take(12).collect();

    sqlx::query(
        "INSERT INTO api_keys (id, tenant_id, owner_id, role, name, key_hash, prefix, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(auth.user_id)
    .bind(role.as_str())
    .bind(body.name.trim())
    .bind(&key.hash)
    .bind(&prefix)
    .bind(Utc::now() + chrono::Duration::days(days))
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "api_keys:create",
            "API_KEY",
            Some(id),
        ))
        .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "id": id,
                "name": body.name.trim(),
                "role": role.as_str(),
                "expires_at": Utc::now() + chrono::Duration::days(days),
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

    let affected =
        sqlx::query("UPDATE api_keys SET revoked_at = NOW() WHERE id = $1 AND tenant_id = $2")
            .bind(key_id)
            .bind(auth.tenant_id.into_uuid())
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?
            .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("api key"));
    }

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "api_keys:revoke",
            "API_KEY",
            Some(key_id),
        ))
        .await;

    Ok(Json(ApiResponse::new(
        json!({ "id": key_id, "revoked": true }),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateServiceAccountRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires_in_days: Option<i64>,
}

/// `POST /service-accounts` — mint a new scoped service account.
pub async fn create_service_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateServiceAccountRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("users:write")?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::validation("name is required").with_detail("name", "empty"));
    }

    let token = forge_auth::generate_service_account_token(&state.api_key_pepper);
    let prefix: String = token.raw.chars().take(15).collect();
    let raw_token = token.raw;
    let hash = token.hash;

    let id = Uuid::new_v4();
    let expires_at = body
        .expires_in_days
        .map(|days| Utc::now() + chrono::Duration::days(days.max(1)));

    sqlx::query(
        "INSERT INTO service_accounts (id, tenant_id, name, description, token_hash, token_prefix, scopes, expires_at, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(name)
    .bind(body.description.as_deref())
    .bind(&hash)
    .bind(&prefix)
    .bind(&body.scopes)
    .bind(expires_at)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "service_accounts:create",
            "SERVICE_ACCOUNT",
            Some(id),
        ))
        .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "id": id,
                "name": name,
                "description": body.description,
                "token": raw_token,
                "prefix": prefix,
                "scopes": body.scopes,
                "expires_at": expires_at,
                "warning": "this token is shown only once; store it now",
            }),
            auth.request_id,
        )),
    ))
}

/// `GET /service-accounts` — list active and revoked service accounts.
pub async fn list_service_accounts(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("users:read")?;

    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', id, 'name', name, 'description', description,
             'prefix', token_prefix, 'scopes', scopes,
             'revoked', revoked_at IS NOT NULL,
             'expires_at', expires_at, 'created_at', created_at,
             'last_used_at', last_used_at
         )
         FROM service_accounts WHERE tenant_id = $1 ORDER BY created_at DESC",
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

/// `POST /service-accounts/{id}/revoke` — revoke a service account immediately.
pub async fn revoke_service_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(sa_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    let affected = sqlx::query(
        "UPDATE service_accounts SET revoked_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(sa_id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("service account"));
    }

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "service_accounts:revoke",
            "SERVICE_ACCOUNT",
            Some(sa_id),
        ))
        .await;

    Ok(Json(ApiResponse::new(
        json!({ "id": sa_id, "revoked": true }),
        auth.request_id,
    )))
}

/// `DELETE /service-accounts/{id}` — delete a service account.
pub async fn delete_service_account(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(sa_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    let affected = sqlx::query("DELETE FROM service_accounts WHERE id = $1 AND tenant_id = $2")
        .bind(sa_id)
        .bind(auth.tenant_id.into_uuid())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("service account"));
    }

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "service_accounts:delete",
            "SERVICE_ACCOUNT",
            Some(sa_id),
        ))
        .await;

    Ok(Json(ApiResponse::new(
        json!({ "id": sa_id, "deleted": true }),
        auth.request_id,
    )))
}

/// `GET /integrations` (spec 05 endpoint 53).
/// `GET /integrations` (spec 05 endpoint 53, UI.md section 74).
///
/// Returns the configured integrations plus the full set the spec names and
/// which of them are not configured yet.
pub async fn list_integrations(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("integrations:read")?;

    // One JSON column, so the rows are plain values rather than 1-tuples.
    let rows: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT json_build_object(
             'id', id, 'kind', kind, 'name', name,
             -- An identifier pointing at stored material, never the secret.
             'secret_reference', secret_reference,
             'has_credentials', secret_reference IS NOT NULL,
             'enabled', enabled,
             'last_success_at', last_success_at,
             'last_failure_at', last_failure_at,
             'last_error', last_error,
             'last_checked_at', last_checked_at,
             'created_at', created_at
         )
         FROM integration_configurations WHERE tenant_id = $1 ORDER BY kind, name",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    // The spec names ten integration kinds; report the full set so the console
    // can show what is available and what is not yet configured.
    let configured: Vec<String> = rows
        .iter()
        .filter_map(|v| v.get("kind").and_then(|k| k.as_str()).map(String::from))
        .collect();
    let not_configured: Vec<&str> = SUPPORTED_INTEGRATION_KINDS
        .iter()
        .copied()
        .filter(|kind| !configured.iter().any(|c| c == kind))
        .collect();

    Ok(Json(ApiResponse::new(
        json!({
            "data": rows,
            "supported": SUPPORTED_INTEGRATION_KINDS,
            "not_configured": not_configured,
        }),
        auth.request_id,
    )))
}

/// The integration kinds UI.md section 74 names.
pub const SUPPORTED_INTEGRATION_KINDS: &[&str] = &[
    "POSTGRESQL",
    "KAFKA",
    "REDIS",
    "KUBERNETES",
    "SLACK",
    "TEAMS",
    "EMAIL",
    "PAGERDUTY",
    "CLOUD_PROVIDER",
    "SECRET_MANAGER",
];

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
    auth.require("integrations:write")?;

    if body.kind.trim().is_empty() || body.name.trim().is_empty() {
        return Err(
            ApiError::validation("kind and name must not be empty").with_detail("kind", "required")
        );
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
    auth.require("integrations:write")?;

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
    auth.require("integrations:write")?;

    let affected =
        sqlx::query("DELETE FROM integration_configurations WHERE id = $1 AND tenant_id = $2")
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

/// `POST /integrations/{id}/test` — attempt a connection check (UI.md section 74).
///
/// The check is recorded rather than faked: Forge does not dial arbitrary
/// endpoints on an authenticated request, so the response states what was
/// checked and reports `connection_verified: false` instead of claiming a
/// success it cannot verify.
pub async fn test_integration(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("integrations:write")?;

    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT json_build_object('id', id, 'name', name, 'kind', kind)
         FROM integration_configurations WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((integration,)) = row else {
        return Err(ApiError::not_found("integration"));
    };

    // Record that a check happened, so the console's "last checked" is a fact
    // rather than an assumption.
    sqlx::query(
        "UPDATE integration_configurations
         SET last_checked_at = NOW(), updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({
            "integration": integration,
            "checked_at": chrono::Utc::now(),
            "connection_verified": false,
            "detail": "Configuration inspected. Forge does not dial external \
                       endpoints on request, so no connection was attempted.",
        }),
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

// ---------------------------------------------------------------------------
// Teams
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub description: Option<String>,
    pub on_call_email: Option<String>,
}

/// `GET /teams`
pub async fn list_teams(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<Value>>, ApiError> {
    auth.require("users:read")?;

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT json_build_object(
             'id', t.id,
             'name', t.name,
             'description', t.description,
             'on_call', t.on_call_email,
             'members', (SELECT COUNT(*) FROM team_members m WHERE m.team_id = t.id),
             'created_at', t.created_at
         )
         FROM teams t
         WHERE t.tenant_id = $1
         ORDER BY t.name ASC",
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

/// `POST /teams`
pub async fn create_team(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateTeamRequest>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    auth.require("users:write")?;

    if body.name.trim().is_empty() {
        return Err(ApiError::validation("name must not be empty"));
    }

    let id = Uuid::new_v4();
    let row: (Value,) = sqlx::query_as(
        "INSERT INTO teams (id, tenant_id, name, description, on_call_email)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING json_build_object(
             'id', id, 'name', name, 'description', description,
             'on_call', on_call_email, 'created_at', created_at
         )",
    )
    .bind(id)
    .bind(auth.tenant_id.into_uuid())
    .bind(body.name.trim())
    .bind(body.description)
    .bind(body.on_call_email)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(row.0, auth.request_id)),
    ))
}

/// `GET /teams/{id}`
pub async fn get_team(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(team_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("users:read")?;

    let row: (Value,) = sqlx::query_as(
        "SELECT json_build_object(
             'id', t.id,
             'name', t.name,
             'description', t.description,
             'on_call', t.on_call_email,
             'members', (SELECT COUNT(*) FROM team_members m WHERE m.team_id = t.id),
             'created_at', t.created_at
         )
         FROM teams t
         WHERE t.id = $1 AND t.tenant_id = $2",
    )
    .bind(team_id)
    .bind(auth.tenant_id.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError::not_found("team"))?;

    Ok(Json(ApiResponse::new(row.0, auth.request_id)))
}

/// `DELETE /teams/{id}`
pub async fn delete_team(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(team_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    auth.require("users:write")?;

    let affected = sqlx::query("DELETE FROM teams WHERE id = $1 AND tenant_id = $2")
        .bind(team_id)
        .bind(auth.tenant_id.into_uuid())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("team"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": team_id, "deleted": true }),
        auth.request_id,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AT-API-004: invalid input returns a standard error.
    #[test]
    fn an_invalid_key_name_is_rejected_with_a_field() {
        let error =
            ApiError::validation("name must not be empty").with_detail("name", "must not be empty");
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
        assert!(!["\"password\"", "\"secret\"", "\"token\"", "\"api_key\""]
            .iter()
            .any(|m| rendered.contains(m)));
    }
}
