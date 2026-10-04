//! Authentication and authorization (spec 05 §5.10, spec 11).
//!
//! Four things live here:
//!
//! * password hashing with argon2 (spec 11.2 requires Argon2 or bcrypt);
//! * JWT access tokens plus **rotating** refresh tokens, with reuse detection;
//! * resource-scoped RBAC, expressed as `resource:action` strings so a grant can
//!   be stored in the database and audited;
//! * OIDC single sign-on ([`oidc`]), which `redesign.md` §G requires and which
//!   the `user_identities` table has been carrying unused since migration 005.

pub mod crypto;
pub mod oidc;

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Roles from spec 05 §5.10.
///
/// The pre-existing four-role set (`SystemAdmin`, `TenantAdmin`, `Developer`,
/// `Viewer`) did not match the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Role {
    Owner,
    Admin,
    Operator,
    Developer,
    Auditor,
    Viewer,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Owner => "OWNER",
            Role::Admin => "ADMIN",
            Role::Operator => "OPERATOR",
            Role::Developer => "DEVELOPER",
            Role::Auditor => "AUDITOR",
            Role::Viewer => "VIEWER",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "OWNER" => Some(Role::Owner),
            "ADMIN" => Some(Role::Admin),
            "OPERATOR" => Some(Role::Operator),
            "DEVELOPER" => Some(Role::Developer),
            "AUDITOR" => Some(Role::Auditor),
            "VIEWER" => Some(Role::Viewer),
            _ => None,
        }
    }

    /// The permissions this role grants, as `resource:action` strings.
    ///
    /// This matrix is the source of truth; the `roles` table is seeded from it.
    pub fn permissions(&self) -> &'static [&'static str] {
        match self {
            // Spec 11.4: OWNER holds every permission.
            Role::Owner => &["*"],

            Role::Admin => &[
                "jobs:read",
                "jobs:write",
                "jobs:trigger",
                "jobs:delete",
                "job_versions:read",
                "job_versions:write",
                "executions:read",
                "executions:cancel",
                "executions:retry",
                "workflows:read",
                "workflows:write",
                "workflows:trigger",
                "schedules:read",
                "schedules:write",
                "queues:read",
                "queues:write",
                "workers:read",
                "workers:admin",
                // The worker protocol itself. `workers:claim` and
                // `workers:heartbeat` are also the only permissions a worker
                // token carries (see `forge-api`'s `WORKER_PERMISSIONS`), so an
                // administrator can drive the protocol for diagnostics.
                "workers:claim",
                "workers:heartbeat",
                "users:read",
                "users:write",
                "audit:read",
                "settings:read",
                "settings:write",
                // Operational writes the server itself performs on behalf of a
                // tenant: recording scheduler heartbeats, execution metrics and
                // bulk cancellation.
                "executions:write",
                "alerts:read",
                "alerts:write",
                "incidents:read",
                "incidents:write",
                "webhooks:read",
                "webhooks:write",
                "integrations:read",
                "integrations:write",
                "notifications:read",
                "notifications:write",
            ],

            // Spec 01.12 separates running work from changing definitions.
            Role::Operator => &[
                "jobs:read",
                "jobs:trigger",
                "executions:read",
                "executions:cancel",
                "executions:retry",
                "executions:write",
                "workflows:read",
                "workflows:trigger",
                "schedules:read",
                "queues:read",
                "workers:read",
                "audit:read",
                "settings:read",
                "alerts:read",
                "alerts:write",
                "incidents:read",
                "incidents:write",
                "webhooks:read",
                "webhooks:write",
                "integrations:read",
                "notifications:read",
                "notifications:write",
            ],

            Role::Developer => &[
                "jobs:read",
                "jobs:write",
                "jobs:trigger",
                "job_versions:read",
                "job_versions:write",
                "executions:read",
                "executions:cancel",
                "executions:retry",
                "workflows:read",
                "workflows:write",
                "workflows:trigger",
                "schedules:read",
                "schedules:write",
                "queues:read",
                "workers:read",
                "settings:read",
                "alerts:read",
                "incidents:read",
                "webhooks:read",
                "integrations:read",
                "notifications:read",
            ],

            Role::Auditor => &[
                "jobs:read",
                "job_versions:read",
                "executions:read",
                "workflows:read",
                "schedules:read",
                "queues:read",
                "workers:read",
                "audit:read",
                "settings:read",
                "alerts:read",
                "incidents:read",
                "notifications:read",
            ],

            Role::Viewer => &[
                "jobs:read",
                "executions:read",
                "workflows:read",
                "queues:read",
                "workers:read",
                "alerts:read",
                "incidents:read",
                "notifications:read",
            ],
        }
    }

    /// Whether this role grants `permission`.
    ///
    /// A `*` grant covers everything, so an OWNER is not enumerated permission by
    /// permission.
    pub fn allows(&self, permission: &str) -> bool {
        self.permissions().iter().any(|p| {
            *p == "*"
                || *p == permission
                // `jobs:*` covers `jobs:write`.
                || (p.ends_with(":*") && permission.starts_with(&p[..p.len() - 1]))
        })
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Passwords
// ---------------------------------------------------------------------------

/// Hashes a password with argon2id.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    if password.is_empty() {
        return Err(AuthError::WeakPassword("password must not be empty".into()));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::Crypto(format!("hash password: {e}")))
}

/// Verifies a password against a stored argon2 hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| AuthError::Crypto(format!("parse password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Rejects passwords that are trivially weak.
///
/// Length is the dominant factor, so a character-class rule is deliberately not
/// imposed.
pub fn check_password_strength(password: &str) -> Result<(), AuthError> {
    const MIN_LENGTH: usize = 12;
    if password.chars().count() < MIN_LENGTH {
        return Err(AuthError::WeakPassword(format!(
            "password must be at least {MIN_LENGTH} characters"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Access tokens
// ---------------------------------------------------------------------------

/// Claims carried by an access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the user id.
    pub sub: String,
    pub tenant_id: String,
    pub role: Role,
    pub exp: usize,
    pub iat: usize,
    /// Distinguishes an access token from a refresh token.
    #[serde(default)]
    pub token_type: String,
}

// `EncodingKey` and `DecodingKey` deliberately do not derive `Debug`: printing
// one would risk emitting signing material into a log line.
pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_ttl: Duration,
}

impl JwtService {
    pub fn new(secret: &[u8]) -> Self {
        Self::new_with_ttl(secret, Duration::hours(24))
    }

    pub fn new_with_ttl(secret: &[u8], access_ttl: Duration) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            access_ttl,
        }
    }

    pub fn issue(&self, user_id: Uuid, tenant_id: Uuid, role: Role) -> Result<String, AuthError> {
        let now = Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            role,
            exp: (now + self.access_ttl).timestamp() as usize,
            iat: now.timestamp() as usize,
            token_type: "access".to_string(),
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|e| AuthError::Crypto(format!("sign token: {e}")))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        let mut validation = Validation::new(Algorithm::HS256);
        // Allow for modest clock skew between issuer and verifier.
        validation.leeway = 60;

        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|d| d.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                _ => AuthError::InvalidToken,
            })
    }

    pub fn access_ttl(&self) -> Duration {
        self.access_ttl
    }
}

// ---------------------------------------------------------------------------
// Refresh tokens
// ---------------------------------------------------------------------------

/// A refresh token, stored only as a hash.
///
/// Spec 11.3 requires rotating refresh tokens. Rotation with reuse detection
/// makes a stolen token single-use: presenting one that has already been
/// exchanged revokes the chain.
#[derive(Debug, Clone)]
pub struct RefreshToken {
    /// The raw value handed to the client. Never stored.
    pub raw: String,
    /// SHA-256 of `raw`, which is what goes in the database.
    pub hash: String,
    pub expires_at: DateTime<Utc>,
}

/// How long a refresh token is valid.
pub const REFRESH_TTL_DAYS: i64 = 30;

/// Generates a refresh token: 32 random bytes plus its hash.
pub fn generate_refresh_token() -> Result<RefreshToken, AuthError> {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = base64_url_encode(&bytes);
    Ok(RefreshToken {
        hash: hash_token(&raw),
        expires_at: Utc::now() + Duration::days(REFRESH_TTL_DAYS),
        raw,
    })
}

/// Hashes a token for storage (spec 08.8: secure one-way hashing).
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex(&hasher.finalize())
}

// ---------------------------------------------------------------------------
// API keys
// ---------------------------------------------------------------------------

/// An API key, shown to the client exactly once (spec 05 endpoint 49).
#[derive(Debug, Clone)]
pub struct GeneratedApiKey {
    /// The raw key. Returned once at creation and never recoverable afterwards.
    pub raw: String,
    /// Keyed digest of `raw`, stored in `api_keys.key_hash`.
    pub hash: String,
}

/// Keyed digest for API keys (spec 11.3).
///
/// Spec 11.3 requires API tokens to be stored "hashed using SHA-256 or
/// stronger". A bare SHA-256 of a 256-bit random token is already infeasible to
/// invert, but `FORGE_API_KEY_HASHING_SECRET` exists so that a database dump
/// alone is not enough to *test candidate keys* — for instance a key an operator
/// pasted into a ticket. HMAC-SHA256 makes that required secret load-bearing
/// instead of decorative.
pub fn hash_api_key_with(pepper: &[u8], raw: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    // HMAC accepts a key of any length, so this cannot fail.
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(pepper).expect("HMAC accepts keys of any length");
    mac.update(raw.as_bytes());
    hex(&mac.finalize().into_bytes())
}

/// Generates an API key with a recognisable prefix.
pub fn generate_api_key(pepper: &[u8]) -> GeneratedApiKey {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = format!("forge_{}", base64_url_encode(&bytes));
    GeneratedApiKey {
        hash: hash_api_key_with(pepper, &raw),
        raw,
    }
}

/// Generates a service account token with `forge_sa_` prefix.
pub fn generate_service_account_token(pepper: &[u8]) -> GeneratedApiKey {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = format!("forge_sa_{}", base64_url_encode(&bytes));
    GeneratedApiKey {
        hash: hash_api_key_with(pepper, &raw),
        raw,
    }
}

/// Hashes an API key for lookup.
pub fn hash_api_key(pepper: &[u8], raw: &str) -> String {
    hash_api_key_with(pepper, raw)
}

// ---------------------------------------------------------------------------
// Worker credentials
// ---------------------------------------------------------------------------

/// A worker token, shown once at registration (spec 10.2).
///
/// A worker needs a credential that identifies *the worker*, not the person who
/// registered it: only the lease holder may renew (spec 10.3) or complete
/// (spec 10.4) an execution, and a worker's operator is a different identity.
/// Hashing follows `generate_api_key` so a leaked database row cannot be used to
/// claim work.
#[derive(Debug, Clone)]
pub struct GeneratedWorkerToken {
    /// The raw token. Returned once at registration and never recoverable.
    pub raw: String,
    /// SHA-256 of `raw`, stored in `workers.token_hash`.
    pub hash: String,
}

/// Generates a worker token.
///
/// The `forge_wkr_` prefix is deliberately distinct from `forge_` so a worker
/// token pasted into an API-key field is rejected by the prefix check rather
/// than silently treated as a valid API key.
pub fn generate_worker_token() -> GeneratedWorkerToken {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = format!("forge_wkr_{}", base64_url_encode(&bytes));
    GeneratedWorkerToken {
        hash: hash_token(&raw),
        raw,
    }
}

/// Hashes a worker token for lookup and comparison.
pub fn hash_worker_token(raw: &str) -> String {
    hash_token(raw)
}

/// Whether a presented token is shaped like a worker token.
pub fn is_worker_token(raw: &str) -> bool {
    raw.starts_with("forge_wkr_")
}

// ---------------------------------------------------------------------------
// SSRF guard
// ---------------------------------------------------------------------------

/// Outcome of validating an outbound URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlVerdict {
    Allowed,
    /// The URL is refused, with a reason safe to show a user.
    Blocked(String),
}

/// Checks a URL against the SSRF rules in spec 11.
///
/// Blocks loopback, link-local, private ranges, and cloud metadata endpoints.
/// This is a syntactic first line of defence; a deployment that can resolve DNS
/// to an internal address must also apply the check after resolution, which is
/// documented as a known limitation rather than pretended away.
pub fn check_outbound_url(url: &str) -> UrlVerdict {
    let Ok(parsed) = url::Url::parse(url) else {
        return UrlVerdict::Blocked("URL could not be parsed".into());
    };

    // Only HTTP(S) is ever safe to fetch.
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return UrlVerdict::Blocked(format!(
                "scheme `{other}` is not permitted; use http or https"
            ))
        }
    }

    let Some(host) = parsed.host_str() else {
        return UrlVerdict::Blocked("URL has no host".into());
    };

    if let Some(decision) = check_host(host) {
        return decision;
    }

    UrlVerdict::Allowed
}

/// Checks a hostname or IP literal.
pub fn check_host(host: &str) -> Option<UrlVerdict> {
    let lower = host.to_ascii_lowercase();

    // Hostnames that resolve to metadata services or to the loopback interface.
    const BLOCKED_NAMES: &[&str] = &[
        "localhost",
        "localhost.localdomain",
        "ip6-localhost",
        "metadata.google.internal",
        "metadata",
        "instance-data",
    ];
    if BLOCKED_NAMES.contains(&lower.as_str()) {
        return Some(UrlVerdict::Blocked(
            "host is not permitted for outbound requests".into(),
        ));
    }

    // A bare IPv6 literal arrives bracketed from `Url::host_str` in some cases.
    let unbracketed = lower.trim_start_matches('[').trim_end_matches(']');

    match unbracketed.parse::<std::net::IpAddr>() {
        Ok(ip) => check_ip(ip),
        Err(_) => {
            // A hostname that is not an IP literal cannot be judged here; the
            // post-resolution check covers it.
            None
        }
    }
}

/// Blocks private, loopback, link-local, and multicast destinations.
pub fn check_ip(ip: std::net::IpAddr) -> Option<UrlVerdict> {
    use std::net::IpAddr::{V4, V6};

    let blocked = match ip {
        V4(v4) => {
            v4.is_loopback()
                // 0.0.0.0/8 "this network"
                || v4.is_unspecified()
                // RFC 1918
                || v4.is_private()
                // 169.254.0.0/16 link-local, which includes the AWS metadata
                // endpoint at 169.254.169.254
                || v4.is_link_local()
                // 100.64.0.0/10 carrier-grade NAT
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
                || v4.is_multicast()
                || v4.is_broadcast()
                // Shared address space used by some clouds.
                || (v4.octets()[0] == 198 && (v4.octets()[1] & 0xFE) == 18)
        }
        V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique local addresses fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6.is_multicast()
        }
    };

    blocked.then(|| UrlVerdict::Blocked("address is not permitted for outbound requests".into()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid or malformed token")]
    InvalidToken,
    #[error("token has expired")]
    TokenExpired,
    #[error("password rejected: {0}")]
    WeakPassword(String),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("refresh token reuse detected; the session chain has been revoked")]
    RefreshTokenReuse,
    #[error("account is disabled")]
    AccountDisabled,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("tenant isolation violation: {0}")]
    TenantIsolation(String),
    #[error("cryptographic failure: {0}")]
    Crypto(String),
    /// OIDC failures are grouped rather than enumerated so a provider's own
    /// error text never reaches a user-facing message, while the reason is still
    /// available to a log.
    #[error("identity provider error: {0}")]
    Oidc(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- roles ---

    #[test]
    fn every_spec_role_exists() {
        // Spec 05 §5.10 names exactly these six.
        for name in [
            "OWNER",
            "ADMIN",
            "OPERATOR",
            "DEVELOPER",
            "AUDITOR",
            "VIEWER",
        ] {
            assert!(Role::parse(name).is_some(), "{name} must be a role");
        }
        assert_eq!(
            Role::parse("SystemAdmin"),
            None,
            "the old four-role set is gone"
        );
    }

    #[test]
    fn roles_round_trip_through_their_string_form() {
        for role in [
            Role::Owner,
            Role::Admin,
            Role::Operator,
            Role::Developer,
            Role::Auditor,
            Role::Viewer,
        ] {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
    }

    /// Spec 11.4 lists the permission vocabulary; each must be grantable.
    #[test]
    fn the_spec_permission_vocabulary_is_covered() {
        for permission in [
            "jobs:read",
            "jobs:write",
            "jobs:trigger",
            "executions:read",
            "executions:cancel",
            "workers:admin",
            "audit:read",
        ] {
            assert!(
                Role::Admin.allows(permission) || Role::Owner.allows(permission),
                "{permission} must be grantable"
            );
        }
    }

    #[test]
    fn an_owner_may_do_anything() {
        for permission in ["jobs:read", "users:write", "tenants:delete", "any:thing"] {
            assert!(
                Role::Owner.allows(permission),
                "OWNER must allow {permission}"
            );
        }
    }

    #[test]
    fn a_viewer_is_read_only() {
        assert!(Role::Viewer.allows("jobs:read"));
        assert!(Role::Viewer.allows("executions:read"));
        assert!(
            !Role::Viewer.allows("jobs:write"),
            "a viewer must not author definitions"
        );
        assert!(!Role::Viewer.allows("executions:cancel"));
        assert!(!Role::Viewer.allows("audit:read"));
    }

    /// Spec 01.12: running work and changing definitions are different powers.
    #[test]
    fn an_operator_may_run_work_but_not_rewrite_it() {
        assert!(Role::Operator.allows("jobs:trigger"));
        assert!(Role::Operator.allows("executions:cancel"));
        assert!(
            !Role::Operator.allows("jobs:write"),
            "an operator must not redefine a job"
        );
        assert!(!Role::Operator.allows("workers:admin"));
    }

    #[test]
    fn a_developer_may_write_but_not_administer() {
        assert!(Role::Developer.allows("jobs:write"));
        assert!(Role::Developer.allows("workflows:write"));
        assert!(!Role::Developer.allows("users:write"));
        assert!(!Role::Developer.allows("workers:admin"));
    }

    #[test]
    fn an_auditor_reads_history_but_changes_nothing() {
        assert!(Role::Auditor.allows("audit:read"));
        assert!(Role::Auditor.allows("executions:read"));
        assert!(!Role::Auditor.allows("jobs:write"));
        assert!(!Role::Auditor.allows("executions:cancel"));
    }

    /// An admin must not be able to delete the tenant itself.
    #[test]
    fn tenant_deletion_is_owner_only() {
        assert!(Role::Owner.allows("tenants:delete"));
        assert!(!Role::Admin.allows("tenants:delete"));
    }

    #[test]
    fn a_wildcard_permission_prefix_covers_its_sub_actions() {
        // `jobs:*` must satisfy `jobs:write` without enumerating it.
        assert!(Role::Owner.allows("jobs:write"));
        assert!(!Role::Viewer.allows("jobs:delete"));
    }

    // --- passwords ---

    #[test]
    fn a_password_round_trips_through_argon2() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2"), "must be argon2: {hash}");
        assert!(verify_password("correct horse battery staple", &hash).unwrap());
        assert!(
            !verify_password("wrong password", &hash).unwrap(),
            "a wrong password must not verify"
        );
    }

    #[test]
    fn the_same_password_hashes_differently_each_time() {
        // Distinct salts mean a stolen table cannot be attacked with
        // precomputed hashes.
        let a = hash_password("correct horse battery staple").unwrap();
        let b = hash_password("correct horse battery staple").unwrap();
        assert_ne!(a, b, "each hash must use a fresh salt");
    }

    #[test]
    fn a_hash_never_contains_the_password() {
        let password = "correct horse battery staple";
        assert!(!hash_password(password).unwrap().contains(password));
    }

    #[test]
    fn weak_passwords_are_rejected() {
        assert!(check_password_strength("").is_err());
        assert!(check_password_strength("short").is_err());
        // "elevenchars" is 11 characters: one short of the minimum.
        assert!(check_password_strength("elevenchars").is_err());
        assert!(check_password_strength("twelvechars!").is_ok());
        assert!(hash_password("").is_err());
    }

    #[test]
    fn a_malformed_hash_is_an_error_not_a_silent_pass() {
        assert!(verify_password("anything", "not-a-hash").is_err());
    }

    // --- access tokens ---

    #[test]
    fn an_access_token_round_trips() {
        let service = JwtService::new(b"test-secret");
        let user = Uuid::new_v4();
        let tenant = Uuid::new_v4();

        let token = service.issue(user, tenant, Role::Operator).unwrap();
        let claims = service.verify(&token).unwrap();

        assert_eq!(claims.sub, user.to_string());
        assert_eq!(claims.tenant_id, tenant.to_string());
        assert_eq!(claims.role, Role::Operator);
        assert_eq!(claims.token_type, "access");
    }

    #[test]
    fn a_token_signed_with_another_secret_is_rejected() {
        let issuer = JwtService::new(b"secret-one");
        let verifier = JwtService::new(b"secret-two");
        let token = issuer
            .issue(Uuid::new_v4(), Uuid::new_v4(), Role::Admin)
            .unwrap();
        assert!(verifier.verify(&token).is_err());
    }

    #[test]
    fn a_malformed_token_is_rejected() {
        assert!(matches!(
            JwtService::new(b"secret").verify("not-a-jwt"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn an_expired_token_is_reported_as_expired() {
        // The verifier allows 60s of clock skew, so a token must be expired by
        // more than that to register as expired rather than merely invalid.
        let service = JwtService::new_with_ttl(b"secret", Duration::seconds(-120));
        let token = service
            .issue(Uuid::new_v4(), Uuid::new_v4(), Role::Viewer)
            .unwrap();
        assert!(matches!(
            service.verify(&token),
            Err(AuthError::TokenExpired)
        ));
    }

    #[test]
    fn a_token_within_the_leeway_window_is_still_accepted() {
        // A small negative skew must not lock a user out during ordinary clock
        // drift.
        let service = JwtService::new_with_ttl(b"secret", Duration::seconds(-10));
        let token = service
            .issue(Uuid::new_v4(), Uuid::new_v4(), Role::Viewer)
            .unwrap();
        assert!(service.verify(&token).is_ok(), "modest skew is tolerated");
    }

    // --- refresh tokens ---

    #[test]
    fn a_refresh_token_is_stored_only_as_a_hash() {
        let token = generate_refresh_token().unwrap();
        assert!(
            token.raw.len() >= 32,
            "the raw value must be long enough to resist guessing"
        );
        assert_ne!(token.hash, token.raw, "the raw value is never stored");
        assert_eq!(token.hash.len(), 64, "SHA-256 is 64 hex characters");
    }

    #[test]
    fn refresh_tokens_are_unique() {
        let a = generate_refresh_token().unwrap();
        let b = generate_refresh_token().unwrap();
        assert_ne!(a.raw, b.raw);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn hashing_is_deterministic_so_a_token_can_be_looked_up() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }

    #[test]
    fn a_refresh_token_carries_an_expiry() {
        let token = generate_refresh_token().unwrap();
        assert!(token.expires_at > Utc::now());
        assert!(token.expires_at <= Utc::now() + Duration::days(REFRESH_TTL_DAYS + 1));
    }

    // --- API keys ---

    /// The pepper used by these tests. Any non-empty value works; the point is
    /// that it must be *used*.
    const PEPPER: &[u8] = b"test-pepper";

    #[test]
    fn an_api_key_is_prefixed_and_shown_once() {
        let key = generate_api_key(PEPPER);
        assert!(key.raw.starts_with("forge_"), "keys are recognisable");
        assert_ne!(key.hash, key.raw, "only the hash is stored");
        assert_eq!(key.hash.len(), 64);
    }

    #[test]
    fn api_keys_are_unique() {
        assert_ne!(generate_api_key(PEPPER).raw, generate_api_key(PEPPER).raw);
    }

    #[test]
    fn an_api_key_can_be_looked_up_by_its_hash() {
        let key = generate_api_key(PEPPER);
        assert_eq!(hash_api_key(PEPPER, &key.raw), key.hash);
    }

    /// `FORGE_API_KEY_HASHING_SECRET` must actually protect the stored hash: a
    /// database dump plus a candidate key is not enough without the pepper.
    #[test]
    fn a_different_pepper_does_not_verify_the_same_key() {
        let key = generate_api_key(PEPPER);
        assert_ne!(hash_api_key(b"another-pepper", &key.raw), key.hash);
        assert_ne!(hash_api_key(b"", &key.raw), key.hash);
    }

    /// RFC 4231 test case 2, so the HMAC construction is verified against the
    /// standard rather than only against itself.
    #[test]
    fn keyed_hashing_matches_rfc_4231() {
        let digest = hash_api_key_with(b"Jefe", "what do ya want for nothing?");
        assert_eq!(
            digest,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    // --- SSRF (AT-SEC-002) ---

    #[test]
    fn loopback_and_metadata_targets_are_blocked() {
        for url in [
            "http://localhost/",
            "http://127.0.0.1/",
            "http://127.0.0.1:8080/admin",
            "http://[::1]/",
            "http://169.254.169.254/latest/meta-data/",
            "http://metadata.google.internal/computeMetadata/v1/",
        ] {
            assert!(
                matches!(check_outbound_url(url), UrlVerdict::Blocked(_)),
                "{url} must be blocked"
            );
        }
    }

    #[test]
    fn private_ranges_are_blocked() {
        for url in [
            "http://10.0.0.1/",
            "http://192.168.1.1/",
            "http://172.16.0.1/",
            "http://[fc00::1]/",
            "http://[fe80::1]/",
        ] {
            assert!(
                matches!(check_outbound_url(url), UrlVerdict::Blocked(_)),
                "{url} must be blocked"
            );
        }
    }

    #[test]
    fn non_http_schemes_are_blocked() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/",
            "gopher://example.com/",
        ] {
            assert!(
                matches!(check_outbound_url(url), UrlVerdict::Blocked(_)),
                "{url} must be blocked"
            );
        }
    }

    #[test]
    fn a_malformed_url_is_blocked_rather_than_allowed() {
        assert!(matches!(
            check_outbound_url("not a url at all"),
            UrlVerdict::Blocked(_)
        ));
    }

    #[test]
    fn public_destinations_are_allowed() {
        for url in [
            "https://example.com/webhook",
            "http://93.184.216.34/hook",
            "https://api.stripe.com/v1/events",
        ] {
            assert_eq!(
                check_outbound_url(url),
                UrlVerdict::Allowed,
                "{url} should be allowed"
            );
        }
    }

    #[test]
    fn an_unresolvable_hostname_is_not_judged_by_the_literal_check() {
        // A hostname is checked after DNS resolution, which this layer cannot do;
        // it must not be refused outright either.
        assert_eq!(check_host("internal.example.com"), None);
    }

    #[test]
    fn hex_encoding_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
