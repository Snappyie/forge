use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    SystemAdmin,
    TenantAdmin,
    Developer,
    Viewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    // Jobs
    CreateJob,
    ReadJob,
    UpdateJob,
    DeleteJob,
    // Executions
    TriggerExecution,
    CancelExecution,
    ReadExecution,
    // System
    ManageWorkers,
    ManageUsers,
    ManageTenants,
}

impl Role {
    pub fn permissions(&self) -> Vec<Permission> {
        match self {
            Role::SystemAdmin => vec![
                Permission::CreateJob, Permission::ReadJob, Permission::UpdateJob, Permission::DeleteJob,
                Permission::TriggerExecution, Permission::CancelExecution, Permission::ReadExecution,
                Permission::ManageWorkers, Permission::ManageUsers, Permission::ManageTenants,
            ],
            Role::TenantAdmin => vec![
                Permission::CreateJob, Permission::ReadJob, Permission::UpdateJob, Permission::DeleteJob,
                Permission::TriggerExecution, Permission::CancelExecution, Permission::ReadExecution,
                Permission::ManageUsers, // Scoped to their tenant only
            ],
            Role::Developer => vec![
                Permission::CreateJob, Permission::ReadJob, Permission::UpdateJob,
                Permission::TriggerExecution, Permission::CancelExecution, Permission::ReadExecution,
            ],
            Role::Viewer => vec![
                Permission::ReadJob, Permission::ReadExecution,
            ],
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // User ID
    pub tenant_id: String,
    pub role: Role,
    pub exp: usize,
    pub iat: usize,
}

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtService {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn generate_token(&self, user_id: Uuid, tenant_id: Uuid, role: Role) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let exp = (now + Duration::hours(24)).timestamp() as usize; // 24 hour expiry
        
        let claims = Claims {
            sub: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            role,
            exp,
            iat: now.timestamp() as usize,
        };

        encode(&Header::default(), &claims, &self.encoding_key)
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::default();
        validation.leeway = 60; // 60 seconds leeway for clock skew
        
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_lifecycle() {
        let secret = "super_secret_key_for_testing";
        let service = JwtService::new(secret);
        
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        
        let token = service.generate_token(user_id, tenant_id, Role::TenantAdmin).unwrap();
        
        let claims = service.validate_token(&token).unwrap();
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.tenant_id, tenant_id.to_string());
        assert_eq!(claims.role, Role::TenantAdmin);
        assert!(claims.role.permissions().contains(&Permission::ManageUsers));
        assert!(!claims.role.permissions().contains(&Permission::ManageTenants)); // TenantAdmin cannot manage tenants globally
    }
}
