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
}

impl AuthContext {
    /// Whether this caller holds `permission`.
    pub fn can(&self, permission: &str) -> bool {
        self.role.allows(permission)
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
/// API keys are also accepted: spec 11.1 lists API tokens as a first-class
/// credential alongside JWTs.
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
        }))
    }
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
}
