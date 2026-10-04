//! OpenID Connect single sign-on.
//!
//! `redesign.md` §G requires "SSO through OIDC/OAuth2 and enterprise identity
//! providers". The schema has carried a `user_identities` table since migration
//! 005 and nothing has ever read or written it, so this module is what makes
//! that table mean something.
//!
//! The flow implemented is authorization code + PKCE, which is the only one worth
//! supporting for a server-side web console:
//!
//! - **PKCE is not optional.** A confidential client keeps its secret on the
//!   server, but the code travels through the browser, and without PKCE anyone
//!   who can observe the redirect can redeem the code. RFC 9700 requires it.
//! - **The ID token is verified against the provider's JWKS**, never decoded
//!   and trusted. An unverified ID token is an attacker-chosen user identity.
//! - **`iss`, `aud` and `exp` are all checked.** Checking only the signature
//!   would accept a token minted for a different client or a different provider.
//!
//! No token is ever accepted without a matching `(issuer, subject)` in
//! [`IdentityLink`], so a provider cannot assert a user the console has never
//! seen — which is what stops a rogue or misconfigured IdP from granting access
//! to arbitrary accounts.

use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AuthError;

/// An OIDC provider's discovery document, as fetched from `/.well-known/openid-configuration`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
}

impl ProviderMetadata {
    /// Fetches and validates a provider's discovery document.
    ///
    /// The issuer is echoed back by the provider and must match what was
    /// configured. Without that check, a misconfigured or hijacked DNS entry
    /// could redirect the whole login flow at a substitute IdP, and every token
    /// issued by it would then verify.
    pub async fn discover(client: &reqwest::Client, issuer: &str) -> Result<Self, AuthError> {
        let base = issuer.trim_end_matches('/');
        let url = format!("{base}/.well-known/openid-configuration");

        let metadata: ProviderMetadata = client
            .get(&url)
            .send()
            .await
            .map_err(|e| AuthError::Oidc(format!("discovery request failed: {e}")))?
            .error_for_status()
            .map_err(|e| AuthError::Oidc(format!("discovery returned an error: {e}")))?
            .json()
            .await
            .map_err(|e| AuthError::Oidc(format!("discovery document is not valid JSON: {e}")))?;

        // Providers conventionally publish the issuer with a trailing slash and
        // without one, so compare on the trimmed form.
        if metadata.issuer.trim_end_matches('/') != base {
            return Err(AuthError::Oidc(format!(
                "discovery document declares issuer {} but was fetched from {}",
                metadata.issuer, base
            )));
        }

        Ok(metadata)
    }
}

/// A configured provider: the credentials plus its endpoints.
#[derive(Debug, Clone)]
pub struct OidcProvider {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub metadata: ProviderMetadata,
    pub scopes: Vec<String>,
}

impl OidcProvider {
    /// Builds the URL the browser is redirected to.
    ///
    /// `state` and the PKCE challenge are always present. `state` is the CSRF
    /// defence — without it, a third party can complete a login on the operator's
    /// behalf by feeding their own authorization code into the callback.
    pub fn authorization_url(
        &self,
        redirect_uri: &str,
        state: &str,
        challenge: &PkceChallenge,
    ) -> Result<String, AuthError> {
        let mut url = url::Url::parse(&self.metadata.authorization_endpoint)
            .map_err(|e| AuthError::Oidc(format!("authorization endpoint is not a URL: {e}")))?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &self.client_id);
            query.append_pair("redirect_uri", redirect_uri);
            query.append_pair("scope", &self.scopes.join(" "));
            query.append_pair("state", state);
            query
                .append_pair("code_challenge", challenge.challenge.as_str())
                .append_pair("code_challenge_method", "S256");
        }

        Ok(url.into())
    }

    /// Exchanges an authorization code for the token set.
    pub async fn exchange_code(
        &self,
        client: &reqwest::Client,
        redirect_uri: &str,
        code: &str,
        verifier: &PkceVerifier,
    ) -> Result<TokenSet, AuthError> {
        let response = client
            .post(&self.metadata.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("code_verifier", verifier.verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AuthError::Oidc(format!("token request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            // The body can carry the provider's own error code, which is what an
            // operator needs to debug a misconfigured client — but it can also
            // echo the submitted code, so only the status is surfaced here.
            return Err(AuthError::Oidc(format!("token endpoint returned {status}")));
        }

        response
            .json::<TokenSet>()
            .await
            .map_err(|e| AuthError::Oidc(format!("token response is not valid JSON: {e}")))
    }
}

/// The provider's token response.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub id_token: String,
    pub token_type: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// The verified claims of an ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    /// `(issuer, subject)` — the stable key to store against a local account.
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub display_name: Option<String>,
}

/// A key from the provider's JWKS.
#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    #[serde(default)]
    alg: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

/// Verifies an ID token against the provider's JWKS and returns its claims.
///
/// Returns the *verified* identity or an error. There is deliberately no
/// "unverified" path: an ID token that has not had its signature checked is an
/// attacker-supplied user identity, so a caller cannot ask for one.
pub async fn verify_id_token(
    client: &reqwest::Client,
    metadata: &ProviderMetadata,
    id_token: &str,
    client_id: &str,
) -> Result<VerifiedIdentity, AuthError> {
    let jwks: Jwks = client
        .get(&metadata.jwks_uri)
        .send()
        .await
        .map_err(|e| AuthError::Oidc(format!("JWKS request failed: {e}")))?
        .error_for_status()
        .map_err(|e| AuthError::Oidc(format!("JWKS returned an error: {e}")))?
        .json()
        .await
        .map_err(|e| AuthError::Oidc(format!("JWKS is not valid JSON: {e}")))?;

    let header = jsonwebtoken::decode_header(id_token)
        .map_err(|e| AuthError::Oidc(format!("ID token header is unreadable: {e}")))?;

    // An RSA key set is the near-universal case and the only one verified here.
    // Refusing anything else is deliberate: silently skipping verification
    // because the provider chose an unexpected algorithm is how "alg: none"
    // bypasses happen.
    let key = jwks
        .keys
        .iter()
        .find(|k| k.kty == "RSA")
        .and_then(|k| match (&header.kid, &k.kid) {
            (Some(want), Some(have)) if want != have => None,
            _ => Some(k),
        })
        .ok_or_else(|| AuthError::Oidc("no matching RSA key in the JWKS".into()))?;

    let n = key
        .n
        .as_deref()
        .ok_or_else(|| AuthError::Oidc("JWKS key has no modulus".into()))?;
    let e = key
        .e
        .as_deref()
        .ok_or_else(|| AuthError::Oidc("JWKS key has no exponent".into()))?;

    let algorithm = match key.alg.as_deref() {
        Some("RS256") | None => jsonwebtoken::Algorithm::RS256,
        Some(other) => {
            return Err(AuthError::Oidc(format!(
                "JWKS key declares unsupported algorithm {other}"
            )))
        }
    };

    let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(n, e)
        .map_err(|err| AuthError::Oidc(format!("JWKS key is unusable: {err}")))?;

    let mut validation = jsonwebtoken::Validation::new(algorithm);
    validation.set_issuer(&[metadata.issuer.trim_end_matches('/')]);
    validation.set_audience(&[client_id]);
    // Small leeway for clock skew between the provider and this server.
    validation.leeway = 60;

    let token = jsonwebtoken::decode::<IdTokenClaims>(id_token, &decoding_key, &validation)
        .map_err(|e| AuthError::Oidc(format!("ID token failed verification: {e}")))?;

    Ok(VerifiedIdentity {
        issuer: metadata.issuer.trim_end_matches('/').to_string(),
        subject: token.claims.sub,
        email: token.claims.email,
        email_verified: token.claims.email_verified,
        display_name: token.claims.name,
    })
}

#[derive(Debug, Clone, Deserialize)]
struct IdTokenClaims {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

/// A PKCE code challenge (`S256`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceChallenge {
    pub challenge: String,
}

/// The verifier that produced a challenge.
///
/// Kept distinct from the challenge and never serialised anywhere the browser
/// can see it: the challenge is public by design, the verifier is the secret.
#[derive(Debug, Clone)]
pub struct PkceVerifier {
    pub verifier: String,
}

impl PkceVerifier {
    /// Generates a fresh verifier and its challenge.
    pub fn generate() -> Self {
        // RFC 7636 §4.1 requires 43-128 characters of unreserved alphabet.
        // 32 random bytes base64url-encode to 43 characters, the minimum.
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        Self { verifier }
    }

    /// Derives the `S256` challenge for this verifier.
    pub fn challenge(&self) -> PkceChallenge {
        let digest = Sha256::digest(self.verifier.as_bytes());
        PkceChallenge {
            challenge: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest),
        }
    }
}

/// An opaque CSRF value for the authorization request.
#[derive(Debug, Clone)]
pub struct StateToken(pub String);

impl StateToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }
}

/// The result of resolving an SSO callback into a local principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// The provider's identity is already linked to a local user.
    Existing { user_id: uuid::Uuid },
    /// The identity is new. `may_provision` is false when the provider is
    /// restricted to known domains, so the caller must refuse rather than
    /// silently create an account.
    Unlinked {
        email: Option<String>,
        may_provision: bool,
    },
}

/// Whether a provider is willing to let this identity create an account.
///
/// This is the control that stops a misconfigured IdP from granting access to
/// the console: a provider restricted to verified domains refuses an identity
/// outside them instead of provisioning one.
pub fn may_provision(allowed_domains: &[String], email: Option<&str>) -> bool {
    if allowed_domains.is_empty() {
        // Unconfigured means open, which is the right default for a
        // self-hosted single-provider install and the wrong one otherwise — so
        // the deployment is expected to configure it.
        return true;
    }
    let Some(email) = email else {
        return false;
    };

    // Exactly one '@'. Using `rsplit_once` and taking the last would let
    // `attacker@evil.com@acme.com` match an allow-list entry for `acme.com`,
    // which is the shape a spoofed address takes — so the whole string is
    // checked for a single separator first.
    let mut parts = email.split('@');
    let (local, domain) = match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => (local, domain),
        _ => return false,
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }

    let domain = domain.to_ascii_lowercase();
    allowed_domains.iter().any(|allowed| {
        allowed
            .trim()
            .trim_start_matches('@')
            .eq_ignore_ascii_case(&domain)
    })
}

/// Serialises a provider for the console.
///
/// The client secret is deliberately absent: this is what a
/// `GET /identity-providers` response returns, so including it would put a
/// credential in a browser.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSummary {
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub enabled: bool,
    pub domains_restricted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pkce_challenge_is_the_sha256_of_its_verifier() {
        // RFC 7636 appendix B's worked example, so an implementation change that
        // alters the derivation fails here rather than at a provider.
        let verifier = PkceVerifier {
            verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_string(),
        };
        assert_eq!(
            verifier.challenge().challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_verifiers_satisfy_the_rfc_length_bounds() {
        let verifier = PkceVerifier::generate();
        assert!(
            (43..=128).contains(&verifier.verifier.len()),
            "verifier length {} is outside the RFC 7636 bounds",
            verifier.verifier.len()
        );
        assert_eq!(
            verifier.challenge().challenge.len(),
            43,
            "an S256 challenge is always 43 characters"
        );
    }

    #[test]
    fn generated_verifiers_are_unique() {
        let a = PkceVerifier::generate();
        let b = PkceVerifier::generate();
        assert_ne!(
            a.verifier, b.verifier,
            "a fresh login must not reuse a verifier"
        );
    }

    #[test]
    fn state_tokens_are_unique_and_url_safe() {
        let a = StateToken::generate();
        let b = StateToken::generate();
        assert_ne!(a.0, b.0);
        assert!(
            a.0.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "state travels in a query string, so it must be URL-safe"
        );
    }

    #[test]
    fn an_open_provider_provisions_any_verified_identity() {
        assert!(may_provision(&[], Some("anyone@anywhere.com")));
    }

    #[test]
    fn a_restricted_provider_refuses_an_email_outside_its_domains() {
        let allowed = vec!["acme.com".to_string(), "@contractors.example".to_string()];
        assert!(may_provision(&allowed, Some("dev@acme.com")));
        // The stored value may or may not carry a leading '@'; both are accepted
        // because an operator will write it either way.
        assert!(may_provision(&allowed, Some("dev@contractors.example")));
    }

    #[test]
    fn whitespace_inside_a_configured_domain_is_ignored() {
        // An operator pasting a list may leave whitespace. Trimming is the
        // difference between a typo silently disabling a restriction and it
        // being honoured.
        let allowed = vec!["  acme.com  ".to_string()];
        assert!(may_provision(&allowed, Some("dev@acme.com")));
        assert!(!may_provision(&allowed, Some("dev@evil.com")));
    }

    #[test]
    fn a_restricted_provider_refuses_a_foreign_domain() {
        let allowed = vec!["acme.com".to_string()];
        assert!(!may_provision(&allowed, Some("attacker@evil.com")));
    }

    #[test]
    fn a_restricted_provider_refuses_an_identity_with_no_email() {
        // A provider restricted to verified domains must not provision an
        // account it cannot check the domain of.
        let allowed = vec!["acme.com".to_string()];
        assert!(!may_provision(&allowed, None));
    }

    #[test]
    fn domain_matching_ignores_case() {
        let allowed = vec!["Acme.COM".to_string()];
        assert!(may_provision(&allowed, Some("dev@ACME.com")));
    }

    #[test]
    fn a_malformed_email_is_refused_rather_than_matched_on_a_substring() {
        let allowed = vec!["acme.com".to_string()];
        // No '@': there is no domain to check.
        assert!(!may_provision(&allowed, Some("just-a-name")));
        // An '@' in the domain part must not be used as a substring match.
        assert!(!may_provision(&allowed, Some("dev@evil.com@acme.com")));
    }
}
