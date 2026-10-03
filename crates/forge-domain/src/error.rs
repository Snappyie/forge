use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error taxonomy from spec 02.14.
///
/// Retry decisions MUST be driven by this classification rather than by
/// matching free-text error strings (spec 10.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorClass {
    Validation,
    Authentication,
    Authorization,
    NotFound,
    Conflict,
    RateLimited,
    Transient,
    DependencyUnavailable,
    Timeout,
    Cancellation,
    ResourceExhausted,
    Permanent,
    Internal,
}

impl ErrorClass {
    /// Whether a failure in this class is eligible for retry by default.
    ///
    /// `Transient`, `DependencyUnavailable`, `Timeout` and `ResourceExhausted`
    /// describe conditions that may resolve on their own. The rest describe a
    /// request or configuration problem that will not change on replay.
    pub fn is_retryable_by_default(&self) -> bool {
        matches!(
            self,
            ErrorClass::Transient
                | ErrorClass::DependencyUnavailable
                | ErrorClass::Timeout
                | ErrorClass::ResourceExhausted
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorClass::Validation => "VALIDATION",
            ErrorClass::Authentication => "AUTHENTICATION",
            ErrorClass::Authorization => "AUTHORIZATION",
            ErrorClass::NotFound => "NOT_FOUND",
            ErrorClass::Conflict => "CONFLICT",
            ErrorClass::RateLimited => "RATE_LIMITED",
            ErrorClass::Transient => "TRANSIENT",
            ErrorClass::DependencyUnavailable => "DEPENDENCY_UNAVAILABLE",
            ErrorClass::Timeout => "TIMEOUT",
            ErrorClass::Cancellation => "CANCELLATION",
            ErrorClass::ResourceExhausted => "RESOURCE_EXHAUSTED",
            ErrorClass::Permanent => "PERMANENT",
            ErrorClass::Internal => "INTERNAL",
        }
    }
}

impl std::fmt::Display for ErrorClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum DomainError {
    #[error("Invalid state transition: cannot transition from {from} to {to}")]
    InvalidStateTransition {
        from: String,
        to: String,
    },
    #[error("Validation error: {0}")]
    ValidationError(String),
    /// A workflow graph is not a valid DAG (spec 02.11).
    #[error("Workflow graph is invalid: {0}")]
    InvalidWorkflow(String),
    /// A concurrency limit would be exceeded (spec 01.12).
    #[error("Concurrency limit reached: {0}")]
    ConcurrencyLimitExceeded(String),
    /// Retry policy does not permit another attempt (spec 10.8).
    #[error("Retry not permitted: {0}")]
    RetryNotPermitted(String),
    /// Cross-tenant reference (spec 01.16 invariant 1).
    #[error("Tenant isolation violation: {0}")]
    TenantIsolation(String),
}