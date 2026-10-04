//! Authentication and authorization extractors (spec 11.4, spec 11.5).
//!
//! Every tenant-scoped route takes [`AuthContext`] rather than reading claims
//! ad hoc, so authorization cannot be forgotten at a call site: the handler
//! physically cannot run without an identity, and the tenant is carried from
//! the token rather than the request body.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use forge_auth::{Claims, Role};
use forge_domain::TenantId;
use uuid::Uuid;

use crate::envelope::ApiError;
use crate::router::AppState;

/// The authenticated caller.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub tenant_id: TenantId,
    pub role: Role,
    pub request_id: String,
    /// Set when the caller authenticated with a worker token (`forge_wkr_`).
    ///
    /// Spec 11.1 says workers are not implicitly trusted, so a worker is a
    /// *narrower* principal than any human role: it may only drive the worker
    /// protocol, and never administer the tenant it belongs to. Without this
    /// flag a worker credential is indistinguishable from an ADMIN, which would
    /// let any compromised worker mint users, revoke other workers and read the
    /// audit trail.
    pub worker_id: Option<Uuid>,
}

/// The only permissions a worker token can ever satisfy.
///
/// Kept as an explicit allow-list rather than a role so that adding a
/// permission to a role cannot silently widen worker authority.
pub const WORKER_PERMISSIONS: &[&str] = &["workers:claim", "workers:heartbeat", "executions:write"];

impl AuthContext {
    /// Whether this caller holds `permission`.
    pub fn can(&self, permission: &str) -> bool {
        match self.worker_id {
            Some(_) => WORKER_PERMISSIONS.contains(&permission),
            None => self.role.allows(permission),
        }
    }

    /// Refuses the request unless the caller holds `permission`.
    pub fn require(&self, permission: &str) -> Result<(), ApiError> {
        if self.can(permission) {
            Ok(())
        } else {
            Err(ApiError::forbidden(format!(
                "role {} does not grant `{permission}`",
                self.role
            )))
        }
    }

    /// Refuses the request unless the caller may administer the tenant.
    pub fn require_admin(&self) -> Result<(), ApiError> {
        self.require("users:write")
    }
}

/// Extracts the caller from the bearer token.
///
/// Two credentials are accepted, because they answer different questions:
///
/// - A **JWT** identifies a person and their role. It is what the console sends.
/// - An **API key** (`forge_...`) identifies a tenant-scoped integration without a
///   human session. Spec 11.1 lists these as first-class credentials, so they are
///   verified here rather than only being generated and stored.
///
/// A `forge_wkr_` token is accepted, but only as a *worker* principal: it can
/// claim, heartbeat, complete and log work for its own tenant and nothing else
/// ([`WORKER_PERMISSIONS`]). It is deliberately not mapped to any human role, so
/// a worker cannot administer the tenant it works for.
pub struct AuthUser(pub AuthContext);

#[axum::async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let request_id = crate::extract_request_id(parts);

        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| ApiError::unauthenticated("missing Authorization header"))?;

        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or_else(|| {
                ApiError::unauthenticated("Authorization header must use the Bearer scheme")
            })?
            .trim();

        if token.is_empty() {
            return Err(ApiError::unauthenticated("empty bearer token"));
        }

        if forge_auth::is_worker_token(token) {
            return worker_token_context(state, token, request_id).await;
        }

        if token.starts_with("forge_") {
            return api_key_context(state, token, request_id).await;
        }

        let claims: Claims = state
            .jwt
            .verify(token)
            .map_err(|_| ApiError::unauthenticated("the access token is invalid or has expired"))?;

        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| ApiError::unauthenticated("the token subject is not a valid id"))?;
        let tenant_uuid = Uuid::parse_str(&claims.tenant_id)
            .map_err(|_| ApiError::unauthenticated("the token carries no valid tenant scope"))?;

        Ok(AuthUser(AuthContext {
            user_id,
            tenant_id: TenantId::from_uuid(tenant_uuid),
            role: claims.role,
            request_id,
            worker_id: None,
        }))
    }
}

/// Resolves an API key to an identity.
///
/// Spec 11.1 makes API keys a first-class credential, and the admin endpoints
/// already mint them, so a key that cannot be used is worse than no key at all:
/// it looks like a working integration credential and silently fails at runtime.
///
/// The stored `api_keys.tenant_id` is the authority for scope, never a tenant
/// named in the request, and a revoked or expired key is refused.
async fn api_key_context(
    state: &AppState,
    token: &str,
    request_id: String,
) -> Result<AuthUser, ApiError> {
    use forge_storage::ApiKeyRepository;

    let hash = forge_auth::hash_api_key(&state.api_key_pepper, token);
    let record = ApiKeyRepository::new(&state.pool)
        .find_by_hash(&hash)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::unauthenticated("the API key is not valid"))?;

    if record.revoked_at.is_some() {
        return Err(ApiError::unauthenticated("the API key has been revoked"));
    }
    if let Some(expires_at) = record.expires_at {
        if expires_at <= chrono::Utc::now() {
            return Err(ApiError::unauthenticated("the API key has expired"));
        }
    }

    // Record use so an operator can see which keys are still live. A failure
    // here must not fail the request the key just authorised.
    let _ = ApiKeyRepository::new(&state.pool).touch(record.id).await;

    // Fail closed. A key whose stored role cannot be parsed is a corrupted or
    // tampered row, not an opportunity to hand out ADMIN (spec 11.1).
    let role = forge_auth::Role::parse(&record.role).ok_or_else(|| {
        tracing::error!(
            api_key_id = %record.id,
            "api key has an unrecognised role; refusing the request"
        );
        ApiError::unauthenticated("the API key is misconfigured and cannot be used")
    })?;

    Ok(AuthUser(AuthContext {
        // An API key has no user behind it. `owner_id` records the human who
        // created it, which is what the audit log should attribute the action to.
        user_id: record.owner_id.unwrap_or(record.id),
        tenant_id: TenantId::from_uuid(record.tenant_id),
        // Scope comes from the key itself, never from a role in the request.
        role,
        request_id,
        worker_id: None,
    }))
}

/// Resolves a worker token to the worker identity.
async fn worker_token_context(
    state: &AppState,
    token: &str,
    request_id: String,
) -> Result<AuthUser, ApiError> {
    use forge_storage::WorkerRepository;

    let hash = forge_auth::hash_worker_token(token);
    let worker = WorkerRepository::new(&state.pool)
        .find_by_token_hash(&hash)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::unauthenticated("the worker token is not valid"))?;

    if worker.status == "REVOKED" {
        return Err(ApiError::unauthenticated("the worker has been revoked"));
    }

    Ok(AuthUser(AuthContext {
        user_id: worker.id,
        tenant_id: TenantId::from_uuid(worker.tenant_id),
        // A worker is deliberately not given a human role; `can` consults
        // `WORKER_PERMISSIONS` whenever `worker_id` is set. VIEWER is recorded
        // only so logs and error messages do not overstate its authority.
        role: Role::Viewer,
        request_id,
        worker_id: Some(worker.id),
    }))
}

/// Extractor form, so handlers can take `AuthContext` directly.
pub struct Auth(pub AuthContext);

#[axum::async_trait]
impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthUser(context) = AuthUser::from_request_parts(parts, state).await?;
        Ok(Auth(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(role: Role) -> AuthContext {
        AuthContext {
            user_id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            role,
            request_id: "req-1".into(),
            worker_id: None,
        }
    }

    fn worker(worker_id: Uuid) -> AuthContext {
        AuthContext {
            user_id: worker_id,
            tenant_id: TenantId::new(),
            role: Role::Viewer,
            request_id: "req-1".into(),
            worker_id: Some(worker_id),
        }
    }

    #[test]
    fn a_context_reports_its_role_permissions() {
        let admin = context(Role::Admin);
        assert!(admin.can("jobs:write"));
        assert!(!admin.can("tenants:delete"));
    }

    #[test]
    fn requiring_a_granted_permission_succeeds() {
        let developer = context(Role::Developer);
        assert!(developer.require("jobs:write").is_ok());
        assert!(developer.require("jobs:read").is_ok());
    }

    #[test]
    fn requiring_an_ungranted_permission_is_refused() {
        let viewer = context(Role::Viewer);
        let error = viewer.require("jobs:write").unwrap_err();
        assert_eq!(error.status, axum::http::StatusCode::FORBIDDEN);
        assert!(
            error.message.contains("jobs:write"),
            "the refusal names the missing permission: {}",
            error.message
        );
    }

    #[test]
    fn admin_requires_user_write() {
        assert!(context(Role::Admin).require_admin().is_ok());
        assert!(context(Role::Operator).require_admin().is_err());
        assert!(context(Role::Viewer).require_admin().is_err());
    }

    #[test]
    fn an_owner_is_allowed_everything() {
        let owner = context(Role::Owner);
        for permission in ["jobs:write", "users:write", "tenants:delete"] {
            assert!(owner.require(permission).is_ok());
        }
    }

    /// A worker credential is not a tenant administrator.
    ///
    /// Mapping `forge_wkr_` to `Role::Admin` meant any compromised or
    /// over-privileged worker could mint users, revoke other workers and read
    /// the audit trail across the whole tenant (spec 11.1: no implicit trust of
    /// workers, least privilege).
    #[test]
    fn a_worker_may_only_drive_the_worker_protocol() {
        let worker = worker(Uuid::new_v4());
        for permission in WORKER_PERMISSIONS {
            assert!(
                worker.require(permission).is_ok(),
                "a worker must be able to `{permission}`"
            );
        }
    }

    #[test]
    fn a_worker_is_refused_tenant_administration() {
        let worker = worker(Uuid::new_v4());
        for permission in [
            "users:write",
            "users:read",
            "workers:admin",
            "workers:read",
            "settings:write",
            "audit:read",
            "jobs:write",
            "jobs:delete",
            "api_keys:write",
            "tenants:delete",
        ] {
            assert!(
                worker.require(permission).is_err(),
                "a worker must never hold `{permission}`"
            );
        }
    }

    /// The allow-list must stay narrow: adding a claim to `WORKER_PERMISSIONS`
    /// should be a deliberate, reviewable act.
    #[test]
    fn the_worker_permission_list_stays_narrow() {
        assert_eq!(
            WORKER_PERMISSIONS.to_vec(),
            vec!["workers:claim", "workers:heartbeat", "executions:write"]
        );
    }
}
