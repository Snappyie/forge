use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum DomainError {
    #[error("Invalid state transition: cannot transition from {from} to {to}")]
    InvalidStateTransition {
        from: String,
        to: String,
    },
    #[error("Validation error: {0}")]
    ValidationError(String),
}
