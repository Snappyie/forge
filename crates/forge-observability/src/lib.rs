//! Observability: logging, metrics, correlation, and redaction (spec 12).
//!
//! The three invariants this crate exists to uphold:
//!
//! * a request can be traced end to end by its correlation id (AT-OBS-001);
//! * an operator can see queue depth, worker state, and *why* work was
//!   dispatched where it was, without querying the database (AT-OBS-002/003/005);
//! * a secret never reaches a log line (AT-SEC-001, spec 01.22 invariant 11).

pub mod correlation;
pub mod logging;
pub mod metrics;
pub mod redaction;

pub use correlation::{
    resolve_correlation_id, resolve_request_id, CORRELATION_ID_HEADER, REQUEST_ID_HEADER,
};
pub use logging::{init as init_logging, LogFormat};
pub use metrics::{DispatchExplanation, Metrics, MetricsSnapshot, Sample};
pub use redaction::{
    is_sensitive, redact_headers, redact_json, redact_url, redact_value, REDACTED,
};

/// The shared observability state, threaded through the server.
#[derive(Debug, Clone, Default)]
pub struct Observability {
    pub metrics: std::sync::Arc<Metrics>,
}

impl Observability {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience accessor for the metric registry.
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observability_is_shareable_and_defaulted() {
        let obs = Observability::new();
        obs.metrics().increment("x", &[], 1.0);
        assert_eq!(obs.metrics().counter("x", &[]), 1.0);
    }

    /// Spec invariant 11 and AT-SEC-001 together: the redaction surface is
    /// reachable from the top of the crate, so call sites use it by default.
    #[test]
    fn the_redaction_helpers_are_exported() {
        assert_eq!(redact_value("password", "x"), REDACTED);
        assert!(is_sensitive("token"));
    }
}
