//! Authentication endpoints (spec 05 endpoints 57–59) and user administration
//! (spec 05 endpoints 45–48).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use forge_auth::Role;
use forge_domain::TenantId;

use crate::envelope::{ApiError, ApiResponse, ListResponse};
use crate::extract::Auth;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    /// The tenant to create for a first user, or the name of a new one.
    #[serde(default)]
    pub tenant_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// `POST /auth/register` (spec 05 endpoint 57).
pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    let email = body.email.trim().to_ascii_lowercase();
    if !valid_email(&email) {
        return Err(
            ApiError::validation("email must be a valid address").with_detail("email", "invalid")
        );
    }
    forge_auth::check_password_strength(&body.password)?;

    // A user always belongs to a tenant; the first user of a named tenant also
    // creates it and becomes its owner.
    let tenant_id = ensure_tenant(&state, body.tenant_name.as_deref()).await?;
    let user_id = Uuid::new_v4();
    let password_hash = forge_auth::hash_password(&body.password)?;

    let inserted = sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(user_id)
    .bind(&email)
    .bind(&password_hash)
    .bind(body.display_name.as_deref().unwrap_or(&email))
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    if inserted.rows_affected() == 0 {
        // Spec 11: registration must not reveal that an address is already in
        // use, so this reads as a conflict on the address rather than an
        // invitation to try another one.
        return Err(ApiError::conflict("an account with that email already exists")
            .with_detail("email", "already registered"));
    }

    sqlx::query(
        "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'OWNER')",
    )
    .bind(user_id)
    .bind(tenant_id.into_uuid())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let tokens = issue_session(&state, user_id, tenant_id, Role::Owner).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "user_id": user_id,
                "tenant_id": tenant_id.into_uuid(),
                "role": "OWNER",
                "access_token": tokens.access_token,
                "refresh_token": tokens.refresh_token,
                "expires_in_secs": state.jwt.access_ttl().num_seconds(),
            }),
            uuid::Uuid::new_v4().to_string(),
        )),
    ))
}

/// Resolves or creates the tenant a new user belongs to.
async fn ensure_tenant(
    state: &AppState,
    tenant_name: Option<&str>,
) -> Result<TenantId, ApiError> {
    // A single-tenant deployment reuses the existing tenant rather than
    // fragmenting data across tenants created by successive sign-ups.
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM tenants ORDER BY created_at ASC LIMIT 1")
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;

    if let Some((id,)) = existing {
        return Ok(TenantId::from_uuid(id));
    }

    let id = Uuid::new_v4();
    let name = tenant_name.unwrap_or("default").trim();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(id)
        .bind(if name.is_empty() { "default" } else { name })
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;
    Ok(TenantId::from_uuid(id))
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// `POST /auth/login`.
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let email = body.email.trim().to_ascii_lowercase();

    // Look the user up, and always perform a hash comparison so the response
    // time does not reveal whether the address exists.
    // `tenant_id` is a native `uuid` column; `users.password_hash` is nullable
    // because an SSO-only account has no password.
    let row: Option<(Uuid, Option<String>, Option<Uuid>, bool)> = sqlx::query_as(
        "SELECT u.id, u.password_hash, tm.tenant_id, u.disabled
         FROM users u
         LEFT JOIN tenant_memberships tm ON tm.user_id = u.id
         WHERE u.email = $1
         LIMIT 1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((user_id, password_hash, tenant_uuid, disabled)) = row else {
        // No such user: do the work anyway, then refuse.
        let _ = forge_auth::verify_password(
            &body.password,
            &forge_auth::hash_password("dummy-password-for-timing")?,
        );
        return Err(ApiError::unauthenticated("invalid email or password"));
    };

    if disabled {
        return Err(ApiError::forbidden("this account has been disabled"));
    }

    let hash = password_hash.ok_or_else(|| {
        // An SSO-only account has no password to check.
        ApiError::unauthenticated("this account signs in through a provider")
    })?;

    if !forge_auth::verify_password(&body.password, &hash)? {
        return Err(ApiError::unauthenticated("invalid email or password"));
    }

    let tenant = match tenant_uuid {
        Some(id) => TenantId::from_uuid(id),
        None => return Err(ApiError::forbidden("this account has no tenant")),
    };
    let role = role_for(&state, user_id, tenant).await?;

    let tokens = issue_session(&state, user_id, tenant, role).await?;

    Ok(Json(ApiResponse::new(
        json!({
            "user_id": user_id,
            "tenant_id": tenant.into_uuid(),
            "role": role.as_str(),
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "expires_in_secs": state.jwt.access_ttl().num_seconds(),
        }),
        uuid::Uuid::new_v4().to_string(),
    )))
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// `POST /auth/refresh` (spec 05 endpoint 59).
///
/// Spec 11.3 requires rotating refresh tokens. Rotation with reuse detection
/// means a stolen token is single-use: presenting one that has already been
/// exchanged revokes the whole chain.
pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let presented = forge_auth::hash_token(&body.refresh_token);

    let row: Option<(Uuid, Uuid, bool)> = sqlx::query_as(
        "SELECT user_id, tenant_id, revoked_at IS NOT NULL
         FROM refresh_tokens WHERE token = $1",
    )
    .bind(&presented)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((user_id, tenant_uuid, revoked)) = row else {
        return Err(ApiError::unauthenticated("the refresh token is not valid"));
    };

    if revoked {
        // Reuse of an already-exchanged token: the chain is compromised.
        sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1")
            .bind(user_id)
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?;
        return Err(forge_auth::AuthError::RefreshTokenReuse.into());
    }

    sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE token = $1")
        .bind(&presented)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    let tenant = TenantId::from_uuid(tenant_uuid);
    let role = role_for(&state, user_id, tenant).await?;
    let tokens = issue_session(&state, user_id, tenant, role).await?;

    Ok(Json(ApiResponse::new(
        json!({
            "user_id": user_id,
            "tenant_id": tenant_uuid,
            "role": role.as_str(),
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "expires_in_secs": state.jwt.access_ttl().num_seconds(),
        }),
        uuid::Uuid::new_v4().to_string(),
    )))
}

struct SessionTokens {
    access_token: String,
    refresh_token: String,
}

/// Issues an access token and stores the matching refresh token.
///
/// Only the refresh token's hash is persisted; the raw value exists solely in
/// this response, so a database compromise does not yield usable sessions.
async fn issue_session(
    state: &AppState,
    user_id: Uuid,
    tenant: TenantId,
    role: Role,
) -> Result<SessionTokens, ApiError> {
    let access_token = state
        .jwt
        .issue(user_id, tenant.into_uuid(), role)
        .map_err(ApiError::from)?;
    let refresh = forge_auth::generate_refresh_token().map_err(ApiError::from)?;

    sqlx::query(
        "INSERT INTO refresh_tokens (token, user_id, tenant_id, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(&refresh.hash)
    .bind(user_id)
    .bind(tenant.into_uuid())
    .bind(refresh.expires_at)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(SessionTokens {
        access_token,
        refresh_token: refresh.raw,
    })
}

async fn role_for(
    state: &AppState,
    user_id: Uuid,
    tenant: TenantId,
) -> Result<Role, ApiError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM tenant_memberships WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(user_id)
    .bind(tenant.into_uuid())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    row.and_then(|(raw,)| Role::parse(&raw))
        .ok_or_else(|| ApiError::forbidden("this account has no role in the tenant"))
}

/// `POST /auth/logout` — revokes every refresh token for the caller.
pub async fn logout(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = NOW()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({ "signed_out": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// User administration (spec 05 endpoints 45-48)
// ---------------------------------------------------------------------------

/// A user row joined with its role in this tenant.
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    display_name: Option<String>,
    role: String,
    disabled: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn user_view(row: &UserRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "email": row.email,
        "display_name": row.display_name,
        "role": row.role,
        "disabled": row.disabled,
        "created_at": row.created_at,
    })
}

/// `GET /users` (spec 05 endpoint 45).
pub async fn list_users(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ListResponse<serde_json::Value>>, ApiError> {
    auth.require("users:read")?;

    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.email, u.display_name, tm.role, u.disabled, u.created_at
         FROM users u
         JOIN tenant_memberships tm ON tm.user_id = u.id
         WHERE tm.tenant_id = $1
         ORDER BY u.created_at ASC",
    )
    .bind(auth.tenant_id.into_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let items = rows.iter().map(user_view).collect();
    Ok(Json(ListResponse::new(
        items,
        Default::default(),
        auth.request_id,
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    pub role: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// `POST /users` (spec 05 endpoint 46).
pub async fn create_user(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("users:write")?;

    let email = body.email.trim().to_ascii_lowercase();
    if !valid_email(&email) {
        return Err(ApiError::validation("email must be a valid address")
            .with_detail("email", "invalid"));
    }
    forge_auth::check_password_strength(&body.password)?;

    let role = Role::parse(&body.role).ok_or_else(|| {
        ApiError::validation(format!("`{}` is not a valid role", body.role)).with_detail(
            "role",
            "expected OWNER, ADMIN, OPERATOR, DEVELOPER, AUDITOR or VIEWER",
        )
    })?;

    // An admin must not mint an owner, or privilege could escalate.
    if role == Role::Owner && !auth.can("tenants:delete") {
        return Err(ApiError::forbidden(
            "only an owner may create another owner",
        ));
    }

    let user_id = Uuid::new_v4();
    let hash = forge_auth::hash_password(&body.password)?;
    let inserted = sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(user_id)
    .bind(&email)
    .bind(&hash)
    .bind(body.display_name.as_deref().unwrap_or(&email))
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    if inserted.rows_affected() == 0 {
        return Err(ApiError::conflict("an account with that email already exists")
            .with_detail("email", "already registered"));
    }

    sqlx::query(
        "INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(auth.tenant_id.into_uuid())
    .bind(role.as_str())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({ "id": user_id, "email": email, "role": role.as_str() }),
            auth.request_id,
        )),
    ))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub role: Option<String>,
}

/// `PATCH /users/{id}` (spec 05 endpoint 47).
pub async fn update_user(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(user_id): Path<Uuid>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    if let Some(name) = &body.display_name {
        sqlx::query("UPDATE users SET display_name = $2 WHERE id = $1")
            .bind(user_id)
            .bind(name)
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?;
    }

    if let Some(raw) = &body.role {
        let role = Role::parse(raw).ok_or_else(|| {
            ApiError::validation(format!("`{raw}` is not a valid role")).with_detail("role", "unknown")
        })?;
        if role == Role::Owner && !auth.can("tenants:delete") {
            return Err(ApiError::forbidden("only an owner may grant ownership"));
        }
        let affected = sqlx::query(
            "UPDATE tenant_memberships SET role = $3
             WHERE user_id = $1 AND tenant_id = $2",
        )
        .bind(user_id)
        .bind(auth.tenant_id.into_uuid())
        .bind(role.as_str())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

        if affected == 0 {
            return Err(ApiError::not_found("user"));
        }
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": user_id, "updated": true }),
        auth.request_id,
    )))
}

/// `POST /users/{id}/disable` (spec 05 endpoint 48).
pub async fn disable_user(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    // Disabling oneself would lock the caller out mid-request.
    if user_id == auth.user_id {
        return Err(ApiError::validation("you cannot disable your own account")
            .with_detail("user", "refusing to disable the caller"));
    }

    sqlx::query("UPDATE users SET disabled = TRUE WHERE id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    // A disabled account must not keep refreshing.
    sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse::new(
        json!({ "id": user_id, "disabled": true }),
        auth.request_id,
    )))
}

fn valid_email(email: &str) -> bool {
    // A pragmatic check: one `@`, something either side, no whitespace.
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    parts.next().is_none()
        && !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !email.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_email_is_accepted() {
        for good in ["a@b.com", "first.last@example.co.uk", "x+tag@sub.domain.org"] {
            assert!(valid_email(good), "{good} should be valid");
        }
    }

    #[test]
    fn a_malformed_email_is_rejected() {
        for bad in [
            "",
            "no-at-sign",
            "@domain.com",
            "local@",
            "local@domain",
            "a@b@c.com",
            "has space@x.com",
            "has\ttab@x.com",
        ] {
            assert!(!valid_email(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn the_refresh_window_matches_the_token_ttl() {
        assert_eq!(forge_auth::REFRESH_TTL_DAYS, 30);
    }
}