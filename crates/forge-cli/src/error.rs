//! CLI errors, mapped onto the exit codes in spec 16.8.

use crate::output::ExitCode;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// A command line that does not parse.
    #[error("{0}")]
    Usage(String),

    /// The credential is missing, expired, or rejected.
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// The credential is valid but lacks permission.
    #[error("authorization failed: {0}")]
    Authorization(String),

    /// The resource does not exist, or is not visible to this context.
    #[error("not found: {0}")]
    NotFound(String),

    /// A conflicting concurrent change, or a reused idempotency key.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The server rejected the input.
    #[error("validation failed: {0}")]
    Validation(String),

    /// The server could not be reached, or returned 5xx.
    #[error("server error: {0}")]
    Network(String),

    /// The local config file could not be read or written.
    #[error("configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    General(String),
}

impl CliError {
    /// The process exit code for this failure.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            CliError::Usage(_) => ExitCode::UsageError,
            CliError::Authentication(_) => ExitCode::AuthenticationFailure,
            CliError::Authorization(_) => ExitCode::AuthorizationFailure,
            CliError::NotFound(_) => ExitCode::NotFound,
            CliError::Conflict(_) => ExitCode::Conflict,
            CliError::Validation(_) => ExitCode::ValidationFailure,
            CliError::Network(_) | CliError::Config(_) => ExitCode::NetworkFailure,
            CliError::General(_) => ExitCode::GeneralFailure,
        }
    }
}

/// Maps an HTTP status onto the right failure kind.
///
/// The mapping is what makes the documented exit codes real: a script that sees
/// `5` knows the resource is missing, and one that sees `3` knows to
/// re-authenticate.
pub fn from_status(status: u16, body: &serde_json::Value) -> CliError {
    let code = body
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("the request was refused")
        .to_string();

    let detail = if code.is_empty() {
        message
    } else {
        format!("{message} ({code})")
    };

    match status {
        401 => CliError::Authentication(detail),
        403 => CliError::Authorization(detail),
        // A cross-tenant read is reported as 404, so the CLI surfaces it the
        // same way the server does rather than pretending it is a permission
        // problem.
        404 => CliError::NotFound(detail),
        409 => CliError::Conflict(detail),
        // 429 is a rate limit rather than a permission problem.
        429 => CliError::Network(format!("rate limited: {detail}")),
        400 | 422 => CliError::Validation(detail),
        500..=599 => CliError::Network(detail),
        other => CliError::General(format!("unexpected status {other}: {detail}")),
    }
}

pub type Result<T> = std::result::Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn body(code: &str, message: &str) -> serde_json::Value {
        json!({ "error": { "code": code, "message": message, "request_id": "r" } })
    }

    #[test]
    fn each_error_maps_to_its_documented_exit_code() {
        assert_eq!(
            CliError::Usage("x".into()).exit_code(),
            ExitCode::UsageError
        );
        assert_eq!(
            CliError::Authentication("x".into()).exit_code(),
            ExitCode::AuthenticationFailure
        );
        assert_eq!(
            CliError::Authorization("x".into()).exit_code(),
            ExitCode::AuthorizationFailure
        );
        assert_eq!(
            CliError::NotFound("x".into()).exit_code(),
            ExitCode::NotFound
        );
        assert_eq!(
            CliError::Conflict("x".into()).exit_code(),
            ExitCode::Conflict
        );
        assert_eq!(
            CliError::Validation("x".into()).exit_code(),
            ExitCode::ValidationFailure
        );
        assert_eq!(
            CliError::Network("x".into()).exit_code(),
            ExitCode::NetworkFailure
        );
        assert_eq!(
            CliError::General("x".into()).exit_code(),
            ExitCode::GeneralFailure
        );
    }

    #[test]
    fn http_statuses_map_to_the_right_failure() {
        assert!(matches!(
            from_status(401, &body("AUTHENTICATION_REQUIRED", "no")),
            CliError::Authentication(_)
        ));
        assert!(matches!(
            from_status(403, &body("AUTHORIZATION_DENIED", "no")),
            CliError::Authorization(_)
        ));
        assert!(matches!(
            from_status(404, &body("NOT_FOUND", "gone")),
            CliError::NotFound(_)
        ));
        assert!(matches!(
            from_status(409, &body("CONFLICT", "stale")),
            CliError::Conflict(_)
        ));
        assert!(matches!(
            from_status(400, &body("VALIDATION_ERROR", "bad")),
            CliError::Validation(_)
        ));
        assert!(matches!(
            from_status(503, &body("DEPENDENCY_UNAVAILABLE", "down")),
            CliError::Network(_)
        ));
        assert!(matches!(
            from_status(429, &body("RATE_LIMITED", "slow down")),
            CliError::Network(_)
        ));
    }

    #[test]
    fn a_rate_limit_is_not_reported_as_a_permission_problem() {
        let error = from_status(429, &body("RATE_LIMITED", "slow down"));
        assert!(
            error.to_string().contains("rate limited"),
            "the message must name the cause: {error}"
        );
    }

    #[test]
    fn the_error_body_is_surfaced_for_the_operator() {
        let error = from_status(404, &body("NOT_FOUND", "job was not found"));
        let text = error.to_string();
        assert!(text.contains("job was not found"), "{text}");
        assert!(text.contains("NOT_FOUND"), "{text}");
    }

    #[test]
    fn an_unrecognised_status_still_produces_a_failure() {
        let error = from_status(418, &json!({}));
        assert!(matches!(error, CliError::General(_)));
        assert!(error.to_string().contains("418"));
    }

    #[test]
    fn a_body_without_an_error_object_is_tolerated() {
        // A proxy or load balancer may return HTML or nothing at all.
        let error = from_status(500, &json!({}));
        assert!(matches!(error, CliError::Network(_)));
    }
}
