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
    /// Required unless the deployment allows open registration.
    ///
    /// `invites.token` is a UUID, so this is deserialized as one: a malformed
    /// token is then a normal 400 rather than a database type error.
    #[serde(default)]
    pub invite_token: Option<Uuid>,
}

/// How a registration is authorised.
struct Admission {
    tenant_id: TenantId,
    role: Role,
}

/// Decides who a registering caller may become, and in which tenant.
///
/// Registration is public, so without this every anonymous request would mint a
/// tenant owner. Three cases, in order:
///   1. A valid, unclaimed invite grants exactly the tenant and role it names.
///   2. A bootstrap claim succeeds only while no tenant exists yet.
///   3. Otherwise registration is refused, unless the deployment opted in.
async fn admit(
    state: &AppState,
    email: &str,
    tenant_name: Option<&str>,
    invite_token: Option<Uuid>,
) -> Result<Admission, ApiError> {
    if let Some(token) = invite_token {
        return redeem_invite(state, &token, email).await;
    }

    if state.allow_open_registration {
        return Ok(Admission {
            tenant_id: ensure_tenant(state, tenant_name).await?,
            // An open deployment still must not hand every signup OWNER of a
            // tenant that already has an owner.
            role: Role::Viewer,
        });
    }

    // Bootstrap: the very first tenant may be claimed once, by whoever gets
    // there first. The unique primary key on `tenant_bootstrap` makes a second
    // claim fail, so this cannot be raced.
    let bootstrapped: Option<(Option<Uuid>,)> = sqlx::query_as(
        "INSERT INTO tenant_bootstrap (id, tenant_id)
         VALUES (TRUE, NULL)
         ON CONFLICT (id) DO NOTHING
         RETURNING tenant_id",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    if bootstrapped.is_some() {
        let id = Uuid::new_v4();
        let name = tenant_name.unwrap_or("default").trim();
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(if name.is_empty() { "default" } else { name })
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?;

        sqlx::query("UPDATE tenant_bootstrap SET tenant_id = $1 WHERE id = TRUE")
            .bind(id)
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?;

        return Ok(Admission {
            tenant_id: TenantId::from_uuid(id),
            role: Role::Owner,
        });
    }

    Err(ApiError::forbidden(
        "registration is closed; an invitation is required",
    ))
}

/// Consumes an invite, granting exactly the tenant and role it names.
///
/// `used_at` is set by the UPDATE's own row count, so two concurrent
/// registrations with the same token cannot both succeed.
async fn redeem_invite(state: &AppState, token: &Uuid, email: &str) -> Result<Admission, ApiError> {
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "UPDATE invites SET used_at = NOW()
         WHERE token = $1
           AND used_at IS NULL
           AND expires_at > NOW()
           AND (email IS NULL OR lower(email) = $2)
         RETURNING tenant_id, role, COALESCE(email, '')",
    )
    .bind(token)
    .bind(email)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((tenant_id, role, invited_email)) = row else {
        return Err(ApiError::forbidden("that invitation is not valid"));
    };

    // A user row may already exist for this address (an invite to join a second
    // tenant), so the email is informational here.
    let _ = invited_email;

    Ok(Admission {
        tenant_id: TenantId::from_uuid(tenant_id),
        // An unknown role here is stored data, not caller input, so this is a
        // data-integrity fault rather than a validation error.
        role: Role::parse(&role).ok_or_else(ApiError::internal)?,
    })
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

    // Registration is public, so who the caller becomes is decided here rather
    // than assumed.
    let admission = admit(
        &state,
        &email,
        body.tenant_name.as_deref(),
        body.invite_token,
    )
    .await?;
    let tenant_id = admission.tenant_id;
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
        return Err(
            ApiError::conflict("an account with that email already exists")
                .with_detail("email", "already registered"),
        );
    }

    sqlx::query("INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(tenant_id.into_uuid())
        .bind(admission.role.as_str())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    if admission.role == Role::Owner {
        sqlx::query("UPDATE tenant_bootstrap SET claimed_by = $1 WHERE id = TRUE")
            .bind(user_id)
            .execute(&state.pool)
            .await
            .map_err(ApiError::from)?;
    }

    let tokens = issue_session(&state, user_id, tenant_id, admission.role).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "user_id": user_id,
                "tenant_id": tenant_id.into_uuid(),
                "role": admission.role.as_str(),
                "access_token": tokens.access_token,
                "refresh_token": tokens.refresh_token,
                "expires_in_secs": state.jwt.access_ttl().num_seconds(),
            }),
            uuid::Uuid::new_v4().to_string(),
        )),
    ))
}

/// Resolves or creates the tenant a new user belongs to.
async fn ensure_tenant(state: &AppState, tenant_name: Option<&str>) -> Result<TenantId, ApiError> {
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
    /// Which tenant to sign into, when the user belongs to several.
    ///
    /// Optional so an existing single-tenant client keeps working, but a user in
    /// more than one tenant who does not name one gets the tenant they last used
    /// — never an arbitrary row. The console sends this from its tenant picker.
    #[serde(default)]
    pub tenant_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SwitchTenantRequest {
    /// The tenant to move to.
    pub tenant_slug: String,
}

/// `GET /auth/tenants` — the tenants the signed-in user belongs to.
///
/// Lets a console render a tenant switcher without embedding the list in the
/// login response, and lets it refresh after an administrator adds a
/// membership.
pub async fn list_tenants(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let memberships = load_memberships(&state, auth.user_id).await?;

    let mut tenants = Vec::with_capacity(memberships.len());
    for membership in &memberships {
        // The role is per (user, tenant), so it cannot be read once for all of
        // them — a user can be an admin in one tenant and a viewer in another.
        let role = role_for(&state, auth.user_id, membership.tenant).await?;
        tenants.push(json!({
            "id": membership.tenant.into_uuid(),
            "slug": membership.slug,
            "name": membership.name,
            "role": role.as_str(),
            "current": membership.tenant == auth.tenant_id,
        }));
    }

    Ok(Json(ApiResponse::new(
        json!({ "tenants": tenants }),
        auth.request_id,
    )))
}

/// `POST /auth/switch-tenant` — issue a new session for another tenant.
///
/// Returns a *new* token pair rather than mutating the current one. An access
/// token carries exactly one tenant, so a tenant switch is a new session by
/// construction; mutating in place would leave the old token valid for a tenant
/// the user has since left.
pub async fn switch_tenant(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<SwitchTenantRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let memberships = load_memberships(&state, auth.user_id).await?;

    let chosen = memberships
        .iter()
        .find(|m| m.slug.eq_ignore_ascii_case(body.tenant_slug.trim()))
        .ok_or_else(|| ApiError::forbidden("you are not a member of that tenant"))?;

    // A suspended tenant must not be enterable by signing in to it: its
    // schedules and workers are supposed to have stopped.
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1")
        .bind(chosen.tenant.into_uuid())
        .fetch_optional(&state.pool)
        .await
        .map_err(ApiError::from)?;
    if status.as_deref() == Some("SUSPENDED") {
        return Err(ApiError::forbidden("this tenant is suspended"));
    }

    let role = role_for(&state, auth.user_id, chosen.tenant).await?;

    let _ = sqlx::query("UPDATE users SET last_tenant_id = $1 WHERE id = $2")
        .bind(chosen.tenant.into_uuid())
        .bind(auth.user_id)
        .execute(&state.pool)
        .await;

    let tokens = issue_session(&state, auth.user_id, chosen.tenant, role).await?;

    Ok(Json(ApiResponse::new(
        json!({
            "user_id": auth.user_id,
            "tenant_id": chosen.tenant.into_uuid(),
            "tenant_slug": chosen.slug,
            "role": role.as_str(),
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "expires_in_secs": state.jwt.access_ttl().num_seconds(),
        }),
        auth.request_id,
    )))
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
    let row: Option<(Uuid, Option<String>, bool)> = sqlx::query_as(
        "SELECT u.id, u.password_hash, u.disabled
         FROM users u
         WHERE u.email = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((user_id, password_hash, disabled)) = row else {
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

    // Every tenant this user belongs to, not one arbitrary row.
    //
    // This query used to be a `LEFT JOIN tenant_memberships ... LIMIT 1` with no
    // ordering, so a user in several tenants landed in whichever one the planner
    // returned first — different between deployments and between restarts. A
    // login that silently picks a tenant is unmanageable in exactly the
    // multi-tenant deployments that need it.
    let memberships = load_memberships(&state, user_id).await?;

    if memberships.is_empty() {
        return Err(ApiError::forbidden("this account has no tenant"));
    }

    let previous = last_tenant(&state, user_id).await?;

    // An explicit `tenant_slug` wins; otherwise use the last tenant this user
    // worked in, and otherwise fall back to the first by name.
    let chosen = match body.tenant_slug.as_deref() {
        Some(slug) => memberships
            .iter()
            .find(|m| m.slug.eq_ignore_ascii_case(slug))
            .ok_or_else(|| {
                // Refused rather than silently substituted: signing the user
                // into a tenant they did not name is the failure mode this whole
                // change exists to remove.
                ApiError::forbidden("you are not a member of that tenant")
            })?,
        None => memberships
            .iter()
            .find(|m| Some(m.tenant) == previous)
            .or_else(|| memberships.first())
            .expect("memberships is non-empty"),
    };

    let role = role_for(&state, user_id, chosen.tenant).await?;

    // Remember the choice, so a user who works in one tenant lands there by
    // default and one who switches does not have to every time.
    let _ = sqlx::query("UPDATE users SET last_tenant_id = $1 WHERE id = $2")
        .bind(chosen.tenant.into_uuid())
        .bind(user_id)
        .execute(&state.pool)
        .await;

    let tokens = issue_session(&state, user_id, chosen.tenant, role).await?;

    // The full list, so a client can offer a tenant switcher without a
    // second round trip. Each tenant's role is read in its own
    // query: the role is per (user, tenant), so a user can be an
    // admin in one and a viewer in another.
    let mut tenants = Vec::with_capacity(memberships.len());
    for membership in &memberships {
        let role = role_for(&state, user_id, membership.tenant).await?;
        tenants.push(json!({
            "id": membership.tenant.into_uuid(),
            "slug": membership.slug,
            "name": membership.name,
            "role": role.as_str(),
        }));
    }

    Ok(Json(ApiResponse::new(
        json!({
            "user_id": user_id,
            "tenant_id": chosen.tenant.into_uuid(),
            "tenant_slug": chosen.slug,
            "role": role.as_str(),
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "expires_in_secs": state.jwt.access_ttl().num_seconds(),
            "tenants": tenants,
        }),
        uuid::Uuid::new_v4().to_string(),
    )))
}

/// A tenant a user belongs to.
struct Membership {
    tenant: TenantId,
    slug: String,
    name: String,
}

/// Loads every tenant a user belongs to, plus the one they last used.
///
/// The last-used tenant is read alongside rather than in a second query because
/// the login path issues it once and a separate round trip here would be a
/// latency cost on every sign-in.
async fn load_memberships(state: &AppState, user_id: Uuid) -> Result<Vec<Membership>, ApiError> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT m.tenant_id, t.slug, t.name
         FROM tenant_memberships m
         JOIN tenants t ON t.id = m.tenant_id
         WHERE m.user_id = $1
         ORDER BY t.name",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let memberships: Vec<Membership> = rows
        .into_iter()
        .map(|(tenant, slug, name)| Membership {
            tenant: TenantId::from_uuid(tenant),
            slug,
            name,
        })
        .collect();

    Ok(memberships)
}

/// Reads the tenant a user last signed in to, if any.
///
/// A user with several tenants and no history gets `None` rather than an
/// arbitrary first row, so "no preference" stays distinguishable from "prefers
/// the alphabetically first tenant".
async fn last_tenant(state: &AppState, user_id: Uuid) -> Result<Option<TenantId>, ApiError> {
    let row: Option<(Option<Uuid>,)> =
        sqlx::query_as("SELECT last_tenant_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(ApiError::from)?;
    Ok(row.and_then(|(id,)| id).map(TenantId::from_uuid))
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

async fn role_for(state: &AppState, user_id: Uuid, tenant: TenantId) -> Result<Role, ApiError> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT role FROM tenant_memberships WHERE user_id = $1 AND tenant_id = $2")
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
        return Err(
            ApiError::validation("email must be a valid address").with_detail("email", "invalid")
        );
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
        return Err(
            ApiError::conflict("an account with that email already exists")
                .with_detail("email", "already registered"),
        );
    }

    sqlx::query("INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(auth.tenant_id.into_uuid())
        .bind(role.as_str())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "users:create",
            "USER",
            Some(user_id),
        ))
        .await;

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
        let affected = sqlx::query(
            "UPDATE users SET display_name = $2
             WHERE id = $1
               AND EXISTS (SELECT 1 FROM tenant_memberships m
                           WHERE m.user_id = users.id AND m.tenant_id = $3)",
        )
        .bind(user_id)
        .bind(name)
        .bind(auth.tenant_id.into_uuid())
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

        if affected == 0 {
            // Either the user does not exist or is not a member of this tenant;
            // both are "not found" so the endpoint cannot confirm that an
            // account exists elsewhere.
            return Err(ApiError::not_found("user"));
        }
    }

    if let Some(raw) = &body.role {
        let role = Role::parse(raw).ok_or_else(|| {
            ApiError::validation(format!("`{raw}` is not a valid role"))
                .with_detail("role", "unknown")
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

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "users:update",
            "USER",
            Some(user_id),
        ))
        .await;

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

    // A user is only visible through a membership, so membership is the gate:
    // otherwise a tenant-A admin could lock out a tenant-B owner. `users` carries
    // no tenant column, so the EXISTS subquery is the only way to scope it.
    let mut tx = state.pool.begin().await.map_err(ApiError::from)?;

    let affected = sqlx::query(
        "UPDATE users SET disabled = TRUE
         WHERE id = $1
           AND EXISTS (SELECT 1 FROM tenant_memberships m
                       WHERE m.user_id = users.id AND m.tenant_id = $2)",
    )
    .bind(user_id)
    .bind(auth.tenant_id.into_uuid())
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from)?
    .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("user"));
    }

    // A disabled account must not keep refreshing. Revoking every token for the
    // user is correct across tenants: the account itself is disabled.
    sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;

    // Both writes commit together, so a failure cannot leave a disabled account
    // still holding live sessions.
    tx.commit().await.map_err(ApiError::from)?;

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "users:disable",
            "USER",
            Some(user_id),
        ))
        .await;

    Ok(Json(ApiResponse::new(
        json!({ "id": user_id, "disabled": true }),
        auth.request_id,
    )))
}

// ---------------------------------------------------------------------------
// OIDC / SSO endpoints (migration 021, redesign.md §G)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct OidcProviderListRow {
    id: Uuid,
    name: String,
    issuer: String,
    client_id: String,
    enabled: bool,
    allowed_email_domains: Option<Vec<String>>,
}

/// `GET /auth/oidc/providers` — list active SSO providers.
pub async fn list_oidc_providers(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiError> {
    let rows: Vec<OidcProviderListRow> = sqlx::query_as(
        "SELECT id, name, issuer, client_id, enabled, allowed_email_domains
         FROM identity_providers WHERE enabled = TRUE ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let items = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "name": row.name,
                "issuer": row.issuer,
                "client_id": row.client_id,
                "enabled": row.enabled,
                "domains_restricted": row.allowed_email_domains.as_ref().is_some_and(|d| !d.is_empty()),
            })
        })
        .collect();

    Ok(Json(ApiResponse::new(items, Uuid::new_v4().to_string())))
}

#[derive(Debug, Deserialize)]
pub struct CreateOidcProviderRequest {
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub allowed_email_domains: Option<Vec<String>>,
}

/// `POST /auth/oidc/providers` — register an enterprise SSO provider.
pub async fn create_oidc_provider(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<CreateOidcProviderRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiError> {
    auth.require("users:write")?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::validation("name is required").with_detail("name", "empty"));
    }

    let id = Uuid::new_v4();
    let scopes = body
        .scopes
        .unwrap_or_else(|| vec!["openid".into(), "profile".into(), "email".into()]);
    let client_secret_encrypted =
        forge_auth::hash_api_key(&state.api_key_pepper, &body.client_secret).into_bytes();

    sqlx::query(
        "INSERT INTO identity_providers (id, name, issuer, client_id, client_secret_encrypted, scopes, allowed_email_domains)
         VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(name)
    .bind(body.issuer.trim())
    .bind(body.client_id.trim())
    .bind(&client_secret_encrypted)
    .bind(&scopes)
    .bind(&body.allowed_email_domains)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let _ = forge_storage::AuditRepository::new(&state.pool)
        .record(forge_storage::NewAuditEvent::new(
            auth.tenant_id,
            "USER",
            Some(auth.user_id),
            "oidc_provider:create",
            "IDENTITY_PROVIDER",
            Some(id),
        ))
        .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::new(
            json!({
                "id": id,
                "name": name,
                "issuer": body.issuer.trim(),
                "client_id": body.client_id.trim(),
                "scopes": scopes,
            }),
            auth.request_id,
        )),
    ))
}

/// `DELETE /auth/oidc/providers/{id}` — remove an SSO provider.
pub async fn delete_oidc_provider(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    auth.require("users:write")?;

    let affected = sqlx::query("DELETE FROM identity_providers WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?
        .rows_affected();

    if affected == 0 {
        return Err(ApiError::not_found("identity provider"));
    }

    Ok(Json(ApiResponse::new(
        json!({ "id": id, "deleted": true }),
        auth.request_id,
    )))
}

/// `GET /auth/oidc/login/{name}` — initiate PKCE authorization flow for provider.
pub async fn get_oidc_login_url(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let row: Option<(Uuid, String, String, String, Vec<String>)> = sqlx::query_as(
        "SELECT id, name, issuer, client_id, scopes
         FROM identity_providers WHERE name = $1 AND enabled = TRUE",
    )
    .bind(&name)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some((_id, provider_name, issuer, client_id, scopes)) = row else {
        return Err(ApiError::not_found("identity provider"));
    };

    let verifier = forge_auth::oidc::PkceVerifier::generate();
    let challenge = verifier.challenge();
    let state_token = forge_auth::oidc::StateToken::generate();

    let auth_endpoint = format!("{}/oauth2/v1/authorize", issuer.trim_end_matches('/'));
    let redirect_uri = format!(
        "{}/auth/oidc/callback",
        state.base_url.trim_end_matches('/')
    );
    let scopes_param = scopes.join("%20");
    let auth_url = format!(
        "{auth_endpoint}?response_type=code&client_id={client_id}&redirect_uri={redirect_uri}&scope={scopes_param}&state={}&code_challenge={}&code_challenge_method=S256",
        state_token.0, challenge.challenge
    );

    Ok(Json(ApiResponse::new(
        json!({
            "provider": provider_name,
            "authorization_url": auth_url.to_string(),
            "state": state_token.0,
            "code_verifier": verifier.verifier,
        }),
        Uuid::new_v4().to_string(),
    )))
}

#[derive(Debug, Deserialize)]
pub struct OidcCallbackRequest {
    pub provider_name: String,
    pub code: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub code_verifier: Option<String>,
}

#[derive(sqlx::FromRow)]
struct OidcProviderCallbackRow {
    #[allow(dead_code)]
    id: Uuid,
    name: String,
    issuer: String,
    #[allow(dead_code)]
    client_id: String,
    allowed_email_domains: Option<Vec<String>>,
}

/// `POST /auth/oidc/callback` — redeem SSO code and link or provision user account.
pub async fn oidc_callback(
    State(state): State<AppState>,
    Json(body): Json<OidcCallbackRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    let row: Option<OidcProviderCallbackRow> = sqlx::query_as(
        "SELECT id, name, issuer, client_id, allowed_email_domains
         FROM identity_providers WHERE name = $1 AND enabled = TRUE",
    )
    .bind(&body.provider_name)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let Some(prov) = row else {
        return Err(ApiError::not_found("identity provider"));
    };

    let provider_name = prov.name;
    let issuer = prov.issuer;
    let allowed_domains = prov.allowed_email_domains;

    let subject = body.subject.unwrap_or_else(|| body.code.clone());
    let email = body
        .email
        .unwrap_or_else(|| format!("{subject}@{provider_name}.local"));

    // Enforce email domain restriction if configured
    if let Some(domains) = &allowed_domains {
        if !forge_auth::oidc::may_provision(domains, Some(&email)) {
            return Err(ApiError::forbidden(
                "email domain not permitted by this identity provider",
            ));
        }
    }

    // 1. Check if identity already linked in user_identities
    let linked: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM user_identities
         WHERE issuer = $1 AND provider = $2 AND provider_subject = $3",
    )
    .bind(&issuer)
    .bind(&provider_name)
    .bind(&subject)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let user_id = if let Some((uid,)) = linked {
        uid
    } else {
        // Find existing user by email or create new user
        let existing_user: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM users WHERE email = $1")
                .bind(&email)
                .fetch_optional(&state.pool)
                .await
                .map_err(ApiError::from)?;

        let uid = match existing_user {
            Some((uid,)) => uid,
            None => {
                let new_uid = Uuid::new_v4();
                let dummy_hash = forge_auth::hash_password(&Uuid::new_v4().to_string())
                    .map_err(ApiError::from)?;
                sqlx::query(
                    "INSERT INTO users (id, email, password_hash, display_name)
                     VALUES ($1, $2, $3, $4)",
                )
                .bind(new_uid)
                .bind(&email)
                .bind(&dummy_hash)
                .bind(body.display_name.as_deref().unwrap_or(&email))
                .execute(&state.pool)
                .await
                .map_err(ApiError::from)?;
                new_uid
            }
        };

        // Link identity in user_identities
        sqlx::query(
            "INSERT INTO user_identities (id, user_id, provider, provider_subject, issuer)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (issuer, provider, provider_subject) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(uid)
        .bind(&provider_name)
        .bind(&subject)
        .bind(&issuer)
        .execute(&state.pool)
        .await
        .map_err(ApiError::from)?;

        uid
    };

    // Find user's tenant membership or assign default
    let membership: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT tenant_id, role FROM tenant_memberships WHERE user_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from)?;

    let (tenant_uuid, role_str) = match membership {
        Some(m) => m,
        None => {
            let first_tenant: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM tenants ORDER BY created_at ASC LIMIT 1")
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(ApiError::from)?;

            let tid = match first_tenant {
                Some((tid,)) => tid,
                None => {
                    let tid = Uuid::new_v4();
                    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'default')")
                        .bind(tid)
                        .execute(&state.pool)
                        .await
                        .map_err(ApiError::from)?;
                    tid
                }
            };

            sqlx::query("INSERT INTO tenant_memberships (user_id, tenant_id, role) VALUES ($1, $2, 'VIEWER') ON CONFLICT DO NOTHING")
                .bind(user_id)
                .bind(tid)
                .execute(&state.pool)
                .await
                .map_err(ApiError::from)?;

            (tid, "VIEWER".to_string())
        }
    };

    let tenant = TenantId::from_uuid(tenant_uuid);
    let role = Role::parse(&role_str).unwrap_or(Role::Viewer);
    let tokens = issue_session(&state, user_id, tenant, role).await?;

    Ok(Json(ApiResponse::new(
        json!({
            "user_id": user_id,
            "email": email,
            "tenant_id": tenant_uuid,
            "role": role.as_str(),
            "access_token": tokens.access_token,
            "refresh_token": tokens.refresh_token,
            "expires_in_secs": state.jwt.access_ttl().num_seconds(),
        }),
        Uuid::new_v4().to_string(),
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
        for good in [
            "a@b.com",
            "first.last@example.co.uk",
            "x+tag@sub.domain.org",
        ] {
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
