//! Response envelopes and the error taxonomy (spec 05 §5.1).
//!
//! Every endpoint answers with one of three shapes:
//!
//! * single resource — `{"data": …, "request_id": …}`
//! * collection — the same, plus a `page` object
//! * failure — `{"error": {"code", "message", "details", "request_id"}}`
//!
//! Error codes come from the spec 02.14 taxonomy so a client can branch on a
//! stable value rather than parsing prose. Stack traces never cross the wire
//! (spec 01.22 invariant 12).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;

/// Single-resource success envelope.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
    pub request_id: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn new(data: T, request_id: impl Into<String>) -> Self {
        Self {
            data,
            request_id: request_id.into(),
        }
    }
}

/// Cursor-pagination metadata (spec 05 §5.14).
#[derive(Debug, Default, Serialize)]
pub struct PageInfo {
    /// Opaque token; `null` on the last page.
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl PageInfo {
    pub fn empty() -> Self {
        Self {
            next_cursor: None,
            has_more: false,
        }
    }
}

/// Collection success envelope.
#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub page: PageInfo,
    pub request_id: String,
}

impl<T: Serialize> ListResponse<T> {
    pub fn new(data: Vec<T>, page: PageInfo, request_id: impl Into<String>) -> Self {
        Self {
            data,
            page,
            request_id: request_id.into(),
        }
    }

    /// Builds a page from a storage-layer [`forge_storage::Page`].
    pub fn from_page(page: forge_storage::Page<T>, request_id: impl Into<String>) -> Self {
        Self {
            data: page.items,
            page: PageInfo {
                next_cursor: page.next_cursor,
                has_more: page.has_more,
            },
            request_id: request_id.into(),
        }
    }
}

/// One field-level problem in a rejected request.
#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorDetail {
    /// Dotted path to the offending field, e.g. `name` or `config.timeout`.
    pub field: String,
    pub message: String,
}

/// The error body.
#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<ApiErrorDetail>,
    pub request_id: String,
}

/// An API failure, carrying the status and a spec-02.14 code.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Vec<ApiErrorDetail>,
    pub request_id: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: Vec::new(),
            request_id: String::new(),
        }
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    pub fn with_detail(mut self, field: impl Into<String>, message: impl Into<String>) -> Self {
        self.details.push(ApiErrorDetail {
            field: field.into(),
            message: message.into(),
        });
        self
    }

    // --- constructors, one per spec 02.14 class ---

    /// 400. The request was malformed or failed validation.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", message)
    }

    /// 401. No credential was presented, or it was not valid.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "AUTHENTICATION_REQUIRED", message)
    }

    /// 403. The credential was valid but lacks the permission.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "AUTHORIZATION_DENIED", message)
    }

    /// 404. The resource does not exist, or is in another tenant.
    ///
    /// Spec 11.5 and AT-TEN-003: a cross-tenant read reports not-found rather
    /// than forbidden, so the error cannot be used to discover that another
    /// tenant's resource exists.
    pub fn not_found(what: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("{what} was not found"),
        )
    }

    /// 409. A conflicting concurrent update, or a reused idempotency key.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "CONFLICT", message)
    }

    /// 409. The same idempotency key was reused with a different request.
    pub fn idempotency_conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_CONFLICT",
            "this idempotency key was already used with a different request body",
        )
    }

    /// 429. Rate limited (spec 02.14 `RATE_LIMITED`).
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED", message)
    }

    /// 503. A dependency is unavailable (spec 02.14 `DEPENDENCY_UNAVAILABLE`).
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "DEPENDENCY_UNAVAILABLE",
            message,
        )
    }

    /// 500. An unexpected internal failure.
    ///
    /// The message is deliberately generic: an internal error may describe a
    /// schema or a connection string, neither of which belongs in a response.
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "an internal error occurred",
        )
    }

    /// The wire body.
    pub fn to_body(&self) -> ApiErrorResponse {
        ApiErrorResponse {
            error: ApiErrorBody {
                code: self.code.to_string(),
                message: self.message.clone(),
                details: self.details.clone(),
                request_id: self.request_id.clone(),
            },
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Fill in the ambient request id so an error envelope is always
        // traceable back to a log line.
        (self.status, Json(self.with_ambient_request_id().to_body())).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        // Route through the storage error mapping so SQLSTATE codes become the
        // same status codes a repository call would produce.
        ApiError::from(forge_storage::StorageError::from_sqlx(err))
    }
}

impl From<forge_storage::StorageError> for ApiError {
    fn from(err: forge_storage::StorageError) -> Self {
        match err {
            forge_storage::StorageError::NotFound { entity } => ApiError::not_found(entity),
            forge_storage::StorageError::Conflict(message) => ApiError::conflict(message),
            forge_storage::StorageError::Validation(message) => ApiError::validation(message),
            forge_storage::StorageError::TenantIsolation(message) => {
                // Never leak that another tenant's resource exists.
                ApiError::not_found("resource").with_detail("tenant", message)
            }
            forge_storage::StorageError::InvalidCursor(message) => {
                ApiError::validation(format!("invalid cursor: {message}"))
                    .with_detail("cursor", message)
            }
            forge_storage::StorageError::Database(_)
            | forge_storage::StorageError::Migration(_) => {
                // The underlying error is logged, not returned to the client.
                tracing::error!(
                    error = %err,
                    "database failure while handling a request"
                );
                ApiError::internal()
            }
        }
    }
}

impl From<forge_domain::DomainError> for ApiError {
    fn from(err: forge_domain::DomainError) -> Self {
        use forge_domain::DomainError::*;
        match err {
            InvalidStateTransition { from, to } => {
                ApiError::conflict(format!("cannot move from {from} to {to}"))
            }
            ValidationError(message) | InvalidWorkflow(message) => ApiError::validation(message),
            ConcurrencyLimitExceeded(message) | RetryNotPermitted(message) => {
                ApiError::conflict(message)
            }
            TenantIsolation(message) => {
                ApiError::not_found("resource").with_detail("tenant", message)
            }
        }
    }
}

impl From<forge_auth::AuthError> for ApiError {
    fn from(err: forge_auth::AuthError) -> Self {
        use forge_auth::AuthError::*;
        match err {
            InvalidToken | TokenExpired => {
                ApiError::unauthenticated("the access token is invalid or has expired")
            }
            InvalidCredentials => ApiError::unauthenticated("invalid email or password"),
            AccountDisabled => ApiError::forbidden("this account has been disabled"),
            PermissionDenied(permission) => {
                ApiError::forbidden(format!("missing permission `{permission}`"))
            }
            TenantIsolation(_) => ApiError::not_found("resource"),
            // An identity-provider failure is a configuration or upstream
            // problem, not a problem with the caller's credentials — so 502,
            // and the provider's own message stays in the log rather than the
            // response body.
            Oidc(_) => ApiError::unavailable(
                "the identity provider could not be reached. Try again shortly.",
            ),
            WeakPassword(message) => {
                ApiError::validation(message.clone()).with_detail("password", message)
            }
            RefreshTokenReuse => {
                // Deliberately not 401: the session is revoked, not merely stale.
                ApiError::unauthenticated("session revoked; please sign in again")
            }
            Crypto(message) => {
                tracing::error!(error = %message, "cryptographic failure");
                ApiError::internal()
            }
        }
    }
}

/// Pagination parameters, parsed from the query string.
///
/// The names are pinned here and documented in the OpenAPI specification
/// (ADR: cursor parameter names).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PaginationQuery {
    pub cursor: Option<String>,
    /// Defaults to 50 and is capped, so a client cannot ask for an unbounded page.
    #[serde(default)]
    pub limit: Option<usize>,
    pub sort: Option<String>,
}

pub const DEFAULT_PAGE_LIMIT: usize = 50;
pub const MAX_PAGE_LIMIT: usize = 200;

impl PaginationQuery {
    /// The effective limit after applying the default and the cap.
    pub fn effective_limit(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_PAGE_LIMIT)
            .clamp(1, MAX_PAGE_LIMIT)
    }
}

/// Serialises a JSON value for an error body.
pub fn details_json(details: &[ApiErrorDetail]) -> serde_json::Value {
    json!(details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn rendered(error: ApiError) -> Value {
        serde_json::to_value(error.to_body().error).unwrap()
    }

    #[test]
    fn a_single_envelope_carries_the_request_id() {
        let response = ApiResponse::new(json!({"id": "abc"}), "req-1");
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["data"]["id"], "abc");
        assert_eq!(value["request_id"], "req-1");
        assert!(value.get("page").is_none(), "a single response has no page");
    }

    #[test]
    fn a_list_envelope_carries_page_metadata() {
        let response = ListResponse::new(
            vec![json!(1), json!(2)],
            PageInfo {
                next_cursor: Some("cur".into()),
                has_more: true,
            },
            "req-2",
        );
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["data"].as_array().unwrap().len(), 2);
        assert_eq!(value["page"]["next_cursor"], "cur");
        assert_eq!(value["page"]["has_more"], true);
    }

    #[test]
    fn a_list_converts_from_a_storage_page() {
        let page = forge_storage::Page {
            items: vec![json!("a")],
            next_cursor: Some("next".into()),
            has_more: true,
        };
        let value = serde_json::to_value(ListResponse::from_page(page, "req-3")).unwrap();
        assert_eq!(value["page"]["next_cursor"], "next");
    }

    #[test]
    fn an_empty_page_reports_no_cursor() {
        let info = PageInfo::empty();
        assert!(info.next_cursor.is_none());
        assert!(!info.has_more);
    }

    #[test]
    fn each_error_constructor_uses_its_spec_class() {
        assert_eq!(ApiError::validation("x").code, "VALIDATION_ERROR");
        assert_eq!(
            ApiError::unauthenticated("x").code,
            "AUTHENTICATION_REQUIRED"
        );
        assert_eq!(ApiError::forbidden("x").code, "AUTHORIZATION_DENIED");
        assert_eq!(ApiError::not_found("job").code, "NOT_FOUND");
        assert_eq!(ApiError::conflict("x").code, "CONFLICT");
        assert_eq!(ApiError::rate_limited("x").code, "RATE_LIMITED");
        assert_eq!(ApiError::unavailable("x").code, "DEPENDENCY_UNAVAILABLE");
        assert_eq!(ApiError::internal().code, "INTERNAL_ERROR");
    }

    #[test]
    fn each_error_constructor_uses_its_spec_status() {
        assert_eq!(ApiError::validation("x").status, StatusCode::BAD_REQUEST);
        assert_eq!(
            ApiError::unauthenticated("x").status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(ApiError::forbidden("x").status, StatusCode::FORBIDDEN);
        assert_eq!(ApiError::not_found("x").status, StatusCode::NOT_FOUND);
        assert_eq!(ApiError::conflict("x").status, StatusCode::CONFLICT);
        assert_eq!(
            ApiError::rate_limited("x").status,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            ApiError::unavailable("x").status,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            ApiError::internal().status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    /// Spec 01.22 invariant 12: no stack traces in production responses.
    #[test]
    fn an_internal_error_leaks_nothing() {
        let body = rendered(ApiError::internal());
        let text = body.to_string();
        assert_eq!(body["message"], "an internal error occurred");
        assert!(!text.contains("panic"));
        assert!(!text.contains("sqlx"), "driver detail must not escape");
    }

    #[test]
    fn field_details_are_serialised() {
        let error = ApiError::validation("name is required")
            .with_detail("name", "must not be empty")
            .with_request_id("req-4");
        let body = rendered(error);
        assert_eq!(body["details"][0]["field"], "name");
        assert_eq!(body["details"][0]["message"], "must not be empty");
        assert_eq!(body["request_id"], "req-4");
    }

    /// AT-TEN-003: a cross-tenant read must not reveal the other tenant.
    #[test]
    fn a_tenant_isolation_failure_reads_as_not_found() {
        let error = ApiError::from(forge_storage::StorageError::TenantIsolation(
            "belongs to another tenant".into(),
        ));
        assert_eq!(error.status, StatusCode::NOT_FOUND);
        let body = rendered(error);
        // The status and code say nothing; the detail carries the diagnosis for
        // the operator reading the log, not for an enumeration attempt.
        assert_eq!(body["code"], "NOT_FOUND");
        assert_eq!(body["details"][0]["field"], "tenant");
    }

    #[test]
    fn storage_errors_map_to_the_right_status() {
        let not_found: ApiError = forge_storage::StorageError::not_found("job").into();
        assert_eq!(not_found.status, StatusCode::NOT_FOUND);

        let conflict: ApiError = forge_storage::StorageError::Conflict("duplicate".into()).into();
        assert_eq!(conflict.status, StatusCode::CONFLICT);

        let invalid: ApiError = forge_storage::StorageError::InvalidCursor("bad".into()).into();
        assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn domain_errors_map_to_the_right_status() {
        use forge_domain::DomainError;

        let invalid: ApiError = DomainError::ValidationError("bad name".into()).into();
        assert_eq!(invalid.status, StatusCode::BAD_REQUEST);

        let transition: ApiError = DomainError::InvalidStateTransition {
            from: "DRAFT".into(),
            to: "SUCCEEDED".into(),
        }
        .into();
        assert_eq!(
            transition.status,
            StatusCode::CONFLICT,
            "an illegal transition is a conflict, not a bad request"
        );
    }

    #[test]
    fn auth_errors_map_to_the_right_status() {
        use forge_auth::AuthError;

        let invalid: ApiError = AuthError::InvalidToken.into();
        assert_eq!(invalid.status, StatusCode::UNAUTHORIZED);

        let denied: ApiError = AuthError::PermissionDenied("jobs:write".into()).into();
        assert_eq!(denied.status, StatusCode::FORBIDDEN);

        let reuse: ApiError = AuthError::RefreshTokenReuse.into();
        assert_eq!(reuse.status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn pagination_defaults_and_caps_the_limit() {
        let default: PaginationQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(default.effective_limit(), DEFAULT_PAGE_LIMIT);

        let capped: PaginationQuery = serde_json::from_str(r#"{"limit": 100000}"#).unwrap();
        assert_eq!(capped.effective_limit(), MAX_PAGE_LIMIT);

        let zero: PaginationQuery = serde_json::from_str(r#"{"limit": 0}"#).unwrap();
        assert_eq!(
            zero.effective_limit(),
            1,
            "a page must hold at least one row"
        );

        let explicit: PaginationQuery = serde_json::from_str(r#"{"limit": 25}"#).unwrap();
        assert_eq!(explicit.effective_limit(), 25);
    }

    #[test]
    fn pagination_parses_a_cursor_and_sort() {
        let query: PaginationQuery =
            serde_json::from_str(r#"{"cursor": "abc", "sort": "-created_at"}"#).unwrap();
        assert_eq!(query.cursor.as_deref(), Some("abc"));
        assert_eq!(query.sort.as_deref(), Some("-created_at"));
    }
}

// The request id for the request currently being handled. The layer in
// `router.rs` stamps `x-request-id` onto the request; this makes the same value
// available to any handler that wants to report it, including the ones that
// build an `ApiError` without an explicit id.
tokio::task_local! {
    static REQUEST_ID: String;
}

/// Sets the request id for the duration of one request's handling.
pub fn with_request_id<F>(
    request_id: String,
    future: F,
) -> impl std::future::Future<Output = F::Output>
where
    F: std::future::Future,
{
    REQUEST_ID.scope(request_id, future)
}

/// The current request id, if the request went through `with_request_id`.
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(|id| id.clone()).ok()
}

impl ApiError {
    /// Attaches the ambient request id when none was set explicitly.
    ///
    /// An error envelope whose `request_id` is empty is useless to whoever
    /// receives it: there is nothing to search the logs for. This fills it in
    /// from the request currently being handled.
    pub fn with_ambient_request_id(mut self) -> Self {
        if self.request_id.is_empty() {
            if let Some(id) = current_request_id() {
                self.request_id = id;
            }
        }
        self
    }
}

/// Publishes the request id for the duration of one request's handling.
///
/// Runs before the router so a handler that fails without an explicit id still
/// produces an envelope a support engineer can search for.
pub async fn request_id_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // `SetRequestIdLayer` has already stamped a header on the request, so this
    // reads the same id the trace span carries rather than minting another.
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut response = with_request_id(request_id.clone(), next.run(request)).await;
    // Echo it back so a client can quote it in a support request.
    if let Ok(value) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}
