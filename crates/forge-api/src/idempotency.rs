//! Idempotency (spec 02.15, spec 05 §5.1).
//!
//! A client may send `Idempotency-Key` on a mutation. The server stores a
//! fingerprint of the request alongside the key, which is what lets it tell
//! "the same request replayed" from "the same key used for something else".

use axum::http::StatusCode;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::envelope::{ApiError, ApiResponse};

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";

/// Maximum accepted key length. A longer value is a client bug, not a key.
const MAX_KEY_LEN: usize = 255;

/// A request fingerprint.
///
/// SHA-256 over the canonical body plus the endpoint, so the same body sent to
/// two different endpoints is not treated as a replay.
pub fn fingerprint<T: Serialize>(endpoint: &str, body: &T) -> String {
    let rendered = serde_json::to_string(body).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(endpoint.as_bytes());
    hasher.update([0u8]);
    hasher.update(rendered.as_bytes());
    hex(&hasher.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Validates a client-supplied idempotency key.
pub fn validate_key(raw: &str) -> Result<String, ApiError> {
    if raw.is_empty() {
        return Err(ApiError::validation("idempotency key must not be empty")
            .with_detail("Idempotency-Key", "must not be empty"));
    }
    if raw.len() > MAX_KEY_LEN {
        return Err(ApiError::validation(format!(
            "idempotency key must be at most {MAX_KEY_LEN} characters"
        ))
        .with_detail("Idempotency-Key", "too long"));
    }
    // A control character in a key would end up in a log line and a database
    // index, so reject rather than sanitise.
    if !raw.chars().all(|c| c.is_ascii_graphic()) {
        return Err(
            ApiError::validation("idempotency key must contain only printable ASCII")
                .with_detail("Idempotency-Key", "contains a control character"),
        );
    }
    Ok(raw.to_string())
}

/// How a request carrying an idempotency key should proceed.
#[derive(Debug)]
pub enum IdempotencyAction {
    /// The key is new; run the operation and record the response.
    Proceed { key: String, fingerprint: String },
    /// The same request already ran; return the stored response verbatim.
    Replay {
        status: StatusCode,
        body: serde_json::Value,
    },
}

/// Consults the idempotency store.
pub async fn begin(
    pool: &sqlx::PgPool,
    tenant_id: forge_domain::TenantId,
    key: Option<&str>,
    endpoint: &str,
    body: &serde_json::Value,
) -> Result<IdempotencyAction, ApiError> {
    let Some(raw_key) = key else {
        // No key supplied: the endpoint is simply not idempotent.
        return Ok(IdempotencyAction::Proceed {
            key: String::new(),
            fingerprint: String::new(),
        });
    };

    let key = validate_key(raw_key)?;
    let print = fingerprint(endpoint, body);

    let repo = forge_storage::IdempotencyRepository::new(pool);
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);

    match repo
        .reserve(tenant_id, &key, endpoint, &print, expires_at)
        .await
    {
        Ok(forge_storage::IdempotencyOutcome::Fresh) => Ok(IdempotencyAction::Proceed {
            key,
            fingerprint: print,
        }),
        Ok(forge_storage::IdempotencyOutcome::Replay { status, body }) => {
            Ok(IdempotencyAction::Replay {
                status: StatusCode::from_u16(status as u16).unwrap_or(StatusCode::OK),
                body,
            })
        }
        // AT-API-006: the same key with a different body is a conflict.
        Ok(forge_storage::IdempotencyOutcome::Conflict) => Err(ApiError::idempotency_conflict()),
        Err(e) => Err(ApiError::from(e)),
    }
}

/// Records the response for a key so a later replay returns it.
pub async fn complete(
    pool: &sqlx::PgPool,
    tenant_id: forge_domain::TenantId,
    key: &str,
    endpoint: &str,
    status: StatusCode,
    body: &serde_json::Value,
    resource_id: Option<uuid::Uuid>,
) -> Result<(), ApiError> {
    if key.is_empty() {
        return Ok(());
    }
    let repo = forge_storage::IdempotencyRepository::new(pool);
    repo.complete(
        tenant_id,
        key,
        endpoint,
        status.as_u16() as i32,
        body,
        resource_id,
    )
    .await
    .map_err(ApiError::from)
}

/// What a handler produced, before it is rendered.
pub type HandlerResult<T> = Result<(StatusCode, T, Option<uuid::Uuid>), ApiError>;

/// Wraps a handler so it participates in the idempotency protocol.
///
/// A replay returns the stored status and body unchanged, so a retried request
/// is indistinguishable from the original. The typed return lets a caller
/// render the response however it likes, rather than this module producing a
/// response and forcing callers to take it apart again.
pub async fn run<T, F, Fut>(
    pool: &sqlx::PgPool,
    tenant_id: forge_domain::TenantId,
    key: Option<&str>,
    endpoint: &str,
    request_body: &serde_json::Value,
    request_id: &str,
    handler: F,
) -> Result<IdempotentOutcome<T>, ApiError>
where
    T: Serialize,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = HandlerResult<T>>,
{
    match begin(pool, tenant_id, key, endpoint, request_body).await? {
        IdempotencyAction::Replay { status, body } => {
            // The stored body is the full envelope from the first call, so it
            // is replayed verbatim. Handing back only `data` would drop the
            // original `request_id` and change the shape of the response.
            Ok(IdempotentOutcome::Replay { status, data: body })
        }
        IdempotencyAction::Proceed { key, .. } => {
            let (status, payload, resource_id) = handler().await?;
            let envelope = serde_json::json!({
                "data": payload,
                "request_id": request_id,
            });
            complete(
                pool,
                tenant_id,
                &key,
                endpoint,
                status,
                &envelope,
                resource_id,
            )
            .await?;
            Ok(IdempotentOutcome::Fresh {
                status,
                data: payload,
            })
        }
    }
}

/// The result of an idempotent mutation: either a fresh response or a replay of
/// a previous one.
#[derive(Debug)]
pub enum IdempotentOutcome<T> {
    Fresh {
        status: StatusCode,
        data: T,
    },
    Replay {
        status: StatusCode,
        data: serde_json::Value,
    },
}

impl<T: Serialize> IdempotentOutcome<T> {
    /// The status to answer with.
    pub fn status(&self) -> StatusCode {
        match self {
            IdempotentOutcome::Fresh { status, .. } | IdempotentOutcome::Replay { status, .. } => {
                *status
            }
        }
    }

    /// Renders the outcome, preserving the envelope shape of a fresh response.
    pub fn into_response(self, request_id: &str) -> axum::response::Response {
        use axum::response::IntoResponse;
        match self {
            IdempotentOutcome::Fresh { status, data } => {
                let envelope = ApiResponse::new(data, request_id);
                (status, axum::Json(envelope)).into_response()
            }
            IdempotentOutcome::Replay { status, data } => {
                // The stored envelope is returned verbatim, including its
                // original request id, so a replay is indistinguishable from
                // the first call.
                (status, axum::Json(data)).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_valid_key_is_accepted() {
        assert_eq!(validate_key("abc-123").unwrap(), "abc-123");
        assert_eq!(validate_key(&"k".repeat(255)).unwrap().len(), 255);
    }

    #[test]
    fn an_empty_or_oversized_key_is_rejected() {
        assert!(validate_key("").is_err());
        assert!(validate_key(&"k".repeat(256)).is_err());
    }

    /// A control character would end up in a log line and an index.
    #[test]
    fn a_key_with_a_control_character_is_rejected() {
        for hostile in ["abc\ndef", "abc\tdef", "abc\0def", "has space"] {
            assert!(
                validate_key(hostile).is_err(),
                "{hostile:?} must be rejected"
            );
        }
    }

    /// AT-API-005: the same body at the same endpoint fingerprints the same.
    #[test]
    fn identical_requests_share_a_fingerprint() {
        let a = fingerprint("POST /jobs", &json!({"name": "x"}));
        let b = fingerprint("POST /jobs", &json!({"name": "x"}));
        assert_eq!(a, b);
    }

    /// AT-API-006: a different body must fingerprint differently.
    #[test]
    fn different_requests_differ() {
        let a = fingerprint("POST /jobs", &json!({"name": "x"}));
        let b = fingerprint("POST /jobs", &json!({"name": "y"}));
        assert_ne!(a, b, "a changed body must not look like a replay");
    }

    #[test]
    fn the_endpoint_participates_in_the_fingerprint() {
        // The same body at two endpoints is not a replay of the first.
        let a = fingerprint("POST /jobs", &json!({"name": "x"}));
        let b = fingerprint("POST /workflows", &json!({"name": "x"}));
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprints_are_stable_and_hex() {
        let print = fingerprint("POST /jobs", &json!({"a": 1}));
        assert_eq!(print.len(), 64);
        assert!(print.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(print, fingerprint("POST /jobs", &json!({"a": 1})));
    }

    #[test]
    fn field_order_does_not_change_the_fingerprint() {
        // `serde_json::Value` objects are sorted maps, so two bodies that
        // differ only in the order their keys were written hash identically.
        // That is the behaviour a client wants: a retry that re-serialises its
        // JSON in a different key order is still recognised as the same
        // request, rather than being rejected with a spurious 409.
        let a = fingerprint("POST /jobs", &json!({"a": 1, "b": 2}));
        let b = fingerprint("POST /jobs", &json!({"b": 2, "a": 1}));
        assert_eq!(a, b, "key order must not look like a different request");
    }

    #[test]
    fn nested_field_order_also_does_not_change_the_fingerprint() {
        let a = fingerprint("POST /jobs", &json!({"cfg": {"x": 1, "y": 2}}));
        let b = fingerprint("POST /jobs", &json!({"cfg": {"y": 2, "x": 1}}));
        assert_eq!(a, b);
    }
}
