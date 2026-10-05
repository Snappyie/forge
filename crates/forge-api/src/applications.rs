//! Application and environment endpoints (`redesign.md` §D, §2.G).
//!
//! PowerJob's defining idea is that work is grouped under an *application*;
//! this API is where that grouping is created and read. Environments are the
//! other half: the axis a job migrates along and the thing a production
//! guardrail keys off.
//!
//! Both are addressable by slug as well as by id, because a slug is what an
//! operator types and what appears in a URL and a deployment manifest.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_domain::{ApplicationId, EnvironmentId, EnvironmentKind, Slug, TenantId};
use forge_storage::{
    ApplicationRepository, DeleteEnvironmentError, EnvironmentRepository, NewApplication,
    NewEnvironment,
};

use crate::envelope::{ApiError, ApiResponse};
use crate::extract::Auth;
use crate::router::AppState;

// ---------------------------------------------------------------------------
// Applications
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateApplicationRequest {
    pub name: String,
    /// Optional: derived from `name` when absent, so an operator who types
    /// "Payments API (EU)" gets a usable slug without being asked for one.
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateApplicationRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// `POST /applications`.
pub async fn create_application(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateApplicationRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("jobs:write")?;
    validate_name("name", &body.name)?;

    // A supplied slug is validated strictly; a derived one is normalised, so
    // `slug: "Payments API"` is a 400 (the caller named something invalid) while
    // `name: "Payments API"` derives a valid slug automatically.
    let slug = match body.slug.as_deref() {
        Some(raw) => Slug::try_from(raw.trim().to_string()).map_err(|e| {
            ApiError::validation(e.to_string()).with_detail("slug", "see the slug rules")
        })?,
        None => Slug::parse(&body.name)
            .map_err(|e| ApiError::validation(format!("`name` cannot form a slug: {e}")))?,
    };

    let repo = ApplicationRepository::new(&state.pool);
    let row = repo
        .create(&NewApplication {
            tenant_id: auth.tenant_id,
            slug: slug.clone(),
            name: body.name.clone(),
            description: body.description.clone(),
        })
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            application_view(&row, 0).await,
            auth.request_id,
        )),
    ))
}

/// `GET /applications`.
pub async fn list_applications(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let repo = ApplicationRepository::new(&state.pool);
    let rows = repo.list(auth.tenant_id).await.map_err(ApiError::from)?;

    // Counts are collected in one pass rather than one query per row: the
    // console renders this list on every navigation, and a per-row count would
    // make it N+1 against the most-read screen in the product.
    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let count = repo
            .job_count(auth.tenant_id, ApplicationId::from_uuid(row.id))
            .await
            .map_err(ApiError::from)?;
        items.push(application_view(row, count).await);
    }

    Ok(Json(ApiResponse::new(
        json!({ "applications": items }),
        auth.request_id,
    )))
}

/// `GET /applications/{id}`.
pub async fn get_application(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let repo = ApplicationRepository::new(&state.pool);
    // A row in another tenant reads as not-found rather than forbidden, so the
    // error does not confirm that the id exists somewhere (spec 11 / AT-TEN-003).
    let row = repo
        .get(auth.tenant_id, ApplicationId::from_uuid(id))
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("application"))?;

    let count = repo
        .job_count(auth.tenant_id, ApplicationId::from_uuid(row.id))
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        application_view(&row, count).await,
        auth.request_id,
    )))
}

/// `GET /applications/by-slug/{slug}`.
///
/// The slug-addressed form, because a slug is what appears in a deployment
/// manifest and in a URL an operator bookmarks.
pub async fn get_application_by_slug(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let repo = ApplicationRepository::new(&state.pool);
    let row = repo
        .get_by_slug(auth.tenant_id, slug.trim())
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("application"))?;

    let count = repo
        .job_count(auth.tenant_id, ApplicationId::from_uuid(row.id))
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        application_view(&row, count).await,
        auth.request_id,
    )))
}

/// `PATCH /applications/{id}`.
pub async fn update_application(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateApplicationRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:write")?;
    validate_name("name", &body.name)?;

    let repo = ApplicationRepository::new(&state.pool);
    let row = repo
        .update(
            auth.tenant_id,
            ApplicationId::from_uuid(id),
            &body.name,
            body.description.as_deref(),
        )
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("application"))?;

    let count = repo
        .job_count(auth.tenant_id, ApplicationId::from_uuid(row.id))
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        application_view(&row, count).await,
        auth.request_id,
    )))
}

/// `DELETE /applications/{id}`.
///
/// Refused when jobs still reference it. The column is nullable so the database
/// would allow it, but silently ungrouping a tenant's production jobs is never
/// what the operator meant.
pub async fn delete_application(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:write")?;

    let repo = ApplicationRepository::new(&state.pool);
    let application_id = ApplicationId::from_uuid(id);
    let count = repo
        .job_count(auth.tenant_id, application_id)
        .await
        .map_err(ApiError::from)?;

    if count > 0 {
        return Err(
            ApiError::conflict(format!("{count} job(s) still belong to this application"))
                .with_detail("job_count", count.to_string()),
        );
    }

    if repo
        .delete(auth.tenant_id, application_id)
        .await
        .map_err(ApiError::from)?
    {
        Ok(Json(ApiResponse::new(
            json!({ "deleted": true }),
            auth.request_id,
        )))
    } else {
        Err(ApiError::not_found("application"))
    }
}

async fn application_view(
    row: &forge_storage::ApplicationRow,
    job_count: i64,
) -> serde_json::Value {
    json!({
        "id": row.id,
        "slug": row.slug,
        "name": row.name,
        "description": row.description,
        "labels": row.labels,
        "job_count": job_count,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

// ---------------------------------------------------------------------------
// Environments
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    /// One of `development`, `staging`, `production`, `other`.
    pub kind: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// `POST /environments`.
pub async fn create_environment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateEnvironmentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("settings:write")?;
    validate_name("name", &body.name)?;

    use std::str::FromStr;
    let kind = EnvironmentKind::from_str(&body.kind).map_err(|e| {
        // Refused rather than defaulted: defaulting an unrecognised name to
        // production would flag harmless environments, and defaulting to
        // development would let an operator create an unguarded `PRODUCTION-1`.
        ApiError::validation(e.to_string()).with_detail("kind", &body.kind)
    })?;

    let slug = match body.slug.as_deref() {
        Some(raw) => Slug::try_from(raw.trim().to_string()).map_err(|e| {
            ApiError::validation(e.to_string()).with_detail("slug", "see the slug rules")
        })?,
        None => Slug::parse(&body.name)
            .map_err(|e| ApiError::validation(format!("`name` cannot form a slug: {e}")))?,
    };

    let repo = EnvironmentRepository::new(&state.pool);
    let row = repo
        .create(&NewEnvironment {
            tenant_id: auth.tenant_id,
            slug,
            name: body.name.clone(),
            kind,
            description: body.description.clone(),
        })
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(environment_view(&row), auth.request_id)),
    ))
}

/// `GET /environments`.
pub async fn list_environments(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let repo = EnvironmentRepository::new(&state.pool);
    let rows = repo.list(auth.tenant_id).await.map_err(ApiError::from)?;
    let items: Vec<_> = rows.iter().map(environment_view).collect();

    Ok(Json(ApiResponse::new(
        json!({ "environments": items }),
        auth.request_id,
    )))
}

/// `GET /environments/{id}`.
pub async fn get_environment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let repo = EnvironmentRepository::new(&state.pool);
    let row = repo
        .get(auth.tenant_id, EnvironmentId::from_uuid(id))
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("environment"))?;

    Ok(Json(ApiResponse::new(
        environment_view(&row),
        auth.request_id,
    )))
}

/// `GET /environments/by-slug/{slug}`.
pub async fn get_environment_by_slug(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("jobs:read")?;

    let repo = EnvironmentRepository::new(&state.pool);
    let row = repo
        .get_by_slug(auth.tenant_id, slug.trim())
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("environment"))?;

    Ok(Json(ApiResponse::new(
        environment_view(&row),
        auth.request_id,
    )))
}

/// `PATCH /environments/{id}`.
///
/// `slug` and `kind` are not editable: the slug is a URL identifier other
/// systems bookmark, and `kind` is what the production guardrail reads.
pub async fn update_environment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEnvironmentRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("settings:write")?;
    validate_name("name", &body.name)?;

    let repo = EnvironmentRepository::new(&state.pool);
    let row = repo
        .update(
            auth.tenant_id,
            EnvironmentId::from_uuid(id),
            &body.name,
            body.description.as_deref(),
        )
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("environment"))?;

    Ok(Json(ApiResponse::new(
        environment_view(&row),
        auth.request_id,
    )))
}

/// `DELETE /environments/{id}`.
pub async fn delete_environment(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("settings:write")?;

    let repo = EnvironmentRepository::new(&state.pool);
    match repo
        .delete(auth.tenant_id, EnvironmentId::from_uuid(id))
        .await
    {
        Ok(()) => Ok(Json(ApiResponse::new(
            json!({ "deleted": true }),
            auth.request_id,
        ))),
        // A refusal is a 409 with the count, not a 500: the request was
        // understood and the answer is "no, and here is why".
        Err(DeleteEnvironmentError::StillInUse(count)) => Err(ApiError::conflict(format!(
            "{count} job(s) still belong to this environment"
        ))
        .with_detail("job_count", count.to_string())),
        Err(DeleteEnvironmentError::NotFound) => Err(ApiError::not_found("environment")),
        Err(DeleteEnvironmentError::Storage(e)) => Err(ApiError::from(e)),
    }
}

fn environment_view(row: &forge_storage::EnvironmentRow) -> serde_json::Value {
    // `protected` is computed server-side from the stored kind rather than
    // derived by the console: the guardrail must not depend on every client
    // implementing the rule correctly.
    let protected = row
        .environment_kind()
        .map(|k| k.is_protected())
        .unwrap_or(false);

    json!({
        "id": row.id,
        "slug": row.slug,
        "name": row.name,
        "kind": row.kind,
        "description": row.description,
        "protected": protected,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// Validates a human-supplied name.
///
/// The message names the field, because this is called from two places and
/// "name is required" without saying which is unhelpful.
fn validate_name(field: &str, value: &str) -> Result<(), ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(
            ApiError::validation(format!("`{field}` is required")).with_detail(field, "required")
        );
    }
    if trimmed.chars().count() > 200 {
        return Err(
            ApiError::validation(format!("`{field}` must be under 200 characters"))
                .with_detail(field, "too long"),
        );
    }
    Ok(())
}

/// Resolves an environment slug within a tenant.
///
/// Public because job creation and the migration path both file jobs under an
/// environment, and both must resolve it the same way — a second, subtly
/// different lookup is how a job ends up in the wrong environment.
pub async fn resolve_environment_slug(
    state: &AppState,
    tenant_id: TenantId,
    slug: &str,
) -> Result<Uuid, ApiError> {
    forge_storage::EnvironmentRepository::new(&state.pool)
        .get_by_slug(tenant_id, slug.trim())
        .await
        .map_err(ApiError::from)?
        .map(|row| row.id)
        .ok_or_else(|| ApiError::not_found("environment"))
}
